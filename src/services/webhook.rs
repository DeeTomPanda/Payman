use anyhow::Result;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use sqlx::PgPool;
use uuid::Uuid;
use serde_json::json;

type HmacSha256 = Hmac<Sha256>;

// simply make entry which will be picked up by a webhook-worker
pub async fn dispatch(
    db: &PgPool,
    business_id: Uuid,
    invoice_id: Uuid,
    event_type: &str,
) -> Result<()> {
    // get all active webhook endpoints for this business
    let endpoints = sqlx::query!(
        r#"
        SELECT id, url, secret
        FROM webhook_endpoints
        WHERE business_id = $1 AND active = true
        "#,
        business_id
    )
    .fetch_all(db)
    .await?;

    if endpoints.is_empty() {
        tracing::info!("no webhook endpoints for business {}", business_id);
        return Ok(());
    }

    // build payload
    let payload = json!({
        "event": event_type,
        "invoice_id": invoice_id,
        "business_id": business_id,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });

    // create a delivery record for each endpoint
    for endpoint in endpoints {
        sqlx::query!(
            r#"
            INSERT INTO webhook_deliveries
            (id, webhook_endpoint_id, event_type, payload, status, next_retry_at)
            VALUES ($1, $2, $3, $4, 'pending'::delivery_status, NOW())
            "#,
            Uuid::new_v4(),
            endpoint.id,
            event_type,
            payload
        )
        .execute(db)
        .await?;
    }

    Ok(())
}

// signs the payload with HMAC-SHA256
pub fn sign_payload(secret: &str, payload: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("HMAC can take key of any size");
    mac.update(payload.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

// actually sends the webhook to the business's url
pub async fn deliver(
    db: &PgPool,
    delivery_id: Uuid,
    url: &str,
    secret: &str,
    payload: &serde_json::Value,
) -> Result<()> {
    let payload_str = serde_json::to_string(payload)?;
    let signature = sign_payload(secret, &payload_str);

    let client = reqwest::Client::new();

    let response = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        client
            .post(url)
            .header("Content-Type", "application/json")
            .header("X-Webhook-Signature", format!("sha256={}", signature))
            .body(payload_str)
            .send(),
    )
    .await;

    let success = match response {
        Ok(Ok(r)) => r.status().is_success(),
        _ => false,
    };

    if success {
        // mark delivered
        sqlx::query!(
            r#"
            UPDATE webhook_deliveries
            SET status = 'delivered',
                attempt_count = attempt_count + 1,
                updated_at = NOW()
            WHERE id = $1
            "#,
            delivery_id
        )
        .execute(db)
        .await?;
    } else {
        // increment attempt, schedule retry with exponential backoff
        sqlx::query!(
            r#"
            UPDATE webhook_deliveries
            SET attempt_count = attempt_count + 1,
                next_retry_at = NOW() + (INTERVAL '1 minute' * POWER(2, attempt_count)),
                status = CASE
                    WHEN attempt_count >= 4 THEN 'failed'::delivery_status
                    ELSE 'pending'::delivery_status
                END,
                updated_at = NOW()
            WHERE id = $1
            "#,
            delivery_id
        )
        .execute(db)
        .await?;
    }

    Ok(())
}