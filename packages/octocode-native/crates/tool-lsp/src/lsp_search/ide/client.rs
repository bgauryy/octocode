//! The editor bridge's HTTP client: one authenticated `POST /lsp` per
//! question, over loopback.
//!
//! The bridge accepts a request only with `Host: 127.0.0.1:<port>` (a guard
//! against DNS rebinding) and its bearer token, so both are set here on every
//! call. The client sets no timeout of its own: the rung's share of the
//! request's deadline ([`Bridge::of`]) and the request's cancellation
//! ([`super::super::cancellable`]) bound each call, so a hung editor leaves
//! the other half of the request to the server and syntax rungs.
use super::lock::Lock;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HOST};
use serde_json::{Value, json};

/// Why a bridge call did not produce an answer.
#[derive(Debug)]
pub(super) enum BridgeError {
    /// The bridge could not be reached or its answer could not be read.
    Transport(String),
    /// The bridge answered with an error status and its `reason`.
    Status { code: u16, reason: String },
    /// The editor did not answer within the rung's share of the request.
    TimedOut,
    /// The editor has no provider for this language.
    Unsupported,
}

impl BridgeError {
    /// Whether the handle a hierarchy call carried is unknown to the editor
    /// (it forgot it), so the item must be prepared again.
    pub(super) fn is_stale_handle(&self) -> bool {
        matches!(self, Self::Status { code: 404, .. })
    }

    /// The `fellBackFrom.reason` this error stands for. Never names the token.
    pub(super) fn reason(&self) -> String {
        match self {
            Self::Transport(error) => format!("the IDE bridge could not be reached: {error}"),
            Self::Status { code, reason } => {
                let meaning = match code {
                    400 | 401 => "refused the request (stale lock file or token)",
                    404 => "no longer knows the item (it was not prepared again)",
                    422 => "could not open the document",
                    _ => "failed",
                };
                if reason.is_empty() {
                    format!("the IDE bridge {meaning} (HTTP {code})")
                } else {
                    format!("the IDE bridge {meaning} (HTTP {code}): {reason}")
                }
            }
            Self::TimedOut => "ide timed out".to_owned(),
            Self::Unsupported => "the editor has no provider for this language".to_owned(),
        }
    }
}

/// A connection to one editor's bridge.
pub(super) struct Bridge {
    base: String,
    host: String,
    token: String,
    /// Where the rung's share of the request ends; `None` for a request no
    /// deadline bounds (its cancellation ends the wait).
    ends: Option<std::time::Instant>,
    /// `ide.name` of the receipt: the editor's own name.
    pub(super) ide: String,
}

impl Bridge {
    /// The bridge of `lock`, whose every call ends by `ends`.
    pub(super) fn of(lock: &Lock, ends: Option<std::time::Instant>) -> Self {
        let host = format!("127.0.0.1:{}", lock.port);
        Self {
            base: format!("http://{host}"),
            host,
            token: lock.token.clone(),
            ends,
            ide: lock.ide.clone(),
        }
    }

    /// The editor's answer (`result`) to an LSP `method` with its `params`
    /// (the fields the bridge reads beside `method`).
    pub(super) async fn ask(&self, method: &str, params: Value) -> Result<Value, BridgeError> {
        let Some(ends) = self.ends else {
            return self.exchange(method, params).await;
        };
        tokio::time::timeout_at(
            tokio::time::Instant::from_std(ends),
            self.exchange(method, params),
        )
        .await
        .unwrap_or(Err(BridgeError::TimedOut))
    }

    async fn exchange(&self, method: &str, params: Value) -> Result<Value, BridgeError> {
        let mut body = json!({ "method": method });
        if let (Some(body), Value::Object(params)) = (body.as_object_mut(), params) {
            body.extend(params);
        }
        let client = http()?;
        let response = client
            .post(format!("{}/lsp", self.base))
            .header(HOST, &self.host)
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
            .header(CONTENT_TYPE, "application/json")
            .body(body.to_string())
            .send()
            .await
            .map_err(|error| BridgeError::Transport(without_url(&error)))?;
        let code = response.status().as_u16();
        let bytes = response
            .bytes()
            .await
            .map_err(|error| BridgeError::Transport(without_url(&error)))?;
        let answer: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        if code != 200 {
            let reason = answer
                .get("reason")
                .or_else(|| answer.get("error"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            return Err(BridgeError::Status { code, reason });
        }
        if answer.get("unsupported").and_then(Value::as_bool) == Some(true) {
            return Err(BridgeError::Unsupported);
        }
        match answer {
            Value::Object(mut fields) if fields.contains_key("result") => {
                Ok(fields.remove("result").unwrap_or(Value::Null))
            }
            _ => Err(BridgeError::Transport(
                "the answer has no `result`".to_owned(),
            )),
        }
    }
}

/// The loopback client from the one HTTP constructor: no proxy (a bridge is
/// never behind one) and no redirect (a service answering in its place must
/// not be able to send the request's paths and symbol names on). Its only
/// wait is the stall bound between reads of a silent socket; the rung's
/// share of the request's deadline ([`Bridge::of`]) ends every call first.
fn http() -> Result<&'static reqwest::Client, BridgeError> {
    static CLIENT: std::sync::LazyLock<Result<reqwest::Client, String>> =
        std::sync::LazyLock::new(|| {
            octocode_io::http::client(&octocode_io::http::ClientPolicy {
                proxy: octocode_io::http::Proxy::Direct,
                ..octocode_io::http::ClientPolicy::without_deadline()
            })
            .map_err(|error| error.to_string())
        });
    CLIENT
        .as_ref()
        .map_err(|error| BridgeError::Transport(error.clone()))
}

/// A transport error without the URL it names (which holds the port).
fn without_url(error: &reqwest::Error) -> String {
    let mut text = error.to_string();
    if let Some(url) = error.url() {
        text = text.replace(url.as_str(), "the bridge");
    }
    text
}
