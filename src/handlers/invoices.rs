use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    AppState,
    errors::{AppError, AppResult},
    middleware::auth::AuthenticatedBusiness,
    models::invoice::{CreateInvoiceRequest, Invoice, InvoiceResponse, InvoiceState, LineItem},
};

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", post(create_invoice).get(list_invoices))
        .route("/{id}", get(get_invoice))
        .route("/{id}/finalize", post(finalize_invoice)) // add this
        .route("/{id}/void", post(void_invoice)) // and this
}

// for filtering by state
#[derive(Deserialize)]
pub struct ListInvoicesQuery {
    pub state: Option<InvoiceState>,
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

    // validate customer
    let _customer = sqlx::query!(
        "SELECT id FROM customers WHERE id = $1 AND business_id = $2",
        req.customer_id,
        auth.business.id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    // compute total 
    let total_cents: i64 = req
        .line_items
        .iter()
        .map(|item| item.quantity as i64 * item.unit_amount_cents)
        .sum();

    if total_cents <= 0 {
        return Err(AppError::BadRequest(
            "invoice total must be greater than zero".to_string(),
        ));
    }

    let mut tx = state.db.begin().await?;

    let invoice_id = Uuid::new_v4();

    let invoice = sqlx::query_as!(
        Invoice,
        r#"
        INSERT INTO invoices (id, business_id, customer_id, state, total_cents, due_date)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, business_id, customer_id, state as "state: InvoiceState",
        total_cents, due_date, created_at, updated_at
        "#,
        invoice_id,
        auth.business.id,
        req.customer_id,
        InvoiceState::Draft as InvoiceState,
        total_cents,
        req.due_date
    )
    .fetch_one(&mut *tx)
    .await?;

    // insert all line items
    let mut line_items = Vec::new();

    for item in &req.line_items {
        if item.quantity <= 0 {
            return Err(AppError::BadRequest(
                "quantity must be greater than zero".to_string(),
            ));
        }
        if item.unit_amount_cents <= 0 {
            return Err(AppError::BadRequest(
                "unit_amount_cents must be greater than zero".to_string(),
            ));
        }

        let line_item = sqlx::query_as!(
            LineItem,
            r#"
            INSERT INTO invoice_line_items
            (id, invoice_id, description, quantity, unit_amount_cents)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            "#,
            Uuid::new_v4(),
            invoice_id,
            item.description.trim(),
            item.quantity,
            item.unit_amount_cents
        )
        .fetch_one(&mut *tx)
        .await?;

        line_items.push(line_item);
    }

    tx.commit().await?;

    Ok(Json(InvoiceResponse {
        invoice,
        line_items,
    }))
}

async fn get_invoice(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthenticatedBusiness>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<InvoiceResponse>> {
    let invoice = sqlx::query_as!(
        Invoice,
        r#"
        SELECT id, business_id, customer_id, state as "state: InvoiceState",
        total_cents, due_date, created_at, updated_at
        FROM invoices
        WHERE id = $1 AND business_id = $2
        "#,
        id,
        auth.business.id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    let line_items = sqlx::query_as!(
        LineItem,
        "SELECT * FROM invoice_line_items WHERE invoice_id = $1",
        id
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(InvoiceResponse {
        invoice,
        line_items,
    }))
}

async fn list_invoices(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthenticatedBusiness>,
    Query(query): Query<ListInvoicesQuery>,
) -> AppResult<Json<Vec<Invoice>>> {
    let invoices = match query.state {
        Some(filter_state) => {
            sqlx::query_as!(
                Invoice,
                r#"
                SELECT id, business_id, customer_id, state as "state: InvoiceState",
                total_cents, due_date, created_at, updated_at
                FROM invoices
                WHERE business_id = $1 AND state = $2
                ORDER BY created_at DESC
                "#,
                auth.business.id,
                filter_state as InvoiceState
            )
            .fetch_all(&state.db)
            .await?
        }
        None => {
            sqlx::query_as!(
                Invoice,
                r#"
                SELECT id, business_id, customer_id, state as "state: InvoiceState",
                total_cents, due_date, created_at, updated_at
                FROM invoices
                WHERE business_id = $1
                ORDER BY created_at DESC
                "#,
                auth.business.id
            )
            .fetch_all(&state.db)
            .await?
        }
    };

    Ok(Json(invoices))
}

async fn finalize_invoice(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthenticatedBusiness>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Invoice>> {

    let mut tx= state.db.begin().await?;
    let invoice = sqlx::query!(
        r#"
        SELECT state as "state: InvoiceState"
        FROM invoices
        WHERE id = $1 AND business_id = $2
        "#,
        id,
        auth.business.id
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound)?;

    if !invoice.state.can_transition_to(&InvoiceState::Open) {
        return Err(AppError::InvalidStateTransition(format!(
            "cannot finalize invoice in '{}' state",
            serde_json::to_string(&invoice.state).unwrap_or_default()
        )));
    }

    let updated = sqlx::query_as!(
        Invoice,
        r#"
        UPDATE invoices
        SET state = $1, updated_at = NOW()
        WHERE id = $2 AND business_id = $3
        RETURNING id, business_id, customer_id,
        state as "state: InvoiceState",
        total_cents, due_date, created_at, updated_at
        "#,
        InvoiceState::Open as InvoiceState,
        id,
        auth.business.id
    )
    .fetch_one(&mut *tx)
    .await?;

    // fire webhook
    let db = state.db.clone();
    let bid = auth.business.id;
    tokio::spawn(async move {
        if let Err(e) = crate::services::webhook::dispatch(&db, bid, id, "invoice.created").await {
            tracing::error!("webhook dispatch failed: {}", e);
        }
    });

    tx.commit().await?;

    Ok(Json(updated))
}

async fn void_invoice(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthenticatedBusiness>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Invoice>> {
    let mut tx = state.db.begin().await?;
    
    let invoice = sqlx::query!(
        r#"
        SELECT state as "state: InvoiceState"
        FROM invoices
        WHERE id = $1 AND business_id = $2
        "#,
        id,
        auth.business.id
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound)?;

    if !invoice.state.can_transition_to(&InvoiceState::Void) {
        return Err(AppError::InvalidStateTransition(format!(
            "cannot void invoice in '{}' state",
            serde_json::to_string(&invoice.state).unwrap_or_default()
        )));
    }

    let updated = sqlx::query_as!(
        Invoice,
        r#"
        UPDATE invoices
        SET state = $1, updated_at = NOW()
        WHERE id = $2 AND business_id = $3
        RETURNING id, business_id, customer_id,
        state as "state: InvoiceState",
        total_cents, due_date, created_at, updated_at
        "#,
        InvoiceState::Void as InvoiceState,
        id,
        auth.business.id
    )
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(Json(updated))
}
