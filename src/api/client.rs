//! HTTP API client for the Weixin iLink Bot API.

use std::time::Duration;

use crate::config::WeixinConfig;
use crate::error::{Error, Result};
use crate::types::{
    BaseInfo, CHANNEL_VERSION, DEFAULT_CONFIG_TIMEOUT_MS, GetConfigRequest, GetConfigResponse,
    GetUpdatesRequest, GetUpdatesResponse, GetUploadUrlRequest, GetUploadUrlResponse, ILINK_APP_ID,
    SendMessageRequest, SendMessageResponse, SendTypingRequest, build_base_info_with_agent,
};
use crate::util::{net_error, redact};

/// Encode version string as `(major<<16)|(minor<<8)|patch`.
fn build_client_version(version: &str) -> u32 {
    let parts: Vec<u32> = version.split('.').filter_map(|p| p.parse().ok()).collect();
    let major = parts.first().copied().unwrap_or(0) & 0xff;
    let minor = parts.get(1).copied().unwrap_or(0) & 0xff;
    let patch = parts.get(2).copied().unwrap_or(0) & 0xff;
    (major << 16) | (minor << 8) | patch
}

/// Generate a random `X-WECHAT-UIN` header value.
fn random_wechat_uin() -> String {
    use base64::Engine;
    use rand::Rng;
    let n: u32 = rand::rng().random();
    base64::engine::general_purpose::STANDARD.encode(n.to_string().as_bytes())
}

fn ensure_trailing_slash(url: &str) -> String {
    if url.ends_with('/') {
        url.to_owned()
    } else {
        format!("{url}/")
    }
}

/// Log a transport-level failure with a redacted URL and a failure category.
///
/// The error itself is deliberately **not** logged: `reqwest::Error` and its
/// source chain can carry the full URL including its query string, which would
/// defeat redaction (standards §1.3).
fn log_transport_failure(method: &str, url: &str, timeout: Option<Duration>, err: &Error) {
    let kind = net_error::classify(err);
    tracing::error!(
        method,
        url = redact::redact_url(url),
        timeout_ms = timeout.map(|t| u64::try_from(t.as_millis()).unwrap_or(u64::MAX)),
        kind = kind.as_str(),
        description = kind.description(),
        "HTTP transport failure"
    );
}

/// Validate a raw `sendMessage` response body.
///
/// Empty bodies and a missing `ret` are both success. This is not defensive
/// guesswork — it is what the live endpoint actually returns (verified against
/// the production API, 2026-08-13):
///
/// - text / media sends answer `{"message_id": <i64>}` — **no `ret` field**;
/// - tool-call progress items (types 11 / 12) answer with an **empty body**.
///
/// Requiring `ret == 0`, or rejecting empty bodies the way the reference
/// implementation's `JSON.parse` does, would therefore fail every successful
/// send. A non-empty body must still be valid JSON: if it cannot be parsed we
/// cannot tell success from failure, so that is surfaced as an error rather than
/// assumed OK.
pub(crate) fn validate_send_message_response(raw: &str) -> Result<()> {
    if raw.trim().is_empty() {
        return Ok(());
    }
    let resp: SendMessageResponse = serde_json::from_str(raw)?;
    match resp.ret {
        Some(ret) if ret != 0 => Err(Error::Api {
            errcode: ret,
            errmsg: resp.errmsg.unwrap_or_default(),
        }),
        _ => Ok(()),
    }
}

/// Low-level HTTP client for all iLink Bot API endpoints.
pub(crate) struct HttpApiClient {
    base_url: String,
    token: String,
    route_tag: Option<u32>,
    api_timeout: Duration,
    bot_agent: String,
    http: reqwest::Client,
}

impl HttpApiClient {
    /// Create a new API client from config.
    pub fn new(config: &WeixinConfig) -> Self {
        Self {
            base_url: ensure_trailing_slash(&config.base_url),
            token: config.token.clone(),
            route_tag: config.route_tag,
            api_timeout: config.api_timeout,
            bot_agent: config.bot_agent.clone(),
            http: reqwest::Client::new(),
        }
    }

    fn common_headers(&self) -> Vec<(&'static str, String)> {
        let mut h = vec![
            ("iLink-App-Id", ILINK_APP_ID.to_owned()),
            (
                "iLink-App-ClientVersion",
                build_client_version(CHANNEL_VERSION).to_string(),
            ),
        ];
        if let Some(tag) = self.route_tag {
            h.push(("SKRouteTag", tag.to_string()));
        }
        h
    }

    fn post_headers(&self) -> Vec<(&'static str, String)> {
        let mut h = vec![
            ("Content-Type", "application/json".to_owned()),
            ("AuthorizationType", "ilink_bot_token".to_owned()),
            ("X-WECHAT-UIN", random_wechat_uin()),
        ];
        if !self.token.is_empty() {
            h.push(("Authorization", format!("Bearer {}", self.token.trim())));
        }
        h.extend(self.common_headers());
        h
    }

    /// POST a JSON body and return the raw response text.
    ///
    /// The single transport path for authenticated POSTs; [`Self::post_json`] is a
    /// thin deserializing wrapper over it.
    async fn post_raw(
        &self,
        endpoint: &str,
        body: &impl serde::Serialize,
        timeout: Option<Duration>,
    ) -> Result<String> {
        let url = format!("{}{endpoint}", self.base_url);
        let body_str = serde_json::to_string(body)?;
        tracing::debug!(
            url = redact::redact_url(&url),
            body = redact::redact_body_default(&body_str),
            "POST"
        );

        let mut builder = self.http.post(&url).body(body_str);
        if let Some(t) = timeout {
            builder = builder.timeout(t);
        }
        for (k, v) in self.post_headers() {
            builder = builder.header(k, v);
        }

        let response = match builder.send().await {
            Ok(r) => r,
            Err(e) => {
                let err = Error::Http(e);
                log_transport_failure("POST", &url, timeout, &err);
                return Err(err);
            }
        };
        let status = response.status();
        let raw = response.text().await?;
        tracing::debug!(
            status = %status,
            body = redact::redact_body_default(&raw),
            "response"
        );
        if !status.is_success() {
            return Err(Error::Api {
                errcode: i32::from(status.as_u16()),
                errmsg: raw,
            });
        }
        Ok(raw)
    }

    async fn post_json<T: serde::de::DeserializeOwned>(
        &self,
        endpoint: &str,
        body: &impl serde::Serialize,
        timeout: Option<Duration>,
    ) -> Result<T> {
        let raw = self.post_raw(endpoint, body, timeout).await?;
        Ok(serde_json::from_str(&raw)?)
    }

    /// Long-poll `getUpdates`. On client-side timeout, returns an empty response.
    pub async fn get_updates(
        &self,
        request: &GetUpdatesRequest,
        timeout: Duration,
    ) -> Result<GetUpdatesResponse> {
        let url = format!("{}ilink/bot/getupdates", self.base_url);
        let body_str = serde_json::to_string(request)?;

        let mut builder = self.http.post(&url).timeout(timeout).body(body_str);
        for (k, v) in self.post_headers() {
            builder = builder.header(k, v);
        }

        match builder.send().await {
            Ok(response) => {
                let raw = response.text().await?;
                Ok(serde_json::from_str(&raw)?)
            }
            Err(e) if e.is_timeout() => {
                tracing::debug!("getUpdates: client-side timeout, returning empty response");
                Ok(GetUpdatesResponse {
                    ret: Some(0),
                    msgs: Some(Vec::new()),
                    get_updates_buf: Some(request.get_updates_buf.clone()),
                    ..Default::default()
                })
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Send a message.
    ///
    /// Returns an error when the server reports a non-zero `ret`: an HTTP 200 with
    /// a business-level failure means the message was **not** delivered.
    pub async fn send_message(&self, request: &SendMessageRequest) -> Result<()> {
        let raw = self
            .post_raw("ilink/bot/sendmessage", request, Some(self.api_timeout))
            .await?;
        validate_send_message_response(&raw)
    }

    /// Get a pre-signed CDN upload URL.
    pub async fn get_upload_url(
        &self,
        request: &GetUploadUrlRequest,
    ) -> Result<GetUploadUrlResponse> {
        self.post_json("ilink/bot/getuploadurl", request, Some(self.api_timeout))
            .await
    }

    /// Fetch bot config (`typing_ticket`).
    pub async fn get_config(
        &self,
        user_id: &str,
        context_token: Option<&str>,
    ) -> Result<GetConfigResponse> {
        let body = GetConfigRequest {
            ilink_user_id: user_id.to_owned(),
            context_token: context_token.map(String::from),
            base_info: self.base_info(),
        };
        self.post_json(
            "ilink/bot/getconfig",
            &body,
            Some(Duration::from_millis(DEFAULT_CONFIG_TIMEOUT_MS)),
        )
        .await
    }

    /// Send a typing indicator.
    pub async fn send_typing(&self, request: &SendTypingRequest) -> Result<()> {
        let _: serde_json::Value = self
            .post_json(
                "ilink/bot/sendtyping",
                request,
                Some(Duration::from_millis(DEFAULT_CONFIG_TIMEOUT_MS)),
            )
            .await?;
        Ok(())
    }

    /// Build a `BaseInfo` with the configured `bot_agent`.
    pub fn base_info(&self) -> BaseInfo {
        build_base_info_with_agent(&self.bot_agent)
    }

    /// POST request without auth token (for QR login endpoint).
    pub async fn api_post(
        &self,
        endpoint: &str,
        body: &impl serde::Serialize,
        timeout: Option<Duration>,
    ) -> Result<String> {
        let url = format!("{}{endpoint}", self.base_url);
        let body_str = serde_json::to_string(body)?;
        tracing::debug!(
            url = redact::redact_url(&url),
            body = redact::redact_body_default(&body_str),
            "POST (no-auth)"
        );

        let mut builder = self.http.post(&url).body(body_str);
        if let Some(t) = timeout {
            builder = builder.timeout(t);
        }
        builder = builder
            .header("Content-Type", "application/json")
            .header("AuthorizationType", "ilink_bot_token")
            .header("X-WECHAT-UIN", random_wechat_uin());
        for (k, v) in self.common_headers() {
            builder = builder.header(k, v);
        }

        let response = builder.send().await?;
        let status = response.status();
        let raw = response.text().await?;
        if !status.is_success() {
            return Err(Error::Api {
                errcode: i32::from(status.as_u16()),
                errmsg: raw,
            });
        }
        Ok(raw)
    }

    /// Notify server that this bot is starting (best-effort).
    pub async fn notify_start(&self) -> Result<()> {
        let body = serde_json::json!({ "base_info": self.base_info() });
        let _: serde_json::Value = self
            .post_json(
                "ilink/bot/msg/notifystart",
                &body,
                Some(Duration::from_secs(10)),
            )
            .await?;
        Ok(())
    }

    /// Notify server that this bot is stopping (best-effort).
    pub async fn notify_stop(&self) -> Result<()> {
        let body = serde_json::json!({ "base_info": self.base_info() });
        let _: serde_json::Value = self
            .post_json(
                "ilink/bot/msg/notifystop",
                &body,
                Some(Duration::from_secs(10)),
            )
            .await?;
        Ok(())
    }

    /// GET request for QR code endpoints.
    pub async fn api_get(&self, endpoint: &str, timeout: Duration) -> Result<String> {
        let url = format!("{}{endpoint}", self.base_url);
        tracing::debug!(url = redact::redact_url(&url), "GET");

        let mut builder = self.http.get(&url).timeout(timeout);
        for (k, v) in self.common_headers() {
            builder = builder.header(k, v);
        }

        let response = match builder.send().await {
            Ok(r) => r,
            Err(e) => {
                let err = Error::Http(e);
                log_transport_failure("GET", &url, Some(timeout), &err);
                return Err(err);
            }
        };
        let status = response.status();
        let raw = response.text().await?;
        if !status.is_success() {
            return Err(Error::Api {
                errcode: i32::from(status.as_u16()),
                errmsg: raw,
            });
        }
        Ok(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_client_version_encoding() {
        assert_eq!(build_client_version("2.1.1"), (2 << 16) | (1 << 8) | 1);
        assert_eq!(build_client_version("1.0.0"), 1 << 16);
        assert_eq!(build_client_version("0.0.1"), 1);
        assert_eq!(build_client_version(""), 0);
    }

    #[test]
    fn ensure_trailing_slash_adds() {
        assert_eq!(
            ensure_trailing_slash("https://example.com"),
            "https://example.com/"
        );
    }

    #[test]
    fn ensure_trailing_slash_noop() {
        assert_eq!(
            ensure_trailing_slash("https://example.com/"),
            "https://example.com/"
        );
    }

    #[test]
    fn random_wechat_uin_format() {
        use base64::Engine;
        let uin = random_wechat_uin();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&uin)
            .unwrap();
        let s = std::str::from_utf8(&decoded).unwrap();
        assert!(s.parse::<u32>().is_ok());
    }

    #[test]
    fn send_message_accepts_zero_ret() {
        assert!(validate_send_message_response(r#"{"ret":0}"#).is_ok());
    }

    #[test]
    fn send_message_accepts_absent_ret() {
        assert!(validate_send_message_response("{}").is_ok());
    }

    #[test]
    fn send_message_accepts_empty_body() {
        assert!(validate_send_message_response("").is_ok());
        assert!(validate_send_message_response("   \n").is_ok());
    }

    #[test]
    fn send_message_rejects_non_zero_ret_with_errcode_and_msg() {
        let err =
            validate_send_message_response(r#"{"ret":-14,"errmsg":"stale token"}"#).unwrap_err();
        match err {
            Error::Api { errcode, errmsg } => {
                assert_eq!(errcode, -14);
                assert_eq!(errmsg, "stale token");
            }
            other => panic!("expected Error::Api, got {other:?}"),
        }
    }

    #[test]
    fn send_message_rejects_malformed_body() {
        // Cannot tell success from failure → must not be assumed OK.
        assert!(validate_send_message_response("not json").is_err());
    }

    /// End-to-end over the real HTTP path: an HTTP 200 carrying a business-level
    /// failure must surface as an error, which the pure-function tests above
    /// cannot prove on their own.
    #[tokio::test]
    async fn send_message_surfaces_server_ret_as_api_error() {
        use crate::messaging::send::build_text_message;
        use tokio::io::AsyncWriteExt;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let body = r#"{"ret":-14,"errmsg":"stale token"}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n{body}",
                body.len()
            );
            // No need to drain the request; reply and close.
            let _ = sock.write_all(resp.as_bytes()).await;
            let _ = sock.flush().await;
        });

        let cfg = WeixinConfig::builder()
            .token("t")
            .base_url(format!("http://{addr}/"))
            .build()
            .unwrap();
        let api = HttpApiClient::new(&cfg);
        let req = build_text_message("u1", "hi", None, None, api.base_info());
        assert!(matches!(
            api.send_message(&req).await,
            Err(Error::Api { errcode: -14, .. })
        ));
    }
}
