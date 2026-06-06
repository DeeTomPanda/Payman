
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Serialize)]
pub struct PspChargeRequest {
    pub card_token: String,
    pub amount_cents: i64,
}

#[derive(Debug, Deserialize)]
pub struct PspChargeResponse {
    pub status: String,
    pub psp_ref: Option<String>,
    pub code: Option<String>,
}

#[derive(Debug)]
pub enum PspResult {
    Succeeded { psp_ref: String },
    Failed { code: String },
    TimedOut,
    NetworkError,
}

pub async fn call_psp(
    psp_url: &str,
    card_token: &str,
    amount_cents: i64,
) -> PspResult {
    let client = reqwest::Client::new();

    let result = tokio::time::timeout(
        Duration::from_secs(5), // never wait more than 5s
        client
            .post(format!("{}/charge", psp_url))
            .json(&PspChargeRequest {
                card_token: card_token.to_string(),
                amount_cents,
            })
            .send(),
    )
    .await;

    match result {
        // timeout hit
        Err(_) => PspResult::TimedOut,

        // request completed
        Ok(Ok(response)) => {
            if !response.status().is_success() {
                return PspResult::NetworkError;
            }

            match response.json::<PspChargeResponse>().await {
                Ok(psp) => match psp.status.as_str() {
                    "succeeded" => PspResult::Succeeded {
                        psp_ref: psp.psp_ref.unwrap_or_default(),
                    },
                    _ => PspResult::Failed {
                        code: psp.code.unwrap_or("unknown".to_string()),
                    },
                },
                Err(_) => PspResult::NetworkError,
            }
        }

        // network error
        Ok(Err(_)) => PspResult::NetworkError,
    }
}