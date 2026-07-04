use crate::models::payment::{PspChargeRequest, PspChargeResponse, PspResult};
use std::time::Duration;

pub async fn call_psp(psp_url: &str, attempt_id: String, card_token: &str) -> PspResult {
    let client = reqwest::Client::new();

    let result = tokio::time::timeout(
        Duration::from_secs(5), // never wait more than 5s
        client
            .post(format!("{}/charge", psp_url))
            .json(&PspChargeRequest {
                card_token: card_token.to_string(),
                attempt_id,
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
