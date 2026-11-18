use serde_json::Value;
use std::fmt::{self, Display, Formatter};

/// Engine error. `reason` is the stable, rendered surface (the
/// runtime displays `reason`); `kind` keeps the machine-readable cause so
/// callers branch on a typed value instead of parsing `reason` text.
#[derive(Clone, Debug)]
pub struct Error {
    pub reason: String,
    kind: ErrorKind,
}

/// Machine-readable cause of an [`Error`].
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ErrorKind {
    SourceCorrespondence(Box<crate::text::analysis::SourceError>),
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
    /// A child process was stopped for using more memory than its cap
    /// allows (a language server over its resident-memory cap); running it
    /// again over the same scope would do the same.
    ResourceExhausted,
    /// A caller-supplied filter value is malformed (a glob, a file type,
    /// a size bound).
    InvalidInput,
    /// A caller-supplied regex does not compile.
    InvalidPattern,
    /// The path the operation starts from does not exist.
    NotFound,
    /// The path the operation starts from cannot be read.
    PermissionDenied,
    /// A caller-supplied glob does not compile (see `search::glob`).
    InvalidGlob,
    /// The caller's stop function ended the operation (see
    /// `search::walk::Stop`).
    Cancelled,
}

/// The message of clangd's `invalid AST` error (see [`RpcError::is_invalid_ast`]).
const INVALID_AST_MESSAGE: &str = "invalid AST";

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

    /// `true` for clangd's answer when it holds no AST for the document: the
    /// document's compile command could not be turned into a compiler
    /// invocation, or no command applies. LSP has no code for it: clangd
    /// replies `-32001` (UnknownErrorCode) with this exact message.
    pub fn is_invalid_ast(&self) -> bool {
        self.code == ErrorCode::UnknownErrorCode && self.message == INVALID_AST_MESSAGE
    }

    /// `true` when the request may be re-sent as-is: ContentModified always;
    /// ServerCancelled only when the server set `data.retriggerRequest`.
    pub(crate) fn is_retryable(&self) -> bool {
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
    pub(crate) fn from_i64(code: i64) -> Self {
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
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
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
            reason,
            kind: ErrorKind::Rpc(Box::new(error)),
        }
    }

    /// An error of the given kind.
    pub fn with_kind(kind: ErrorKind, reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            kind,
        }
    }

    /// A failed filesystem access, classified by its [`std::io::ErrorKind`].
    pub fn io(error: &std::io::Error, reason: impl Into<String>) -> Self {
        let kind = match error.kind() {
            std::io::ErrorKind::NotFound => ErrorKind::NotFound,
            std::io::ErrorKind::PermissionDenied => ErrorKind::PermissionDenied,
            _ => ErrorKind::Other,
        };
        Self::with_kind(kind, reason)
    }

    pub fn timeout(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            kind: ErrorKind::Timeout,
        }
    }

    pub fn connection_closed(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            kind: ErrorKind::ConnectionClosed,
        }
    }

    pub fn resource_exhausted(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            kind: ErrorKind::ResourceExhausted,
        }
    }

    /// The walk was stopped by its caller.
    pub fn cancelled() -> Self {
        Self::with_kind(ErrorKind::Cancelled, "cancelled")
    }

    pub fn kind(&self) -> &ErrorKind {
        &self.kind
    }

    /// The typed JSON-RPC error, when the peer returned one.
    pub(crate) fn rpc_error(&self) -> Option<&RpcError> {
        match &self.kind {
            ErrorKind::Rpc(error) => Some(error),
            _ => None,
        }
    }
}

impl Display for Error {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.reason)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

impl From<crate::text::analysis::SourceError> for Error {
    fn from(source: crate::text::analysis::SourceError) -> Self {
        let reason = source.to_string();
        Self::with_kind(ErrorKind::SourceCorrespondence(Box::new(source)), reason)
    }
}

#[cfg(test)]
mod tests;
