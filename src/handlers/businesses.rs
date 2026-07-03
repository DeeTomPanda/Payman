use crate::models::business;
use crate::services::businesses::create_business as create_business_service;
use crate::{AppState, errors::AppResult};
use axum::{Json, Router, extract::State, routing::post};
use serde_json::json;
use std::sync::Arc;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/", post(create_business))
}

async fn create_business(
    State(state): State<Arc<AppState>>,
    Json(req): Json<business::CreateBusinessRequest>,
) -> AppResult<Json<serde_json::Value>> {
    if req.name.trim().is_empty() {
        return Err(crate::errors::AppError::BadRequest(
            "name cannot be empty".to_string(),
        ));
    }

    let name = req.name.trim();

    let result = create_business_service(&state.db, name).await?;

    Ok(Json(json!({
        "business_id": result.business_id,
        "api_key": result.api_key,
        "prefix": result.prefix,
        "warning": "save this key now. It will never be shown again."
    })))
}
