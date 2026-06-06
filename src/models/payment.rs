use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use validator::Validate;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq)]
#[sqlx(type_name = "payment_status", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum PaymentStatus {
    Pending,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize,Deserialize, sqlx::FromRow)]
pub struct PaymentAttempt {
    pub id: Uuid,
    pub invoice_id: Uuid,
    pub status: PaymentStatus,
    pub card_token: String,
    pub psp_reference: Option<String>,
    pub failure_code: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct PayInvoiceRequest {
    #[validate(length(min = 1, message = "card token cannot be empty"))]
    pub card_token: String,
}