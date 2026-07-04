use crate::errors::AppResult;
use crate::models::customer::{CreateCustomerRequest, Customer};
use uuid::Uuid;

pub async fn create_customer(
    db: &sqlx::PgPool,
    req: CreateCustomerRequest,
    business_id: Uuid,
) -> AppResult<Customer> {
    let id = Uuid::new_v4();
    let name = req.name.trim();
    let email = req.email.trim();

    let customer = sqlx::query_as::<_, Customer>(
        r#"
        INSERT INTO customers (id, business_id, name, email)
        VALUES ($1, $2, $3, $4)
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(business_id)
    .bind(name)
    .bind(email)
    .fetch_one(db)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(ref db_err)
            if db_err.constraint() == Some("customers_business_id_email_key") =>
        {
            crate::errors::AppError::Conflict("customer with this email already exists".to_string())
        }
        _ => crate::errors::AppError::Database(e),
    })?;

    Ok(customer)
}

pub async fn get_customer(db: &sqlx::PgPool, id: Uuid, business_id: Uuid) -> AppResult<Customer> {
    let customer = sqlx::query_as!(
        Customer,
        r#"
        SELECT * FROM customers
        WHERE id = $1
        AND business_id = $2
        "#,
        id,
        business_id
    )
    .fetch_optional(db)
    .await?
    .ok_or(crate::errors::AppError::NotFound)?;

    Ok(customer)
}

pub async fn list_customers(db: &sqlx::PgPool, business_id: Uuid) -> AppResult<Vec<Customer>> {
    let customers = sqlx::query_as!(
        Customer,
        r#"
        SELECT * FROM customers
        WHERE business_id = $1
        ORDER BY created_at DESC
        "#,
        business_id
    )
    .fetch_all(db)
    .await?;

    Ok(customers)
}
