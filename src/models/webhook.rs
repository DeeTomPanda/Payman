use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct WebhookEndpoint {
    pub id: Uuid,
    pub business_id: Uuid,
    pub url: String,
    pub secret: String,
    pub active: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateWebhookEndpointRequest {
    pub url: String,
}

