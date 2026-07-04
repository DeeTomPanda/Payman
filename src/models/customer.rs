use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Customer {
    pub id: Uuid,
    pub business_id: Uuid,
    pub name: String,
    pub email: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateCustomerRequest {
    #[validate(length(min = 2, max = 100, message = "name must be 2–100 characters"))]
    pub name: String,
    #[validate(length(max = 254), email)]
    pub email: String,
}
