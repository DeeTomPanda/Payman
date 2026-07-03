use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};

use std::sync::Arc;
use uuid::Uuid;

use crate::{
    AppState,
    errors::{AppError, AppResult},
    middleware::auth::AuthenticatedBusiness,
    models::invoice::{
        CreateInvoiceRequest, EditInvoiceRequest, FinalizeInvoiceRequest, Invoice, InvoiceResponse,
        VoidInvoiceRequest,
    },
    services::invoices::{
        ListInvoicesQuery, create_invoice as create_invoice_service,
        edit_invoice as edit_invoice_service, finalize_invoice as finalize_invoice_service,
        get_invoice as get_invoice_service, list_invoices as list_invoices_service,
        void_invoice as void_invoice_service,
    },
};

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", post(create_invoice).get(list_invoices))
        .route("/{id}", get(get_invoice).patch(edit_invoice))
        .route("/{id}/finalize", post(finalize_invoice)) // add this
        .route("/{id}/void", post(void_invoice)) // and this
}

async fn create_invoice(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthenticatedBusiness>,
    Json(req): Json<CreateInvoiceRequest>,
) -> AppResult<Json<InvoiceResponse>> {
    // validate line items exist
    if req.line_items.is_empty() {
        return Err(AppError::BadRequest(
            "invoice must have at least one line item".to_string(),
        ));
    }

    let invoice_result = create_invoice_service(&state.db, auth.business.id, req).await?;

    Ok(Json(InvoiceResponse {
        invoice: invoice_result.invoice,
        line_items: invoice_result.line_items,
    }))
}

async fn edit_invoice(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthenticatedBusiness>,
    Path(id): Path<Uuid>,
    Json(req): Json<EditInvoiceRequest>,
) -> AppResult<Json<InvoiceResponse>> {
    let updated = edit_invoice_service(&state.db, id, auth.business.id, req).await?;
    Ok(Json(InvoiceResponse {
        invoice: updated.invoice,
        line_items: updated.line_items,
    }))
}

async fn get_invoice(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthenticatedBusiness>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<InvoiceResponse>> {
    let invoice_result = get_invoice_service(&state.db, id, auth.business.id).await?;
    Ok(Json(InvoiceResponse {
        invoice: invoice_result.invoice,
        line_items: invoice_result.line_items,
    }))
}

async fn list_invoices(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthenticatedBusiness>,
    Query(query): Query<ListInvoicesQuery>,
) -> AppResult<Json<Vec<Invoice>>> {
    let invoices = list_invoices_service(&state.db, auth.business.id, query).await?;
    Ok(Json(invoices))
}

async fn finalize_invoice(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthenticatedBusiness>,
    Path(id): Path<Uuid>,
    Json(req): Json<FinalizeInvoiceRequest>,
) -> AppResult<Json<Invoice>> {
    let finalized = finalize_invoice_service(&state.db, id, auth.business.id, req).await?;
    Ok(Json(finalized))
}

async fn void_invoice(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthenticatedBusiness>,
    Path(id): Path<Uuid>,
    Json(req): Json<VoidInvoiceRequest>,
) -> AppResult<Json<Invoice>> {
    let voided = void_invoice_service(&state.db, id, auth.business.id, req).await?;
    Ok(Json(voided))
}
