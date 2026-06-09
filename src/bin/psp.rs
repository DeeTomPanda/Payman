use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;
use tokio::time::{Duration, sleep};
use uuid::Uuid;

#[derive(Deserialize)]
struct ChargeRequest {
    card_token: String,
    attempt_id: Uuid,
}

#[derive(Serialize, Clone)]
struct ChargeOutcome {
    status: String,
    psp_ref: Option<String>,
    code: Option<String>,
}

type OutcomeStore = Arc<Mutex<HashMap<Uuid, ChargeOutcome>>>;

async fn charge(
    State(store): State<OutcomeStore>,
    Json(req): Json<ChargeRequest>,
) -> Result<Json<ChargeOutcome>, StatusCode> {
    match req.card_token.as_str() {
        "tok_success" => {
            sleep(Duration::from_millis(100)).await;
            let outcome = ChargeOutcome {
                status: "succeeded".to_string(),
                psp_ref: Some(Uuid::new_v4().to_string()),
                code: None,
            };
            store.lock().await.insert(req.attempt_id, outcome.clone());
            Ok(Json(outcome))
        }

        "tok_insufficient_funds" => {
            sleep(Duration::from_millis(100)).await;
            let outcome = ChargeOutcome {
                status: "failed".to_string(),
                psp_ref: None,
                code: Some("insufficient_funds".to_string()),
            };
            store.lock().await.insert(req.attempt_id, outcome.clone());
            Ok(Json(outcome))
        }

        "tok_card_declined" => {
            sleep(Duration::from_millis(100)).await;
            let outcome = ChargeOutcome {
                status: "failed".to_string(),
                psp_ref: None,
                code: Some("card_declined".to_string()),
            };
            store.lock().await.insert(req.attempt_id, outcome.clone());
            Ok(Json(outcome))
        }

        "tok_timeout" => {
            let store = store.clone();
            let attempt_id = req.attempt_id;
            tokio::spawn(async move {
                let outcome = ChargeOutcome {
                    status: "succeeded".to_string(),
                    psp_ref: Some(Uuid::new_v4().to_string()),
                    code: None,
                };
                store.lock().await.insert(attempt_id, outcome);
            });
            sleep(Duration::from_secs(30)).await;
            Ok(Json(ChargeOutcome {
                status: "succeeded".to_string(),
                psp_ref: Some(Uuid::new_v4().to_string()),
                code: None,
            }))
        }

        "tok_network_error" => Err(StatusCode::INTERNAL_SERVER_ERROR),

        _ => Err(StatusCode::BAD_REQUEST),
    }
}

async fn get_outcome(
    State(store): State<OutcomeStore>,
    Path(attempt_id): Path<Uuid>,
) -> Result<Json<ChargeOutcome>, StatusCode> {
    store
        .lock()
        .await
        .get(&attempt_id)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let port = std::env::var("PSP_PORT").unwrap_or_else(|_| "9090".to_string());

    let store: OutcomeStore = Arc::new(Mutex::new(HashMap::new()));

    let app = Router::new()
        .route("/health",get(||async  {"Ok"}))
        .route("/charge", post(charge))
        .route("/charge/{attempt_id}", get(get_outcome))
        .with_state(store);

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port.parse().unwrap_or(9090)));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    println!("mock PSP running on port {}", port);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

async fn shutdown_signal() {
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

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    println!("shutdown signal received, closing connections gracefully...");
}