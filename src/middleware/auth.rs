use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use crate::{errors::AppError, AppState};
use crate::models::business::Business;

#[derive(Clone, Debug)]
pub struct AuthenticatedBusiness {
    pub business: Business,
}

pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    // extract key from header
    let api_key = extract_api_key(&req)?;

    // hash it
    let key_hash = hash_api_key(&api_key);
    let business = find_business_by_key(&state.db, &key_hash).await?;

    // finally attach it
    req.extensions_mut().insert(AuthenticatedBusiness { business });

    Ok(next.run(req).await)
}

fn extract_api_key(req: &Request) -> Result<String, AppError> {
    let auth_header = req
        .headers()
        .get("Authorization")
        .ok_or(AppError::Unauthorized)?
        .to_str()
        .map_err(|_| AppError::Unauthorized)?;

    // expect "Bearer sk_live_xxxxx"
    let key = auth_header
        .strip_prefix("Bearer ")
        .ok_or(AppError::Unauthorized)?
        .to_string();

    if key.is_empty() {
        return Err(AppError::Unauthorized);
    }

    Ok(key)
}

pub fn hash_api_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    hex::encode(hasher.finalize())
}

async fn find_business_by_key(
    db: &sqlx::PgPool,
    key_hash: &str,
) -> Result<Business, AppError> {
    let row = sqlx::query!(
        r#"
        SELECT b.id, b.name, b.created_at
        FROM businesses b
        JOIN api_keys k ON k.business_id = b.id
        WHERE k.key_hash = $1
        AND k.revoked = false
        "#,
        key_hash
    )
    .fetch_optional(db)
    .await?;

    match row {
        Some(r) => Ok(Business {
            id: r.id,
            name: r.name,
            created_at: r.created_at,
        }),
        None => Err(AppError::Unauthorized),
    }
}