use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    AppState,
    errors::{AppError, AppResult},
    middleware::auth::AuthenticatedBusiness,
    models::webhook::{CreateWebhookEndpointRequest, WebhookEndpoint},
    services::webhook::{
        create_endpoint as create_endpoint_service, delete_endpoint as delete_endpoint_service,
        get_endpoint as get_endpoint_service, list_endpoints as list_endpoints_service,
    },
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
            "url must start with http:// or https://".to_string(),
        ));
    }

    let endpoint = create_endpoint_service(&state.db, auth.business.id, req).await?;
    Ok(Json(endpoint))
}

async fn get_endpoint(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthenticatedBusiness>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<WebhookEndpoint>> {
    let endpoint = get_endpoint_service(&state.db, id, auth.business.id).await?;
    Ok(Json(endpoint))
}

async fn list_endpoints(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthenticatedBusiness>,
) -> AppResult<Json<Vec<WebhookEndpoint>>> {
    let endpoints = list_endpoints_service(&state.db, auth.business.id).await?;
    Ok(Json(endpoints))
}

async fn delete_endpoint(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthenticatedBusiness>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    delete_endpoint_service(&state.db, id, auth.business.id).await?;

    Ok(Json(serde_json::json!({ "deleted": true })))
}
