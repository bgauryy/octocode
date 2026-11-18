//! Classification provider abstraction.
//!
//! `clasify` sends evidence to a classification vendor for a typed
//! judgment. Each vendor's built-in defaults (API host, model, endpoint path,
//! wire format) and response contract live behind [`ClassificationProvider`].
//! One provider is built in ([`provider`]); no setting selects it.
//!
//! Transport, batch scheduling, context capture, and the clasify engine are
//! vendor-agnostic: a second vendor needs only another impl and a setting to
//! choose between them.
pub mod jev;

pub mod gate;
pub mod transport;

use octocode_contract_types::ErrorCode;
use serde_json::Value;

/// The code of a provider that could not answer: unreachable, timed out,
/// failing (5xx), overloaded without a retry delay, or paused by the open
/// circuit those failures led to. Unlike a refused credential or request,
/// it says nothing about the caller's input: retrying later can succeed.
pub const CIRCUIT_OPEN: ErrorCode = ErrorCode::ClassificationProviderUnavailable;

/// Whether a classification error `code` says the provider could not answer
/// now (rate limited, overloaded, unreachable, failing, too slow, paused by
/// its circuit) rather than refusing this caller or request (a credential,
/// quota, content or size error the caller must fix). Evidence a reader
/// already has is never withheld for the former.
pub fn is_transient(code: ErrorCode) -> bool {
    matches!(
        code,
        CIRCUIT_OPEN | ErrorCode::ClassificationRateLimited | ErrorCode::Timeout
    )
}

/// Vendor-neutral response-contract violation reported by
/// [`ClassificationProvider::read`] and [`ClassificationProvider::models`].
/// Vendors map their own validator errors into this so vendor names never
/// reach public error codes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderContractError {
    /// `true` when the request (not the vendor response) broke the contract.
    pub request: bool,
    pub message: String,
}

impl ProviderContractError {
    /// Stable public error code for this violation.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        if self.request {
            ErrorCode::InvalidClassificationRequest
        } else {
            ErrorCode::InvalidClassificationResponse
        }
    }
}

/// A vendor's built-in defaults.
#[derive(Debug)]
pub struct ProviderDefaults {
    /// API root used when `OCTOCODE_CLASSIFICATION_API_HOST` is unset.
    pub host: &'static str,
    /// Model used when neither the query nor configuration selects one.
    pub model: &'static str,
    /// Path joined onto the API root to reach the classify endpoint.
    pub endpoint_path: &'static str,
    /// Path of the read-only model catalog on the same API root.
    pub models_path: &'static str,
    /// The vendor's documented default window in tokens, the smaller of its
    /// per-request and per-question contexts. It sizes the sections a source
    /// the window refused is split into; whether a request fits stays the
    /// provider's call.
    pub context_tokens: u64,
    /// The vendor's documented default per-request window in tokens: one
    /// request's state and every question it holds together. Requests that
    /// share a state across units size it from this and `context_tokens`.
    pub request_tokens: u64,
    /// The vendor's documented long-context window in tokens, read at its
    /// lower bound. The runtime retries a refused request there on its own
    /// only when the retry provably fits it.
    pub long_context_tokens: u64,
}

impl ProviderDefaults {
    /// The window of one question over a state (`long`: the long-context
    /// one, whose per-question and per-request contexts are the same).
    pub fn question_window(&self, long: bool) -> u64 {
        if long {
            self.long_context_tokens
        } else {
            self.context_tokens
        }
    }

    /// The window of one whole request: its state and all its questions.
    pub fn request_window(&self, long: bool) -> u64 {
        if long {
            self.long_context_tokens
        } else {
            self.request_tokens
        }
    }
}

/// Token usage a response reports.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProviderUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// What one vendor response holds for the request it answers.
#[derive(Debug)]
pub struct ProviderResponse {
    /// One answer per requested question, in request order; a malformed or
    /// missing answer fails only its own question. `Err` fails the whole
    /// response (its metadata, or an answer the request never asked for).
    pub answers: Result<Vec<Result<Value, ProviderContractError>>, ProviderContractError>,
    /// The model that answered, as reported.
    pub model: Option<String>,
    /// Token usage, as reported, even on a response that failed validation.
    pub usage: Option<ProviderUsage>,
}

/// A classification vendor: its built-in defaults, wire format and response
/// contract. The generic clasify engine calls only these; request and
/// response shapes (answer keys, usage and model fields) stay inside the impl.
pub trait ClassificationProvider: Send + Sync {
    /// API root, model, endpoint paths and setup docs.
    fn defaults(&self) -> &'static ProviderDefaults;

    /// Provider capability opt-ins required by the prepared request.
    fn headers(&self, images: bool, long_context: bool) -> reqwest::header::HeaderMap;

    /// The JSON body asking `questions` (at least one) over one `state`, with
    /// optional image `assets`; the shape is entirely vendor-defined.
    fn build(
        &self,
        state: &Value,
        questions: &[&Value],
        model: &str,
        assets: Option<&Value>,
    ) -> Value;

    /// Validate `response` against the `request` [`Self::build`] made: each
    /// answer against its question's rubric, plus the reported model and usage.
    fn read(&self, request: &Value, response: &Value) -> ProviderResponse;

    /// The model catalog in the clasify `models` output shape, after
    /// validating it without interpreting model names or dates.
    fn models(&self, response: &Value) -> Result<Value, ProviderContractError>;
}

/// The built-in classification provider.
#[must_use]
pub fn provider() -> &'static dyn ClassificationProvider {
    &jev::JEV
}

#[cfg(test)]
mod tests;
