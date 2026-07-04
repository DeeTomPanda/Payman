use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Business {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Deserialize, Validate)]
pub struct CreateBusinessRequest {
    #[validate(length(min = 2, max = 100, message = "name must be 2–100 characters"))]
    pub name: String,
}
