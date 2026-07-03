use std::sync::Arc;
use tokio::time::{Duration, sleep};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{
    workers::webhook::dispatch,
    AppState,
    models::{invoice::InvoiceState},
};

#[derive(Debug, serde::Deserialize)]
struct PspOutcome {
    status: String,
    psp_ref: Option<String>,
    code: Option<String>,
}

pub fn start(state: Arc<AppState>) {
    tokio::spawn(async move {
        loop {
            if let Err(e) = tick(&state).await {
                error!("reconciliation worker error: {}", e);
            }
            sleep(Duration::from_secs(60)).await;
        }
    });
}

async fn tick(state: &Arc<AppState>) -> anyhow::Result<()> {
    let attempts = sqlx::query!(
        r#"
        SELECT id, invoice_id, retry_count, created_at
        FROM payment_attempts
        WHERE status = 'pending'
        AND (next_retry_at IS NULL OR next_retry_at <= NOW())
        AND created_at > NOW() - INTERVAL '24 hours'
        "#
    )
    .fetch_all(&state.db)
    .await?;

    info!("reconciliation tick: {} pending attempts", attempts.len());

    for attempt in attempts {
        if let Err(e) =
            reconcile_attempt(state, attempt.id, attempt.invoice_id, attempt.retry_count).await
        {
            error!("failed to reconcile attempt {}: {}", attempt.id, e);
        }
    }

    // mark attempts older than 24h as failed, revert invoice to Open
    let expired = sqlx::query!(
        r#"
        SELECT id, invoice_id FROM payment_attempts
        WHERE status = 'pending'
        AND created_at <= NOW() - INTERVAL '24 hours'
        "#
    )
    .fetch_all(&state.db)
    .await?;

    for attempt in expired {
        warn!("attempt {} expired after 24h, marking failed", attempt.id);
        let mut tx= state.db.begin().await?; 
        sqlx::query!(
            r#"
            UPDATE payment_attempts
            SET status = 'failed',
                failure_code = 'reconciliation_timeout',
                updated_at = NOW()
            WHERE id = $1
            "#,
            attempt.id
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query!(
            r#"
            UPDATE invoices
            SET state = $1, updated_at = NOW()
            WHERE id = $2
            "#,
            InvoiceState::Open as InvoiceState,
            attempt.invoice_id
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
    }


    Ok(())
}

async fn reconcile_attempt(
    state: &Arc<AppState>,
    attempt_id: Uuid,
    invoice_id: Uuid,
    retry_count: i32,
) -> anyhow::Result<()> {
    let url = format!("{}/charge/{}", state.psp_url, attempt_id);
    let resp = tokio::time::timeout(Duration::from_secs(5), reqwest::get(&url)).await;

    match resp {
        Err(_)=>{
            warn!("timeout occured for attempt {}",attempt_id)
        }, 
        Ok(Err(e)) => {
            // network error
            warn!("PSP status check failed for attempt {}: {}", attempt_id, e);
            return Ok(());
        }
        Ok(Ok(response)) => {
            match response.status() {
                // PSP still processing, schedule next retry with exponential backoff
                s if s == reqwest::StatusCode::NOT_FOUND => {
                    // 1 min, 2 min, 4 min, 8 min, cap at 16 min
                    let backoff_mins = 2_i64.pow(retry_count as u32).min(16);
                    sqlx::query!(
                        r#"
                        UPDATE payment_attempts
                        SET retry_count = retry_count + 1,
                        next_retry_at = NOW() + make_interval(mins => $1),
                        updated_at = NOW()
                        WHERE id = $2
                        "#,
                        backoff_mins as i32,
                        attempt_id
                    )
                    .execute(&state.db)
                    .await?;
                    info!(
                        "attempt {} still pending, next retry in {}min",
                        attempt_id, backoff_mins
                    );
                }

                s if s.is_success() => {
                    let outcome = response.json::<PspOutcome>().await?;
                    let mut tx = state.db.begin().await?;

                    match outcome.status.as_str() {
                        "succeeded" => {
                            sqlx::query!(
                            r#"
                            UPDATE payment_attempts
                            SET status = 'succeeded',
                            psp_reference = $1,
                            updated_at = NOW()
                            WHERE id = $2
                            "#,
                            outcome.psp_ref,
                            attempt_id
                            )
                            .execute(&mut *tx)
                            .await?;

                            sqlx::query!(
                            r#"
                            UPDATE invoices
                            SET state = $1, updated_at = NOW()
                            WHERE id = $2 AND state = $3
                            "#,
                            InvoiceState::Paid as InvoiceState,
                            invoice_id,
                            InvoiceState::Processing as InvoiceState
                            )
                            .execute(&mut *tx)
                            .await?;

                            tx.commit().await?;

                            // fire webhook
                            let db2 = state.db.clone();
                            let bid = get_business_id(&state.db, invoice_id).await?;
                            tokio::spawn(async move {
                                if let Err(e) = dispatch(
                                    &db2,
                                    bid,
                                    invoice_id,
                                    "invoice.paid",
                                )
                                .await
                                {
                                    error!("webhook dispatch failed: {}", e);
                                }
                            });

                            info!("attempt {} reconciled as succeeded", attempt_id);
                        }

                        "failed" => {
                            sqlx::query!(
                            r#"
                            UPDATE payment_attempts
                            SET status = 'failed',
                            failure_code = $1,
                            updated_at = NOW()
                            WHERE id = $2
                            "#,
                            outcome.code,
                            attempt_id
                            )
                            .execute(&mut *tx)
                            .await?;

                            sqlx::query!(
                            r#"
                            UPDATE invoices
                            SET state = $1, updated_at = NOW()
                            WHERE id = $2 AND state = $3
                            "#,
                            InvoiceState::Open as InvoiceState,
                            invoice_id,
                            InvoiceState::Processing as InvoiceState
                            )
                            .execute(&mut *tx)
                            .await?;

                            tx.commit().await?;

                            let db2 = state.db.clone();
                            let bid = get_business_id(&state.db, invoice_id).await?;
                            tokio::spawn(async move {
                                if let Err(e) = dispatch(
                                    &db2,
                                    bid,
                                    invoice_id,
                                    "invoice.payment_failed",
                                )
                                .await
                                {
                                    error!("webhook dispatch failed: {}", e);
                                }
                            });

                            info!("attempt {} reconciled as failed", attempt_id);
                        }

                        _ => {
                            warn!(
                                "unexpected PSP status for attempt {}: {}",
                                attempt_id, outcome.status
                            );
                        }
                    }
                }

                s => {
                    warn!("unexpected PSP response for attempt {}: {}", attempt_id, s);
                }
            }
        }
    }

    Ok(())
}

async fn get_business_id(db: &sqlx::PgPool, invoice_id: Uuid) -> anyhow::Result<Uuid> {
    let row = sqlx::query!("SELECT business_id FROM invoices WHERE id = $1", invoice_id)
        .fetch_one(db)
        .await?;
    Ok(row.business_id)
}
