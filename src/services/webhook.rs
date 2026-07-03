use uuid::Uuid;

use crate::{
    errors::{AppError, AppResult},
    models::webhook::{CreateWebhookEndpointRequest, WebhookEndpoint},
};

pub async fn create_endpoint(
    db: &sqlx::PgPool,
    business_id: Uuid,
    req: CreateWebhookEndpointRequest,
) -> AppResult<WebhookEndpoint> {
    // generate a random secret for signing
    let secret = format!("whsec_{}", Uuid::new_v4().to_string().replace("-", ""));

    let endpoint = sqlx::query_as!(
        WebhookEndpoint,
        r#"
        INSERT INTO webhook_endpoints (id, business_id, url, secret, active)
        VALUES ($1, $2, $3, $4, true)
        RETURNING *
        "#,
        Uuid::new_v4(),
        business_id,
        req.url.trim(),
        secret
    )
    .fetch_one(db)
    .await?;

    Ok(endpoint)
}

pub async fn get_endpoint(
    db: &sqlx::PgPool,
    id: Uuid,
    business_id: Uuid,
) -> AppResult<WebhookEndpoint> {
    let endpoint = sqlx::query_as!(
        WebhookEndpoint,
        r#"
        SELECT * FROM webhook_endpoints
        WHERE id = $1 AND business_id = $2
        "#,
        id,
        business_id
    )
    .fetch_optional(db)
    .await?
    .ok_or(AppError::NotFound)?;

    Ok(endpoint)
}

pub async fn list_endpoints(
    db: &sqlx::PgPool,
    business_id: Uuid,
) -> AppResult<Vec<WebhookEndpoint>> {
    let endpoints = sqlx::query_as!(
        WebhookEndpoint,
        r#"
        SELECT * FROM webhook_endpoints
        WHERE business_id = $1
        ORDER BY created_at DESC
        "#,
        business_id
    )
    .fetch_all(db)
    .await?;

    Ok(endpoints)
}

pub async fn delete_endpoint(db: &sqlx::PgPool, id: Uuid, business_id: Uuid) -> AppResult<()> {
    // soft delete — just mark inactive
    let result = sqlx::query!(
        r#"
        UPDATE webhook_endpoints
        SET active = false
        WHERE id = $1 AND business_id = $2
        "#,
        id,
        business_id
    )
    .execute(db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(())
}
