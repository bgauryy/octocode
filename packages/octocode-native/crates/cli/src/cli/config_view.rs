//! Private, one-session loopback server for `octocode config view`.
//!
//! The browser is deliberately a dumb view: all inspection and edits go back
//! through the native config and MCP-management owners.  The fragment token is
//! exchanged once for a distinct CSRF token, so it is never sent in a URL.
mod backend;
mod lifecycle;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::header::{self, HeaderValue};
use hyper::{Method, Request, Response, StatusCode};
pub(super) use lifecycle::run;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use subtle::ConstantTimeEq;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, watch};
use tokio_util::sync::CancellationToken;

const MAX_BODY: usize = 64 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const HEADER_TIMEOUT: Duration = Duration::from_secs(10);
const CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'";

const INDEX: &str = include_str!("config_view/assets/index.html");
const APP: &str = include_str!("config_view/assets/app.js");
const STYLE: &str = include_str!("config_view/assets/style.css");

#[derive(Clone)]
struct State {
    origin: String,
    host: String,
    tokens: Arc<Mutex<Tokens>>,
    closing: CancellationToken,
    active: Arc<Semaphore>,
    activity: watch::Sender<()>,
}

struct Tokens {
    bootstrap: Option<String>,
    session: Option<String>,
}

#[derive(Debug)]
struct Failure {
    status: StatusCode,
    code: &'static str,
    message: Option<String>,
}
impl Failure {
    fn new(status: StatusCode, code: &'static str) -> Self {
        Self {
            status,
            code,
            message: None,
        }
    }
    fn safe(status: StatusCode, code: &'static str, message: String) -> Self {
        Self {
            status,
            code,
            message: Some(message),
        }
    }
}

fn token() -> Result<String, Failure> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|_| Failure::new(StatusCode::INTERNAL_SERVER_ERROR, "REQUEST_FAILED"))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn constant_time_equal(left: &str, right: &str) -> bool {
    bool::from(left.as_bytes().ct_eq(right.as_bytes()))
}

fn response(
    status: StatusCode,
    body: impl Into<Bytes>,
    content_type: &'static str,
) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(body.into()));
    *response.status_mut() = status;
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CSP),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        "cross-origin-resource-policy",
        HeaderValue::from_static("same-origin"),
    );
    headers.insert("x-frame-options", HeaderValue::from_static("DENY"));
    headers.insert(
        "permissions-policy",
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );
    response
}

fn json_response(status: StatusCode, value: Value) -> Response<Full<Bytes>> {
    response(status, value.to_string(), "application/json; charset=utf-8")
}

fn failure_response(error: Failure) -> Response<Full<Bytes>> {
    let default = match error.code {
        "CONFLICT" => {
            "The file changed. Refresh and review the current configuration before saving."
        }
        "INVALID_INPUT" => "The configuration value is invalid.",
        "FORBIDDEN" => "This change is not allowed for this scope.",
        "CLOSED" => "The configuration session has ended. Run the command again.",
        _ => "The request could not be completed.",
    };
    json_response(
        error.status,
        json!({"error":{"code":error.code,"message":error.message.unwrap_or_else(|| default.into())}}),
    )
}

fn validate_context(request: &Request<Incoming>, state: &State) -> Result<(), Failure> {
    if request.headers().get_all(header::HOST).iter().count() != 1
        || request
            .headers()
            .get(header::HOST)
            .and_then(|value| value.to_str().ok())
            != Some(&state.host)
    {
        return Err(Failure::new(StatusCode::FORBIDDEN, "FORBIDDEN"));
    }
    if request.headers().get_all(header::ORIGIN).iter().count() > 1
        || request.headers().get_all("sec-fetch-site").iter().count() > 1
    {
        return Err(Failure::new(StatusCode::FORBIDDEN, "FORBIDDEN"));
    }
    if let Some(origin) = request.headers().get(header::ORIGIN)
        && origin.to_str().ok() != Some(&state.origin)
    {
        return Err(Failure::new(StatusCode::FORBIDDEN, "FORBIDDEN"));
    }
    if let Some(site) = request
        .headers()
        .get("sec-fetch-site")
        .and_then(|value| value.to_str().ok())
        && !site.is_empty()
        && site != "same-origin"
        && site != "none"
    {
        return Err(Failure::new(StatusCode::FORBIDDEN, "FORBIDDEN"));
    }
    Ok(())
}

async fn read_json(request: Request<Incoming>) -> Result<Value, Failure> {
    if request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        != Some("application/json")
    {
        return Err(Failure::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "JSON_REQUIRED",
        ));
    }
    if request
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok())
        .is_some_and(|size| size > MAX_BODY)
    {
        return Err(Failure::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "BODY_TOO_LARGE",
        ));
    }
    let deadline = tokio::time::Instant::now() + HEADER_TIMEOUT;
    let mut body = request.into_body();
    let mut bytes = Vec::new();
    loop {
        let frame = tokio::time::timeout_at(deadline, body.frame())
            .await
            .map_err(|_| Failure::new(StatusCode::REQUEST_TIMEOUT, "TIMEOUT"))?
            .transpose()
            .map_err(|_| Failure::new(StatusCode::BAD_REQUEST, "INVALID_JSON"))?;
        let Some(frame) = frame else { break };
        if let Ok(data) = frame.into_data() {
            if bytes.len().saturating_add(data.len()) > MAX_BODY {
                return Err(Failure::new(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "BODY_TOO_LARGE",
                ));
            }
            bytes.extend_from_slice(&data);
        }
    }
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|_| Failure::new(StatusCode::BAD_REQUEST, "INVALID_JSON"))?;
    if !value.is_object() || !safe_payload(&value, 0) {
        return Err(Failure::new(StatusCode::BAD_REQUEST, "INVALID_INPUT"));
    }
    Ok(value)
}

fn safe_payload(value: &Value, depth: usize) -> bool {
    if depth > 12 {
        return false;
    }
    match value {
        Value::Object(object) => object.iter().all(|(key, value)| {
            !matches!(key.as_str(), "__proto__" | "constructor" | "prototype")
                && safe_payload(value, depth + 1)
        }),
        Value::Array(items) => items.iter().all(|value| safe_payload(value, depth + 1)),
        Value::String(value) => value.encode_utf16().count() <= 16_384,
        _ => true,
    }
}

fn credential(request: &Request<Incoming>) -> Option<&str> {
    if request
        .headers()
        .get_all("x-octocode-session")
        .iter()
        .count()
        != 1
    {
        return None;
    }
    request
        .headers()
        .get("x-octocode-session")
        .and_then(|value| value.to_str().ok())
}

async fn handle(request: Request<Incoming>, state: State) -> Response<Full<Bytes>> {
    if state.closing.is_cancelled() {
        return failure_response(Failure::new(StatusCode::GONE, "CLOSED"));
    }
    if let Err(error) = validate_context(&request, &state) {
        return failure_response(error);
    }
    let path = request.uri().path().to_owned();
    if request.uri().query().is_some()
        || request.uri().scheme().is_some()
        || request.uri().authority().is_some()
    {
        return failure_response(Failure::new(StatusCode::NOT_FOUND, "NOT_FOUND"));
    }
    if request.method() == Method::GET {
        let asset = match path.as_str() {
            "/" => Some((INDEX, "text/html; charset=utf-8")),
            "/app.js" => Some((APP, "text/javascript; charset=utf-8")),
            "/style.css" => Some((STYLE, "text/css; charset=utf-8")),
            _ => None,
        };
        if let Some((body, kind)) = asset {
            return response(StatusCode::OK, body, kind);
        }
    }
    if !matches!(
        path.as_str(),
        "/api/session" | "/api/request" | "/api/close"
    ) {
        return failure_response(Failure::new(StatusCode::NOT_FOUND, "NOT_FOUND"));
    }
    if request.method() != Method::POST {
        return failure_response(Failure::new(
            StatusCode::METHOD_NOT_ALLOWED,
            "METHOD_NOT_ALLOWED",
        ));
    }
    if request
        .headers()
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        != Some(&state.origin)
    {
        return failure_response(Failure::new(StatusCode::FORBIDDEN, "FORBIDDEN"));
    }
    let provided = credential(&request).map(str::to_owned);
    let expected = {
        let tokens = state
            .tokens
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if path == "/api/session" {
            tokens.bootstrap.clone()
        } else {
            tokens.session.clone()
        }
    };
    if provided
        .as_deref()
        .zip(expected.as_deref())
        .is_none_or(|(actual, expected)| !constant_time_equal(actual, expected))
    {
        return failure_response(Failure::new(StatusCode::UNAUTHORIZED, "UNAUTHORIZED"));
    }
    let permit: OwnedSemaphorePermit = match state.active.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => return failure_response(Failure::new(StatusCode::TOO_MANY_REQUESTS, "BUSY")),
    };
    let payload = match read_json(request).await {
        Ok(payload) => payload,
        Err(error) => return failure_response(error),
    };
    if path == "/api/session" {
        let mut tokens = state
            .tokens
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Recheck after streaming the body: simultaneous bootstrap requests cannot replay it.
        let Some(bootstrap) = tokens.bootstrap.as_deref() else {
            return failure_response(Failure::new(StatusCode::UNAUTHORIZED, "UNAUTHORIZED"));
        };
        if provided
            .as_deref()
            .is_none_or(|actual| !constant_time_equal(actual, bootstrap))
        {
            return failure_response(Failure::new(StatusCode::UNAUTHORIZED, "UNAUTHORIZED"));
        }
        tokens.bootstrap = None;
        match token() {
            Ok(session) => {
                tokens.session = Some(session.clone());
                let _ = state.activity.send(());
                drop(permit);
                return json_response(StatusCode::OK, json!({"token":session}));
            }
            Err(error) => return failure_response(error),
        }
    }
    if path == "/api/close" {
        drop(permit);
        state.closing.cancel();
        return json_response(StatusCode::OK, json!({"closed":true}));
    }
    let _ = state.activity.send(());
    let result = backend::request(payload, &state.closing).await;
    drop(permit);
    let _ = state.activity.send(());
    match result {
        Ok(value) => json_response(StatusCode::OK, value),
        Err(error) => failure_response(error),
    }
}
