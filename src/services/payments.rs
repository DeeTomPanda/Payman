use crate::{
    errors::{AppError, AppResult},
    models::{
        invoice::InvoiceState,
        payment::PspResult,
        payment::{PayInvoiceRequest, PaymentAttempt, PaymentStatus},
    },
    workers::{payment::call_psp, webhook::dispatch},
};
use anyhow::anyhow;
use uuid::Uuid;

pub async fn pay_invoice(
    db: &sqlx::PgPool,
    psp_url: &str,
    invoice_id: Uuid,
    business_id: Uuid,
    idempotency_key: String,
    invoice_req: PayInvoiceRequest,
) -> AppResult<PaymentAttempt> {
    // check duplicate attempt
    let existing = sqlx::query!(
        r#"
        SELECT response_body FROM idempotency_keys
        WHERE key = $1 AND business_id = $2 AND invoice_id = $3
        AND created_at > NOW() - INTERVAL '24 hours'
        "#,
        idempotency_key,
        business_id,
        invoice_id
    )
    .fetch_optional(db)
    .await?;

    // dont retry with same idempotency key
    if let Some(record) = existing {
        let attempt: PaymentAttempt = serde_json::from_value(record.response_body)
            .map_err(|e| AppError::Internal(anyhow!(e)))?;
        return Ok(attempt);
    }

    let mut tx = db.begin().await?;

    // optimistic locking
    let result = sqlx::query!(
        r#"
    UPDATE invoices
    SET state = $1, updated_at = NOW(), versioning = versioning + 1
    WHERE id = $2 
      AND state = $3
      AND versioning = $4
    "#,
        InvoiceState::Processing as InvoiceState,
        invoice_id,
        InvoiceState::Open as InvoiceState,
        invoice_req.versioning
    )
    .execute(&mut *tx)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::Conflict(format!(
            "another transaction in progress, try later!"
        )));
    }

    // create payment attempt as pending
    let attempt_id = Uuid::new_v4();
    let attempt = sqlx::query_as!(
        PaymentAttempt,
        r#"
        INSERT INTO payment_attempts
        (id, invoice_id, status, card_token)
        VALUES ($1, $2, $3, $4)
        RETURNING id, invoice_id, status as "status: PaymentStatus",
        card_token, psp_reference, failure_code,
        created_at, updated_at
        "#,
        attempt_id,
        invoice_id,
        PaymentStatus::Pending as PaymentStatus,
        invoice_req.card_token,
    )
    .fetch_one(&mut *tx)
    .await?;

    let pending_body =
        serde_json::to_value(&attempt).map_err(|e| AppError::Internal(anyhow!(e)))?;

    sqlx::query!(
        r#"
        INSERT INTO idempotency_keys
        (id, key, business_id, invoice_id, request_path, response_status, response_body)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
        Uuid::new_v4(),
        idempotency_key,
        business_id,
        invoice_id,
        format!("/invoices/{}/pay", invoice_id),
        0,
        pending_body
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    // call psp after commit
    let psp_result = call_psp(psp_url, attempt_id.to_string(), &invoice_req.card_token).await;

    let result = match psp_result {
        PspResult::Succeeded { psp_ref } => {
            // update attempt + invoice in one transaction
            let mut tx2 = db.begin().await?;

            let updated = sqlx::query_as!(
                PaymentAttempt,
                r#"
                UPDATE payment_attempts
                SET status = $1, psp_reference = $2, updated_at = NOW()
                WHERE id = $3
                RETURNING id, invoice_id, status as "status: PaymentStatus",
                card_token, psp_reference, failure_code,
                created_at, updated_at
                "#,
                PaymentStatus::Succeeded as PaymentStatus,
                psp_ref,
                attempt_id
            )
            .fetch_one(&mut *tx2)
            .await?;

            let updated_result = sqlx::query!(
                r#"
                UPDATE invoices
                SET state = $1, updated_at = NOW(), versioning=versioning+1
                WHERE id = $2
                "#,
                InvoiceState::Paid as InvoiceState,
                invoice_id
            )
            .execute(&mut *tx2)
            .await?;

            if updated_result.rows_affected() == 0 {
                tx2.rollback().await?;
                return Err(AppError::Conflict(
                    "invoice state changed unexpectedly".to_string(),
                ));
            }

            let response_body =
                serde_json::to_value(&updated).map_err(|e| AppError::Internal(anyhow!(e)))?;

            sqlx::query!(
                r#"
                UPDATE idempotency_keys
                SET response_body = $1,response_status = $2, updated_at = NOW()
                WHERE key = $3 AND business_id = $4 AND invoice_id= $5
                "#,
                response_body,
                200,
                idempotency_key,
                business_id,
                invoice_id
            )
            .execute(&mut *tx2)
            .await?;

            tx2.commit().await?;

            // fire webhook in background
            let db2 = db.clone();
            let bid = business_id;
            tokio::spawn(async move {
                if let Err(e) = dispatch(&db2, bid, invoice_id, "invoice.paid").await {
                    tracing::error!("webhook dispatch failed: {}", e);
                }
            });
            updated
        }

        PspResult::Failed { code } => {
            let mut tx2 = db.begin().await?;
            let updated = sqlx::query_as!(
                PaymentAttempt,
                r#"
                UPDATE payment_attempts
                SET status = $1, failure_code = $2, updated_at = NOW()
                WHERE id = $3
                RETURNING id, invoice_id, status as "status: PaymentStatus",
                card_token, psp_reference, failure_code,
                created_at, updated_at
                "#,
                PaymentStatus::Failed as PaymentStatus,
                code,
                attempt_id
            )
            .fetch_one(&mut *tx2)
            .await?;

            let updated_result = sqlx::query!(
                r#"
                UPDATE invoices
                SET state = $1,
                updated_at = NOW(),
                versioning=versioning+1
                WHERE id = $2
                "#,
                InvoiceState::Open as InvoiceState,
                invoice_id
            )
            .execute(&mut *tx2)
            .await?;

            if updated_result.rows_affected() == 0 {
                tx2.rollback().await?;
                return Err(AppError::Conflict(
                    "invoice state changed unexpectedly".to_string(),
                ));
            }

            let response_body =
                serde_json::to_value(&updated).map_err(|e| AppError::Internal(anyhow!(e)))?;

            sqlx::query!(
                r#"
                UPDATE idempotency_keys
                SET response_body = $1,response_status = $2, updated_at = NOW()
                WHERE key = $3 AND business_id = $4 AND invoice_id = $5
                "#,
                response_body,
                200,
                idempotency_key,
                business_id,
                invoice_id
            )
            .execute(&mut *tx2)
            .await?;

            tx2.commit().await?;

            let db2 = db.clone();
            let bid = business_id;
            tokio::spawn(async move {
                if let Err(e) = dispatch(&db2, bid, invoice_id, "invoice.payment_failed").await {
                    tracing::error!("webhook dispatch failed: {}", e);
                }
            });
            updated
        }

        // timeout or network error
        PspResult::TimedOut | PspResult::NetworkError => {
            tracing::warn!(
                "PSP call failed for attempt {}, leaving as pending",
                attempt_id
            );

            // store idempotency key with final response
            let response_body =
                serde_json::to_value(&attempt).map_err(|e| AppError::Internal(anyhow!(e)))?;

            sqlx::query!(
                r#"
                UPDATE idempotency_keys
                SET response_body = $1,response_status = $2, updated_at = NOW()
                WHERE key = $3 AND business_id = $4 AND invoice_id = $5
                "#,
                response_body,
                202,
                idempotency_key,
                business_id,
                invoice_id
            )
            .execute(db)
            .await?;

            attempt
        }
    };

    Ok(result)
}
