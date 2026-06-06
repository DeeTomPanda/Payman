use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    AppState,
    errors::AppResult,
    middleware::auth::AuthenticatedBusiness,
    models::customer::{CreateCustomerRequest, Customer},
};

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", post(create_customer).get(list_customers))
        .route("/{id}", get(get_customer))
}

async fn create_customer(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthenticatedBusiness>,
    Json(req): Json<CreateCustomerRequest>,
) -> AppResult<Json<Customer>> {
    // validate
    if req.name.trim().is_empty() {
        return Err(crate::errors::AppError::BadRequest(
            "name cannot be empty".to_string(),
        ));
    }
    if req.email.trim().is_empty() {
        return Err(crate::errors::AppError::BadRequest(
            "email cannot be empty".to_string(),
        ));
    }

    let id = Uuid::new_v4();
    let business_id = auth.business.id;
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
    .fetch_one(&state.db)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(ref db_err)
            if db_err.constraint() == Some("customers_business_id_email_key") =>
        {
            crate::errors::AppError::Conflict("customer with this email already exists".to_string())
        }
        _ => crate::errors::AppError::Database(e),
    })?;

    Ok(Json(customer))
}


async fn get_customer(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthenticatedBusiness>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Customer>> {
    let customer = sqlx::query_as!(Customer,
        r#"
        SELECT * FROM customers
        WHERE id = $1
        AND business_id = $2
        "#,
        id,
        auth.business.id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(crate::errors::AppError::NotFound)?;

    Ok(Json(customer))
}


async fn list_customers(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthenticatedBusiness>,
) -> AppResult<Json<Vec<Customer>>> {
    let customers = sqlx::query_as!(Customer,
        r#"
        SELECT * FROM customers
        WHERE business_id = $1
        ORDER BY created_at DESC
        "#,
        auth.business.id
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(customers))
}
