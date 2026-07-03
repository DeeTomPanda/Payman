use axum::{Router, routing::get};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{error, info};
use tracing_subscriber;

mod errors;
mod handlers;
mod middleware;
mod models;
mod services;
mod utils;
mod workers;

#[cfg(test)]
mod test;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub psp_url: String,
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    info!("starting Payman ...");

    // load up keys
    let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let psp_url =
        std::env::var("MOCK_PSP_URL").unwrap_or_else(|_| "http://127.0.0.1:9090".to_string());

    // load up db
    let pool: PgPool = match PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
    {
        Ok(p) => {
            info!("successfully connected to Postgres via SQLx");
            p
        }
        Err(e) => {
            error!("failed to connect to Postgres: {}", e);
            std::process::exit(1);
        }
    };

    // run migrations
    info!("running database migrations...");
    if let Err(e) = sqlx::migrate!("./migrations").run(&pool).await {
        error!("Failed to run migrations: {}", e);
        std::process::exit(1);
    }
    info!("database fully migrated.");

    let worker_db = pool.clone();
    let state = Arc::new(AppState { db: pool, psp_url });
    let worker_state = state.clone();
    
    // workers
    crate::workers::payment_status_worker::start(worker_state);
    crate::workers::webhook_worker::start_webhook_worker(worker_db);

    // state still available here for the router
    // setup axum app with state
    let public_routes = Router::new()
        .route("/health", get(|| async { "OK" }))
        .nest("/businesses", handlers::businesses::routes());

    let protected_routes = Router::new()
        .nest("/customers", handlers::customers::routes())
        .nest("/invoices", handlers::invoices::routes())
        .nest("/webhooks", handlers::webhooks::routes())
        .nest("/payments", handlers::payments::routes())
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::auth::auth_middleware,
        ));

    let app = Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port.parse().unwrap_or(8080)));
    let listener = TcpListener::bind(addr).await.unwrap();

    info!("listening on {}", addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

async fn shutdown_signal() {
    // Job 1: Listen for Ctrl+C (Local development)
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    // Job 3: Race them! Whichever happens first wins.
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("shutdown signal received, closing connections gracefully...");
}
