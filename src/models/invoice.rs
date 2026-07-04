use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq)]
#[sqlx(type_name = "invoice_state", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum InvoiceState {
    Draft,
    Open,
    Paid,
    Void,
    Uncollectible,
    Processing,
}

impl InvoiceState {
    // state machine logic
    pub fn can_transition_to(&self, next: &InvoiceState) -> bool {
        match (self, next) {
            (InvoiceState::Draft, InvoiceState::Open) => true,
            (InvoiceState::Draft, InvoiceState::Void) => true,
            (InvoiceState::Open, InvoiceState::Processing) => true,

            (InvoiceState::Processing, InvoiceState::Paid) => true,
            (InvoiceState::Processing, InvoiceState::Open) => false,
            (InvoiceState::Processing, InvoiceState::Uncollectible) => true,

            (InvoiceState::Open, InvoiceState::Void) => true,
            (InvoiceState::Open, InvoiceState::Uncollectible) => true,
            _ => false, // reject any other case
        }
    }
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Invoice {
    pub id: Uuid,
    pub business_id: Uuid,
    pub customer_id: Uuid,
    pub state: InvoiceState,
    pub total_cents: i64,
    pub due_date: NaiveDate,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub versioning: i32,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct LineItem {
    pub id: Uuid,
    pub invoice_id: Uuid,
    pub description: String,
    pub quantity: i32,
    pub unit_amount_cents: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateInvoiceRequest {
    pub customer_id: Uuid,
    pub due_date: NaiveDate,
    pub line_items: Vec<CreateLineItemRequest>,
}

#[derive(Debug, Deserialize)]
pub struct EditInvoiceRequest {
    pub due_date: Option<NaiveDate>,
    pub line_items: Option<Vec<CreateLineItemRequest>>,
    pub versioning: i32,
}

#[derive(Debug, Deserialize)]
pub struct CreateLineItemRequest {
    pub description: String,
    pub quantity: i32,
    pub unit_amount_cents: i64,
}

#[derive(Debug, Serialize)]
pub struct InvoiceResponse {
    pub invoice: Invoice,
    pub line_items: Vec<LineItem>,
}

#[derive(Debug, Deserialize)]
pub struct FinalizeInvoiceRequest {
    pub versioning: i32,
}

#[derive(Debug, Deserialize)]
pub struct VoidInvoiceRequest {
    pub versioning: i32,
}
