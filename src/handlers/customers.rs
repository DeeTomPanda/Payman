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
    services::customers::create_customer as create_customer_service,
    services::customers::get_customer as get_customer_service,
    services::customers::list_customers as list_customers_service,
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

    let customer = create_customer_service(&state.db, req, auth.business.id).await?;

    Ok(Json(customer))
}

async fn get_customer(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthenticatedBusiness>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Customer>> {
    let customer = get_customer_service(&state.db, id, auth.business.id).await?;

    Ok(Json(customer))
}

async fn list_customers(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthenticatedBusiness>,
) -> AppResult<Json<Vec<Customer>>> {
    let customers = list_customers_service(&state.db, auth.business.id).await?;

    Ok(Json(customers))
}
