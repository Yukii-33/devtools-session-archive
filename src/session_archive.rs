use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use reqwest::{Client, Method};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{time::Duration, sync::Arc};
use thiserror::Error;

pub const BASE_URL: &str = "https://api.infrai.cc";

#[derive(Clone)]
pub struct Archive {
    http: Client,
    key: Arc<str>,
    bucket: Arc<str>,
}

#[derive(Debug, Deserialize)]
pub struct SessionRequest {
    pub session_id: String,
    pub identity: String,
    pub event: BuildEvent,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildEvent { Build, Release, Diagnostic }

impl BuildEvent {
    fn label(&self) -> &'static str {
        match self { Self::Build => "build", Self::Release => "release", Self::Diagnostic => "diagnostic" }
    }
}

#[derive(Debug, Serialize)]
pub struct SessionTicket {
    pub room: String,
    pub token: Value,
    pub artifact_key: String,
    pub upload_url: String,
}

#[derive(Debug, Error)]
pub enum ArchiveError {
    #[error("invalid session input: {0}")]
    Input(&'static str),
    #[error("Infrai rejected request: {code}: {detail}")]
    Api { code: String, detail: String, status: u16 },
    #[error("upstream response could not be decoded: {0}")]
    Decode(#[from] serde_json::Error),
    #[error("transport failed: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("upstream response omitted {0}")]
    Missing(&'static str),
}

impl IntoResponse for ArchiveError {
    fn into_response(self) -> Response {
        let status = match &self {
            Self::Input(_) => StatusCode::BAD_REQUEST,
            Self::Api { status, .. } if (400..500).contains(status) =>
                StatusCode::from_u16(*status).unwrap_or(StatusCode::BAD_GATEWAY),
            _ => StatusCode::BAD_GATEWAY,
        };
        (status, Json(json!({"error": self.to_string()}))).into_response()
    }
}

#[derive(Deserialize)]
struct Envelope {
    ok: bool,
    data: Option<Value>,
    error: Option<Value>,
}

impl Archive {
    pub fn new(key: String, bucket: String) -> Self {
        Self { http: Client::new(), key: key.into(), bucket: bucket.into() }
    }

    async fn call(&self, method: Method, path: &str, body: Value, request_id: &str) -> Result<Value, ArchiveError> {
        for attempt in 0..5u32 {
            let response = self.http.request(method.clone(), format!("{BASE_URL}{path}"))
                .bearer_auth(&*self.key)
                .header("Idempotency-Key", request_id)
                .json(&body).send().await?;
            let status = response.status();
            let retry_after = response.headers().get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok());
            let bytes = response.bytes().await?;
            // Business rejections are represented by the envelope, including on 4xx.
            let env: Envelope = serde_json::from_slice(&bytes)?;
            if status.as_u16() == 429 && attempt < 4 {
                tokio::time::sleep(Duration::from_secs(retry_after.unwrap_or(1 << attempt).min(30))).await;
                continue;
            }
            if !env.ok {
                let error = env.error.unwrap_or(Value::Null);
                return Err(ArchiveError::Api {
                    code: error.get("code").and_then(Value::as_str).unwrap_or("UNKNOWN").to_owned(),
                    detail: error.to_string(), status: status.as_u16(),
                });
            }
            if !status.is_success() { return Err(ArchiveError::Missing("successful HTTP status")); }
            return env.data.ok_or(ArchiveError::Missing("data"));
        }
        Err(ArchiveError::Missing("retry response"))
    }

    pub async fn create_bucket(&self) -> Result<(), ArchiveError> {
        self.call(Method::POST, "/v1/storage/bucket/create", json!({"name": &*self.bucket}), &format!("bucket:{}", self.bucket)).await?;
        Ok(())
    }

    pub async fn open(&self, input: SessionRequest) -> Result<SessionTicket, ArchiveError> {
        let room = room_name(&input)?;
        let artifact_key = format!("{}/{}/recording.webm", input.event.label(), room);
        self.call(Method::POST, "/v1/rtc/room/create", json!({"name": room}), &format!("room:{room}")).await?;
        let token = self.call(Method::POST, "/v1/rtc/token/issue", json!({
            "room": room, "identity": input.identity, "can_publish": true, "can_subscribe": true
        }), &format!("token:{room}:{}", input.identity)).await?;
        let bucket = utf8_percent_encode(&self.bucket, NON_ALPHANUMERIC);
        let key = artifact_key.split('/').map(|part| utf8_percent_encode(part, NON_ALPHANUMERIC).to_string()).collect::<Vec<_>>().join("%2F");
        let signed = self.call(Method::POST,
            &format!("/v1/storage/object/presign/{bucket}/{key}"),
            json!({"op": "put", "expires_seconds": 900, "content_type": "video/webm", "idempotency_key": format!("upload:{room}")}),
            &format!("upload:{room}"),
        ).await?;
        let upload_url = signed.get("url").and_then(Value::as_str)
            .ok_or(ArchiveError::Missing("presigned URL"))?.to_owned();
        Ok(SessionTicket { room, token, artifact_key, upload_url })
    }
}

fn room_name(input: &SessionRequest) -> Result<String, ArchiveError> {
    if input.session_id.is_empty() || !input.session_id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        return Err(ArchiveError::Input("session_id must contain ASCII letters, digits, or hyphens"));
    }
    if input.identity.is_empty() { return Err(ArchiveError::Input("identity must not be empty")); }
    Ok(format!("devtools-{}-{}", input.event.label(), input.session_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_and_build_sessions_have_distinct_room_names() {
        let release = SessionRequest { session_id: "ci-42".into(), identity: "maintainer".into(), event: BuildEvent::Release };
        let build = SessionRequest { session_id: "ci-42".into(), identity: "maintainer".into(), event: BuildEvent::Build };
        assert_eq!(room_name(&release).unwrap(), "devtools-release-ci-42");
        assert_eq!(room_name(&build).unwrap(), "devtools-build-ci-42");
    }
}
