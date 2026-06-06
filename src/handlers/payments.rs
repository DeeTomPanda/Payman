use anyhow::anyhow;
use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    routing::post,
};
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    AppState,
    errors::{AppError, AppResult},
    middleware::auth::AuthenticatedBusiness,
    models::{
        invoice::InvoiceState,
        payment::{PayInvoiceRequest, PaymentAttempt, PaymentStatus},
    },
    services::payment::{PspResult, call_psp},
};

pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/{id}/pay", post(pay_invoice))
}

async fn pay_invoice(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthenticatedBusiness>,
    Path(invoice_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
    Json(req): Json<PayInvoiceRequest>,
) -> AppResult<Json<PaymentAttempt>> {

    // extract idempotency key
    let idempotency_key = headers
        .get("Idempotency-Key")
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::BadRequest(
            "Idempotency-Key header is required".to_string(),
        ))?
        .to_string();

    // check duplicate attempt
    let existing = sqlx::query!(
        r#"
        SELECT response_body FROM idempotency_keys
        WHERE key = $1 AND business_id = $2
        AND created_at > NOW() - INTERVAL '24 hours'
        "#,
        idempotency_key,
        auth.business.id
    )
    .fetch_optional(&state.db)
    .await?;

    // dont retry with same idempotency key
    if let Some(record) = existing {
        let attempt: PaymentAttempt = serde_json::from_value(record.response_body)
            .map_err(|e| AppError::Internal(anyhow!(e)))?;
        return Ok(Json(attempt));
    }

    // use pessimistic lock
    let mut tx = state.db.begin().await?;

    let invoice = sqlx::query!(
        r#"
        SELECT id, state as "state: InvoiceState", total_cents, business_id
        FROM invoices
        WHERE id = $1 AND business_id = $2
        FOR UPDATE
        "#,
        invoice_id,
        auth.business.id
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound)?;

    if invoice.state != InvoiceState::Open {
        return Err(AppError::InvalidStateTransition(format!(
            "cannot pay an invoice in '{:?}' state, must be 'open'",
            invoice.state
        )));
    }

    // update invoice to processing now
    sqlx::query!(
        r#"
                UPDATE invoices
                SET state = $1, 
                updated_at = NOW()
                WHERE id = $2
                "#,
        InvoiceState::Processing as InvoiceState,
        invoice_id
    )
    .execute(&mut *tx)
    .await?;

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
        req.card_token,
    )
    .fetch_one(&mut *tx)
    .await?;

    let pending_body = serde_json::to_value(&attempt)
        .map_err(|e| AppError::Internal(anyhow!(e)))?;
 
    sqlx::query!(
        r#"
        INSERT INTO idempotency_keys
        (id, key, business_id, request_path, response_status, response_body)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (key, business_id) DO NOTHING
        "#,
        Uuid::new_v4(),
        idempotency_key,
        auth.business.id,
        format!("/invoices/{}/pay", invoice_id),
        0,
        pending_body
    )
    .execute(&mut *tx)
    .await?;


    tx.commit().await?;

    // call psp after commit
    let psp_result = call_psp(&state.psp_url, &req.card_token, invoice.total_cents).await;

    let (final_attempt,pay_response) = match psp_result {
        PspResult::Succeeded { psp_ref } => {
            // update attempt + invoice in one transaction
            let mut tx2 = state.db.begin().await?;

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

            sqlx::query!(
                r#"
                UPDATE invoices
                SET state = $1, updated_at = NOW()
                WHERE id = $2
                "#,
                InvoiceState::Paid as InvoiceState,
                invoice_id
            )
            .execute(&mut *tx2)
            .await?;

            tx2.commit().await?;

            // fire webhook in background
            let db = state.db.clone();
            let bid = auth.business.id;
            tokio::spawn(async move {
                if let Err(e) =
                    crate::services::webhook::dispatch(&db, bid, invoice_id, "invoice.paid").await
                {
                    tracing::error!("webhook dispatch failed: {}", e);
                }
            });

            (updated,false) // clear response
        }

        PspResult::Failed { code } => {
            let mut tx2 = state.db.begin().await?;
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

            sqlx::query!(
                r#"
                UPDATE invoices
                SET state = $1,
                updated_at = NOW()
                WHERE id = $2
                "#,
                InvoiceState::Open as InvoiceState,
                invoice_id
            )
            .execute(&mut *tx2)
            .await?;

            tx2.commit().await?;

            let db = state.db.clone();
            let bid = auth.business.id;
            tokio::spawn(async move {
                if let Err(e) = crate::services::webhook::dispatch(
                    &db,
                    bid,
                    invoice_id,
                    "invoice.payment_failed",
                )
                .await
                {
                    tracing::error!("webhook dispatch failed: {}", e);
                }
            });

            (updated,false) // clear response
        }

        // timeout or network error
        PspResult::TimedOut | PspResult::NetworkError => {
            tracing::warn!(
                "PSP call failed for attempt {}, leaving as pending",
                attempt_id
            );

            (attempt,true) // unclear response
        }
    };

    // store idempotency key with final response
    let response_body =
        serde_json::to_value(&final_attempt).map_err(|e| AppError::Internal(anyhow!(e)))?;

    let final_status = if pay_response { 202i32 } else { 200i32 };

    sqlx::query!(
        r#"
        UPDATE idempotency_keys
        SET response_body = $1,response_status = $2, updated_at = NOW()
        WHERE key = $3 AND business_id = $4
        "#,
        response_body,
        final_status,
        idempotency_key,
        auth.business.id,
    )
    .execute(&state.db)
    .await?;


    Ok(Json(final_attempt))
}
