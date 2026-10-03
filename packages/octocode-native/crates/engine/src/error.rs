use serde_json::Value;
use std::fmt::{self, Display, Formatter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    InvalidArg,
    GenericFailure,
}

/// Engine error. `status` and `reason` are the stable, rendered surface (napi
/// and the runtime display `reason`); `kind` keeps the machine-readable cause so
/// callers branch on a typed value instead of parsing `reason` text.
#[derive(Clone, Debug)]
pub struct Error {
    pub status: Status,
    pub reason: String,
    kind: ErrorKind,
}

/// Machine-readable cause of an [`Error`].
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ErrorKind {
    /// No finer classification (the default for [`Error::new`]).
    Other,
    /// The peer answered a JSON-RPC request with an `error` object (boxed to
    /// keep `Result<T, Error>` small).
    Rpc(Box<RpcError>),
    /// A request or write exceeded its deadline.
    Timeout,
    /// The JSON-RPC connection is closed or failed (EOF, protocol fault,
    /// broken write, or retired after a timeout).
    ConnectionClosed,
}

/// A JSON-RPC `ResponseError` (`{code, message, data?}`) returned by a peer.
#[derive(Clone, Debug, PartialEq)]
pub struct RpcError {
    pub code: ErrorCode,
    pub message: String,
    pub data: Option<Value>,
}

impl RpcError {
    /// Parses a JSON-RPC `error` member. A missing or non-integer `code` is
    /// classified as [`ErrorCode::InternalError`] so a malformed error can
    /// never be mistaken for a retryable one.
    pub fn from_value(error: Value) -> Self {
        let Value::Object(mut object) = error else {
            return Self {
                code: ErrorCode::InternalError,
                message: error.to_string(),
                data: None,
            };
        };
        let code = object
            .get("code")
            .and_then(Value::as_i64)
            .map_or(ErrorCode::InternalError, ErrorCode::from_i64);
        let message = match object.remove("message") {
            Some(Value::String(message)) => message,
            Some(other) => other.to_string(),
            None => String::new(),
        };
        Self {
            code,
            message,
            data: object.remove("data"),
        }
    }

    /// `true` when the request may be re-sent as-is: ContentModified always;
    /// ServerCancelled only when the server set `data.retriggerRequest`.
    pub fn is_retryable(&self) -> bool {
        match self.code {
            ErrorCode::ContentModified => true,
            ErrorCode::ServerCancelled => self
                .data
                .as_ref()
                .and_then(|data| data.get("retriggerRequest"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
            _ => false,
        }
    }
}

/// JSON-RPC / LSP error codes that change client behaviour, plus a lossless
/// fallback for everything else.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ErrorCode {
    ParseError,
    InvalidRequest,
    MethodNotFound,
    InvalidParams,
    InternalError,
    ServerNotInitialized,
    UnknownErrorCode,
    RequestFailed,
    ServerCancelled,
    ContentModified,
    RequestCancelled,
    Other(i64),
}

impl ErrorCode {
    pub fn from_i64(code: i64) -> Self {
        match code {
            -32700 => Self::ParseError,
            -32600 => Self::InvalidRequest,
            -32601 => Self::MethodNotFound,
            -32602 => Self::InvalidParams,
            -32603 => Self::InternalError,
            -32002 => Self::ServerNotInitialized,
            -32001 => Self::UnknownErrorCode,
            -32803 => Self::RequestFailed,
            -32802 => Self::ServerCancelled,
            -32801 => Self::ContentModified,
            -32800 => Self::RequestCancelled,
            other => Self::Other(other),
        }
    }

    pub fn as_i64(self) -> i64 {
        match self {
            Self::ParseError => -32700,
            Self::InvalidRequest => -32600,
            Self::MethodNotFound => -32601,
            Self::InvalidParams => -32602,
            Self::InternalError => -32603,
            Self::ServerNotInitialized => -32002,
            Self::UnknownErrorCode => -32001,
            Self::RequestFailed => -32803,
            Self::ServerCancelled => -32802,
            Self::ContentModified => -32801,
            Self::RequestCancelled => -32800,
            Self::Other(code) => code,
        }
    }
}

impl Display for ErrorCode {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Other(code) => write!(formatter, "{code}"),
            named => write!(formatter, "{} ({named:?})", named.as_i64()),
        }
    }
}

impl Error {
    pub fn new(status: Status, reason: impl Into<String>) -> Self {
        Self {
            status,
            reason: reason.into(),
            kind: ErrorKind::Other,
        }
    }

    /// A peer-returned JSON-RPC error. The reason keeps code, message, and
    /// data so rendered output stays as informative as the raw error object.
    pub fn rpc(error: RpcError) -> Self {
        let mut reason = format!("LSP error {}: {}", error.code, error.message);
        if let Some(data) = &error.data {
            reason.push_str(&format!(" (data: {data})"));
        }
        Self {
            status: Status::GenericFailure,
            reason,
            kind: ErrorKind::Rpc(Box::new(error)),
        }
    }

    pub fn timeout(reason: impl Into<String>) -> Self {
        Self {
            status: Status::GenericFailure,
            reason: reason.into(),
            kind: ErrorKind::Timeout,
        }
    }

    pub fn connection_closed(reason: impl Into<String>) -> Self {
        Self {
            status: Status::GenericFailure,
            reason: reason.into(),
            kind: ErrorKind::ConnectionClosed,
        }
    }

    pub fn kind(&self) -> &ErrorKind {
        &self.kind
    }

    /// The typed JSON-RPC error, when the peer returned one.
    pub fn rpc_error(&self) -> Option<&RpcError> {
        match &self.kind {
            ErrorKind::Rpc(error) => Some(error),
            _ => None,
        }
    }

    pub fn rpc_code(&self) -> Option<ErrorCode> {
        self.rpc_error().map(|error| error.code)
    }
}

impl Display for Error {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.reason)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(feature = "napi-addon")]
impl From<Error> for napi::Error {
    fn from(error: Error) -> Self {
        let status = match error.status {
            Status::InvalidArg => napi::Status::InvalidArg,
            Status::GenericFailure => napi::Status::GenericFailure,
        };
        Self::new(status, error.reason)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn error_stays_small_for_result_returns() {
        // `Result<T, Error>` is returned everywhere; RPC detail is boxed so the
        // error stays within clippy's `result_large_err` budget.
        assert!(
            std::mem::size_of::<Error>() <= 48,
            "{}",
            std::mem::size_of::<Error>()
        );
    }

    #[test]
    fn error_code_round_trips_known_and_unknown_codes() {
        for code in [
            -32700, -32601, -32002, -32803, -32802, -32801, -32800, 7, -1,
        ] {
            assert_eq!(ErrorCode::from_i64(code).as_i64(), code);
        }
        assert_eq!(ErrorCode::from_i64(-32801), ErrorCode::ContentModified);
        assert_eq!(ErrorCode::from_i64(-328011), ErrorCode::Other(-328011));
    }

    #[test]
    fn rpc_error_is_typed_and_keeps_an_informative_reason() {
        let error = Error::rpc(RpcError::from_value(json!({
            "code": -32801, "message": "content modified", "data": {"x": 1}
        })));
        assert_eq!(error.rpc_code(), Some(ErrorCode::ContentModified));
        assert!(error.reason.contains("-32801"));
        assert!(error.reason.contains("content modified"));
        assert!(error.reason.contains("\"x\":1"));
        let cloned = error.clone();
        assert_eq!(cloned.kind(), error.kind());
    }

    #[test]
    fn rpc_error_retry_policy_matches_by_code_not_text() {
        let modified = RpcError::from_value(json!({"code": -32801, "message": "m"}));
        assert!(modified.is_retryable());
        let cancelled = RpcError::from_value(json!({"code": -32802, "message": "m"}));
        assert!(
            !cancelled.is_retryable(),
            "ServerCancelled needs retriggerRequest"
        );
        let retrigger = RpcError::from_value(
            json!({"code": -32802, "message": "m", "data": {"retriggerRequest": true}}),
        );
        assert!(retrigger.is_retryable());
        let mentions = RpcError::from_value(
            json!({"code": -32603, "message": "code -32801: content modified"}),
        );
        assert!(!mentions.is_retryable());
        let missing = RpcError::from_value(json!({"message": "no code"}));
        assert_eq!(missing.code, ErrorCode::InternalError);
        // A plain error whose text merely looks like an RPC error is not typed.
        let stringly = Error::new(Status::GenericFailure, "LSP error: {\"code\":-32801}");
        assert_eq!(stringly.rpc_code(), None);
    }
}
