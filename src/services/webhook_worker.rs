use sqlx::PgPool;
use std::time::Duration;
use crate::services::webhook::deliver;

pub fn start_webhook_worker(db: PgPool) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            if let Err(e) = process_pending_deliveries(&db).await {
                tracing::error!("webhook worker error: {}", e);
            }
        }
    });
}

async fn process_pending_deliveries(db: &PgPool) -> anyhow::Result<()> {
    // pick up all pending deliveries that are due
    let deliveries = sqlx::query!(
        r#"
        SELECT
            wd.id,
            wd.payload,
            wd.attempt_count,
            we.url,
            we.secret
        FROM webhook_deliveries wd
        JOIN webhook_endpoints we ON we.id = wd.webhook_endpoint_id
        WHERE wd.status = 'pending'
          AND wd.next_retry_at <= NOW()
        LIMIT 10
        "#
    )
    .fetch_all(db)
    .await?;

    for delivery in deliveries {
        tracing::info!(
            "delivering webhook {} attempt {}",
            delivery.id,
            delivery.attempt_count
        );

        if let Err(e) = deliver(
            db,
            delivery.id,
            &delivery.url,
            &delivery.secret,
            &delivery.payload,
        )
        .await
        {
            tracing::error!("delivery failed for {}: {}", delivery.id, e);
        }
    }

    Ok(())
}

