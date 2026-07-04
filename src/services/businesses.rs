use crate::errors::AppResult;
use crate::utils::api_key::generate_api_key;
use uuid::Uuid;

pub struct CreateBusinessResult {
    pub business_id: Uuid,
    pub api_key: String,
    pub prefix: String,
}

pub async fn create_business(db: &sqlx::PgPool, name: &str) -> AppResult<CreateBusinessResult> {
    let api_key = generate_api_key();
    let business_id = Uuid::new_v4();

    let mut tx = db.begin().await?;

    sqlx::query!(
        "INSERT INTO businesses (id, name) VALUES ($1, $2)",
        business_id,
        name
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query!(
        "INSERT INTO api_keys (id, business_id, key_hash, key_prefix)
         VALUES ($1, $2, $3, $4)",
        Uuid::new_v4(),
        business_id,
        api_key.hash,
        api_key.prefix
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(CreateBusinessResult {
        business_id,
        api_key: api_key.raw,
        prefix: api_key.prefix,
    })
}
