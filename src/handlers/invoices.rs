use axum::{

    Router,
  
};
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    AppState
};

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
 }