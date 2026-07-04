use crate::{
    errors::{AppError, AppResult},
    models::invoice::{
        CreateInvoiceRequest, EditInvoiceRequest, FinalizeInvoiceRequest, Invoice, InvoiceState,
        LineItem, VoidInvoiceRequest,
    },
    workers::webhook::dispatch,
};
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

pub struct InvoiceResult {
    pub invoice: Invoice,
    pub line_items: Vec<LineItem>,
}
pub async fn create_invoice(
    db: &sqlx::PgPool,
    business_id: Uuid,
    req: CreateInvoiceRequest,
) -> AppResult<InvoiceResult> {
    // validate customer
    let _customer = sqlx::query!(
        "SELECT id FROM customers WHERE id = $1 AND business_id = $2",
        req.customer_id,
        business_id
    )
    .fetch_optional(db)
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

    // check due data

    let today = Utc::now().date_naive();
    if req.due_date < today {
        return Err(AppError::BadRequest(
            "due_date cannot be in the past".to_string(),
        ));
    }

    let mut tx = db.begin().await?;

    let invoice_id = Uuid::new_v4();

    let invoice = sqlx::query_as!(
        Invoice,
        r#"
        INSERT INTO invoices (id, business_id, customer_id, state, total_cents, due_date,versioning)
        VALUES ($1, $2, $3, $4, $5, $6,$7)
        RETURNING id, business_id, customer_id, state as "state: InvoiceState",
        total_cents, due_date, created_at, updated_at, versioning
        "#,
        invoice_id,
        business_id,
        req.customer_id,
        InvoiceState::Draft as InvoiceState,
        total_cents,
        req.due_date,
        1
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

    let invoice = InvoiceResult {
        invoice,
        line_items,
    };

    Ok(invoice)
}

pub async fn get_invoice(
    db: &sqlx::PgPool,
    invoice_id: Uuid,
    business_id: Uuid,
) -> AppResult<InvoiceResult> {
    let invoice = sqlx::query_as!(
        Invoice,
        r#"
        SELECT id, business_id, customer_id, state as "state: InvoiceState",
        total_cents, due_date, created_at, updated_at,versioning
        FROM invoices
        WHERE id = $1 AND business_id = $2
        "#,
        invoice_id,
        business_id
    )
    .fetch_optional(db)
    .await?
    .ok_or(AppError::NotFound)?;

    let line_items = sqlx::query_as!(
        LineItem,
        "SELECT * FROM invoice_line_items WHERE invoice_id = $1",
        invoice_id
    )
    .fetch_all(db)
    .await?;

    let invoice = InvoiceResult {
        invoice,
        line_items,
    };

    Ok(invoice)
}

pub async fn edit_invoice(
    db: &sqlx::PgPool,
    invoice_id: Uuid,
    business_id: Uuid,
    req: EditInvoiceRequest,
) -> AppResult<InvoiceResult> {
    // validate line items if provided
    if let Some(ref items) = req.line_items {
        if items.is_empty() {
            return Err(AppError::BadRequest(
                "invoice must have at least one line item".to_string(),
            ));
        }
        for item in items {
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
        }
    }

    // optimistic locking here
    let result = sqlx::query!(
        r#"
    UPDATE invoices
    SET updated_at = NOW(), versioning = versioning +1
    WHERE id = $1
    AND versioning = $2
      AND state = $3
    "#,
        invoice_id,
        req.versioning,
        InvoiceState::Draft as InvoiceState
    )
    .execute(db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::Conflict(format!(
            "invoice is being processed, try later!"
        )));
    }

    // only 1 transaction to edit an invoice
    let mut tx = db.begin().await?;

    let invoice = sqlx::query!(
        r#"
        SELECT id, state as "state: InvoiceState"
        FROM invoices
        WHERE id = $1 AND business_id = $2
        "#,
        invoice_id,
        business_id
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound)?;

    // only draft invoices can be edited
    if invoice.state != InvoiceState::Draft {
        return Err(AppError::InvalidStateTransition(format!(
            "cannot edit invoice in '{:?}' state, must be 'draft'",
            invoice.state
        )));
    }

    // update due_date if provided
    if let Some(due_date) = req.due_date {
        let today = Utc::now().date_naive();

        if due_date < today {
            return Err(AppError::BadRequest(
                "due_date cannot be in the past".to_string(),
            ));
        }
        sqlx::query!(
            r#"
            UPDATE invoices
            SET due_date = $1, updated_at = NOW()
            WHERE id = $2
            "#,
            due_date,
            invoice_id
        )
        .execute(&mut *tx)
        .await?;
    }

    // replace line items if provided
    let line_items = if let Some(items) = req.line_items {
        let total_cents: i64 = items
            .iter()
            .map(|item| item.quantity as i64 * item.unit_amount_cents)
            .sum();

        if total_cents <= 0 {
            return Err(AppError::BadRequest(
                "invoice total must be greater than zero".to_string(),
            ));
        }

        // delete existing line items and replace
        sqlx::query!(
            "DELETE FROM invoice_line_items WHERE invoice_id = $1",
            invoice_id
        )
        .execute(&mut *tx)
        .await?;

        let mut new_items = Vec::new();
        for item in &items {
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
            new_items.push(line_item);
        }

        sqlx::query!(
            r#"
            UPDATE invoices
            SET total_cents = $1, updated_at = NOW()
            WHERE id = $2
            "#,
            total_cents,
            invoice_id
        )
        .execute(&mut *tx)
        .await?;

        new_items
    } else {
        // fetch existing line items
        sqlx::query_as!(
            LineItem,
            "SELECT * FROM invoice_line_items WHERE invoice_id = $1",
            invoice_id
        )
        .fetch_all(&mut *tx)
        .await?
    };

    let updated = sqlx::query_as!(
        Invoice,
        r#"
        SELECT id, business_id, customer_id, state as "state: InvoiceState",
        total_cents, due_date, created_at, updated_at,versioning
        FROM invoices
        WHERE id = $1
        "#,
        invoice_id
    )
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    let invoice = InvoiceResult {
        invoice: updated,
        line_items,
    };

    Ok(invoice)
}

// for filtering by state
#[derive(Deserialize)]
pub struct ListInvoicesQuery {
    pub state: Option<InvoiceState>,
}

pub async fn list_invoices(
    db: &sqlx::PgPool,
    business_id: Uuid,
    query: ListInvoicesQuery,
) -> AppResult<Vec<Invoice>> {
    let invoices = match query.state {
        Some(filter_state) => {
            sqlx::query_as!(
                Invoice,
                r#"
                SELECT id, business_id, customer_id, state as "state: InvoiceState",
                total_cents, due_date, created_at, updated_at,versioning
                FROM invoices
                WHERE business_id = $1 AND state = $2
                ORDER BY created_at DESC
                "#,
                business_id,
                filter_state as InvoiceState
            )
            .fetch_all(db)
            .await?
        }
        None => {
            sqlx::query_as!(
                Invoice,
                r#"
                SELECT id, business_id, customer_id, state as "state: InvoiceState",
                total_cents, due_date, created_at, updated_at,versioning
                FROM invoices
                WHERE business_id = $1
                ORDER BY created_at DESC
                "#,
                business_id
            )
            .fetch_all(db)
            .await?
        }
    };

    Ok(invoices)
}

pub async fn finalize_invoice(
    db: &sqlx::PgPool,
    invoice_id: Uuid,
    business_id: Uuid,
    req: FinalizeInvoiceRequest,
) -> AppResult<Invoice> {
    let mut tx = db.begin().await?;

    let invoice = sqlx::query_as!(
        Invoice,
        r#"
        SELECT id, business_id, customer_id,
        state as "state: InvoiceState",
        total_cents, due_date, created_at, updated_at, versioning
        FROM invoices
        WHERE id = $1 AND business_id = $2
        "#,
        invoice_id,
        business_id
    )
    .fetch_one(&mut *tx)
    .await?;

    let target_state = InvoiceState::Open;

    if !invoice.state.can_transition_to(&target_state) {
        return Err(AppError::InvalidStateTransition(format!(
            "cannot transition invoice from {:?} to {:?}",
            invoice.state, target_state
        )));
    }

    let updated = sqlx::query_as!(
        Invoice,
        r#"
        UPDATE invoices
        SET state = $1,
            updated_at = NOW(),
            versioning = versioning + 1
        WHERE id = $2
            AND business_id = $3
            AND versioning = $4
        RETURNING id, business_id, customer_id,
        state as "state: InvoiceState",
        total_cents, due_date, created_at, updated_at, versioning
        "#,
        target_state as InvoiceState,
        invoice_id,
        business_id,
        req.versioning
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::Conflict("stale invoice version".into()))?;

    tx.commit().await?;

    let db2 = db.clone();
    let bid = business_id;

    // fire webhook
    tokio::spawn(async move {
        if let Err(e) = dispatch(&db2, bid, invoice_id, "invoice.finalized").await {
            tracing::error!("webhook dispatch failed: {}", e);
        }
    });

    Ok(updated)
}

pub async fn void_invoice(
    db: &sqlx::PgPool,
    invoice_id: Uuid,
    business_id: Uuid,
    req: VoidInvoiceRequest,
) -> AppResult<Invoice> {
    let mut tx = db.begin().await?;

    let invoice = sqlx::query!(
        r#"
        SELECT state as "state: InvoiceState"
        FROM invoices
        WHERE id = $1 AND business_id = $2 AND versioning = $3
        "#,
        invoice_id,
        business_id,
        req.versioning
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound)?;

    let target_state = InvoiceState::Open;

    if !invoice.state.can_transition_to(&target_state) {
        return Err(AppError::InvalidStateTransition(format!(
            "cannot transition invoice from {:?} to {:?}",
            invoice.state, target_state
        )));
    }

    let updated = sqlx::query_as!(
        Invoice,
        r#"
        UPDATE invoices
        SET state = $1, updated_at = NOW(), versioning=versioning+1
        WHERE id = $2 AND business_id = $3 
        AND versioning = $4
        RETURNING id, business_id, customer_id,
        state as "state: InvoiceState",
        total_cents, due_date, created_at, updated_at, versioning
        "#,
        InvoiceState::Void as InvoiceState,
        invoice_id,
        business_id,
        req.versioning
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::Conflict("stale invoice version".into()))?;

    tx.commit().await?;

    Ok(updated)
}
