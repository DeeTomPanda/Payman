use axum::{Json, Router, http::StatusCode, routing::post};
use serde::{Deserialize, Serialize};
use tokio::time::{Duration, sleep};

#[derive(Deserialize)]
struct ChargeRequest {
    card_token: String,
    amount_cents: i64,
}

#[derive(Serialize)]
struct ChargeResponse {
    status: String,
    psp_ref: Option<String>,
    code: Option<String>,
}

async fn charge(Json(req): Json<ChargeRequest>) -> Result<Json<ChargeResponse>, StatusCode> {
    match req.card_token.as_str() {
        "tok_success" => {
            sleep(Duration::from_millis(100)).await;
            Ok(Json(ChargeResponse {
                status: "succeeded".to_string(),
                psp_ref: Some(uuid::Uuid::new_v4().to_string()),
                code: None,
            }))
        }
        "tok_insufficient_funds" => {
            sleep(Duration::from_millis(100)).await;
            Ok(Json(ChargeResponse {
                status: "failed".to_string(),
                psp_ref: None,
                code: Some("insufficient_funds".to_string()),
            }))
        }
        "tok_card_declined" => {
            sleep(Duration::from_millis(100)).await;
            Ok(Json(ChargeResponse {
                status: "failed".to_string(),
                psp_ref: None,
                code: Some("card_declined".to_string()),
            }))
        }
        "tok_timeout" => {
            // sleep 30s — your service must handle this!
            sleep(Duration::from_secs(30)).await;
            Ok(Json(ChargeResponse {
                status: "succeeded".to_string(),
                psp_ref: Some(uuid::Uuid::new_v4().to_string()),
                code: None,
            }))
        }
        "tok_network_error" => Err(StatusCode::INTERNAL_SERVER_ERROR),
        _ => Err(StatusCode::BAD_REQUEST),
    }
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let port = std::env::var("PSP_PORT").unwrap_or_else(|_| "3001".to_string());
    let app = Router::new().route("/charge", post(charge));

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port.parse().unwrap_or(3001)));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

    println!("mock PSP running on port {}",port);
    axum::serve(listener, app).await.unwrap();
}
