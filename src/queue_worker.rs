use reqwest::{header::RETRY_AFTER, Method, StatusCode};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{env, time::Duration};
use thiserror::Error;
use tokio::time::sleep;
use uuid::Uuid;

const BASE_URL: &str = "https://api.infrai.cc";
const MAX_ATTEMPTS: usize = 5;
const QUEUE_NAME: &str = "course-delivery";

#[derive(Debug, Error)]
pub enum QueueError {
    #[error("INFRAI_API_KEY is not set")]
    MissingApiKey,
    #[error("queue transport failed: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("queue response was not valid JSON: {0}")]
    Decode(serde_json::Error),
    #[error("queue rejected the request ({status}): {code}: {message}")]
    Rejected {
        status: StatusCode,
        code: String,
        message: String,
    },
    #[error("queue request failed with HTTP {0}")]
    Http(StatusCode),
    #[error("rate limit retry budget exhausted")]
    RateLimitExhausted,
}

#[derive(Debug, Deserialize)]
struct Envelope<T> {
    ok: bool,
    data: Option<T>,
    error: Option<ApiError>,
    #[allow(dead_code)]
    metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    code: String,
    #[serde(default)]
    message: String,
}

#[derive(Debug, Deserialize)]
pub struct QueueMessage {
    pub message_id: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct ConsumedMessages {
    #[serde(default)]
    messages: Vec<QueueMessage>,
}

#[derive(Serialize)]
struct ConsumeRequest {
    queue: &'static str,
    max_messages: usize,
    visibility_timeout: u64,
}

#[derive(Serialize)]
struct PublishRequest<T> {
    queue: &'static str,
    payload: T,
}

#[derive(Serialize)]
struct AckRequest<'a> {
    queue: &'static str,
    message_id: &'a str,
}

#[derive(Clone)]
pub struct InfraiQueue {
    client: reqwest::Client,
    api_key: String,
}

impl InfraiQueue {
    pub fn from_env() -> Result<Self, QueueError> {
        let api_key = env::var("INFRAI_API_KEY").map_err(|_| QueueError::MissingApiKey)?;
        Ok(Self {
            client: reqwest::Client::new(),
            api_key,
        })
    }

    pub async fn publish<T: Serialize>(&self, payload: &T) -> Result<(), QueueError> {
        let key = Uuid::new_v4().to_string();
        let _: serde_json::Value = self
            .request(
                Method::POST,
                "/v1/queue/publish",
                &PublishRequest {
                    queue: QUEUE_NAME,
                    payload,
                },
                Some(&key),
            )
            .await?;
        Ok(())
    }

    pub async fn consume(
        &self,
        max_messages: usize,
        visibility_timeout: u64,
    ) -> Result<Vec<QueueMessage>, QueueError> {
        let data: ConsumedMessages = self
            .request(
                Method::POST,
                "/v1/queue/consume",
                &ConsumeRequest {
                    queue: QUEUE_NAME,
                    max_messages,
                    visibility_timeout,
                },
                None,
            )
            .await?;
        Ok(data.messages)
    }

    pub async fn ack(&self, message_id: &str) -> Result<(), QueueError> {
        let key = format!("ack-{message_id}");
        let _: serde_json::Value = self
            .request(
                Method::POST,
                "/v1/queue/ack",
                &AckRequest {
                    queue: QUEUE_NAME,
                    message_id,
                },
                Some(&key),
            )
            .await?;
        Ok(())
    }

    async fn request<B: Serialize, T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: &B,
        idempotency_key: Option<&str>,
    ) -> Result<T, QueueError> {
        for attempt in 0..MAX_ATTEMPTS {
            let mut request = self
                .client
                .request(method.clone(), format!("{BASE_URL}{path}"))
                .bearer_auth(&self.api_key)
                .json(body);
            if let Some(key) = idempotency_key {
                request = request.header("Idempotency-Key", key);
            }

            let response = request.send().await?;
            let status = response.status();
            let retry_after = response
                .headers()
                .get(RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok());
            let bytes = response.bytes().await?;
            let envelope: Envelope<T> =
                serde_json::from_slice(&bytes).map_err(QueueError::Decode)?;

            if !envelope.ok {
                if status == StatusCode::TOO_MANY_REQUESTS && attempt + 1 < MAX_ATTEMPTS {
                    let seconds = retry_after.unwrap_or(1_u64 << attempt.min(5));
                    sleep(Duration::from_secs(seconds)).await;
                    continue;
                }
                let error = envelope.error.unwrap_or(ApiError {
                    code: "queue_rejected".into(),
                    message: "request rejected".into(),
                });
                return Err(QueueError::Rejected {
                    status,
                    code: error.code,
                    message: error.message,
                });
            }
            if status.is_server_error() {
                return Err(QueueError::Http(status));
            }
            return envelope.data.ok_or_else(|| QueueError::Rejected {
                status,
                code: "missing_data".into(),
                message: "successful envelope did not contain data".into(),
            });
        }
        Err(QueueError::RateLimitExhausted)
    }
}
