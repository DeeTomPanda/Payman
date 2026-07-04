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
    models::payment::{PayInvoiceRequest, PaymentAttempt},
    services::payments::pay_invoice as pay_invoice_service,
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

    let attempt = pay_invoice_service(
        &state.db,
        state.psp_url.as_str(),
        invoice_id,
        auth.business.id,
        idempotency_key,
        req,
    )
    .await?;

    Ok(Json(attempt))
}
