use axum::{
    Router,
    routing::{get, post},
};
use std::sync::Arc;

use crate::{
    AppState,
};

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
}