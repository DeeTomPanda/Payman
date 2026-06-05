use axum::{extract::State, Json, routing::post, Router};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;
use crate::{AppState, errors::AppResult};
use crate::services::api_key::generate_api_key;

#[derive(serde::Deserialize)]
pub struct CreateBusinessRequest {
    pub name: String,
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", post(create_business))
}

async fn create_business(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateBusinessRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let business_id = Uuid::new_v4();
    let api_key = generate_api_key();
    
    sqlx::query!(
        "INSERT INTO businesses (id, name) VALUES ($1, $2)",
        business_id,
        req.name
    )
    .execute(&state.db)
    .await?;

    sqlx::query!(
        "INSERT INTO api_keys (id, business_id, key_hash, key_prefix)
         VALUES ($1, $2, $3, $4)",
        Uuid::new_v4(),
        business_id,
        api_key.hash,
        api_key.prefix
    )
    .execute(&state.db)
    .await?;

    Ok(Json(json!({
        "business_id": business_id,
        "api_key": api_key.raw,
        "prefix": api_key.prefix,
        "warning": "Save this key now. It will never be shown again."
    })))
}