use axum::{
    extract::{Path, State},
    Extension,
    Json,
    Router,
    routing::{get, post},
};
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    AppState,
    errors::{AppError, AppResult},
    middleware::auth::AuthenticatedBusiness,
    models::webhook::{CreateWebhookEndpointRequest, WebhookEndpoint},
};

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", post(create_endpoint).get(list_endpoints))
        .route("/{id}", get(get_endpoint).delete(delete_endpoint))
}

async fn create_endpoint(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthenticatedBusiness>,
    Json(req): Json<CreateWebhookEndpointRequest>,
) -> AppResult<Json<WebhookEndpoint>> {
    // validate url
    if req.url.trim().is_empty() {
        return Err(AppError::BadRequest("url cannot be empty".to_string()));
    }
    if !req.url.starts_with("http://") && !req.url.starts_with("https://") {
        return Err(AppError::BadRequest(
            "url must start with http:// or https://".to_string()
        ));
    }

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
        auth.business.id,
        req.url.trim(),
        secret
    )
    .fetch_one(&state.db)
    .await?;

    Ok(Json(endpoint))
}

async fn get_endpoint(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthenticatedBusiness>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<WebhookEndpoint>> {
    let endpoint = sqlx::query_as!(
        WebhookEndpoint,
        r#"
        SELECT * FROM webhook_endpoints
        WHERE id = $1 AND business_id = $2
        "#,
        id,
        auth.business.id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    Ok(Json(endpoint))
}

async fn list_endpoints(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthenticatedBusiness>,
) -> AppResult<Json<Vec<WebhookEndpoint>>> {
    let endpoints = sqlx::query_as!(
        WebhookEndpoint,
        r#"
        SELECT * FROM webhook_endpoints
        WHERE business_id = $1
        ORDER BY created_at DESC
        "#,
        auth.business.id
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(endpoints))
}

async fn delete_endpoint(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthenticatedBusiness>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    // soft delete — just mark inactive
    let result = sqlx::query!(
        r#"
        UPDATE webhook_endpoints
        SET active = false
        WHERE id = $1 AND business_id = $2
        "#,
        id,
        auth.business.id
    )
    .execute(&state.db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(Json(serde_json::json!({ "deleted": true })))
}