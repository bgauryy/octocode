//! GitHub file and tree reads through one transport.
use bytes::Bytes;

use super::GitHubTransport;

mod file;
mod listing;
mod payload;
mod reference;

pub use payload::decode_content;

#[derive(Clone, Debug)]
pub struct ContentRequest {
    pub owner: String,
    pub repo: String,
    pub path: String,
    pub reference: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContentResponse {
    /// Shared with the HTTP response buffer; raw reads copy no body.
    pub bytes: Bytes,
    pub resolved_ref: String,
    pub raw_response_bytes: usize,
    /// The git blob SHA of the bytes: evidence cited when a caller refuses them.
    pub blob: Option<String>,
}
/// A submodule entry of a repository: the clone URL `.gitmodules` names
/// for it and the commit its parent pins.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Submodule {
    pub url: Option<String>,
    pub commit: Option<String>,
}
/// GitHub reads through one transport.
pub struct GitHubProvider {
    pub transport: GitHubTransport,
}
impl GitHubProvider {
    /// A provider reading through `transport`.
    pub fn new(transport: GitHubTransport) -> Self {
        Self { transport }
    }
}

/// A full 40-hex-digit commit SHA (either case).
pub fn is_full_sha(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
