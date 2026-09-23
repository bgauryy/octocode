//! Classification provider abstraction.
//!
//! `clasify` sends evidence to a classification vendor for a typed
//! judgment. Each vendor's built-in defaults (API host, model, endpoint path,
//! wire format) and response contract live behind [`ClassificationProvider`],
//! keyed by the vendor-neutral `classification.type` config selector.
//!
//! # Adding a vendor
//! 1. Create `providers/classification/<vendor>.rs` implementing this trait.
//! 2. Expose a `pub static VENDOR: Vendor = Vendor;` singleton.
//! 3. Add one arm to [`provider_for`].
//! 4. Add the vendor id to the `classification.type` enum in the config contract.
//!
//! Nothing else changes: transport, batch scheduling, context capture, and the
//! clasify engine are all vendor-agnostic.
pub mod jev;

pub(crate) mod gate;

use serde_json::Value;

/// Vendor-neutral response-contract violation reported by
/// [`ClassificationProvider::validate_response`]. Vendors map their own
/// validator errors into this so vendor names never reach public error codes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderContractError {
    /// `true` when the request (not the vendor response) broke the contract.
    pub request: bool,
    pub message: String,
}

impl ProviderContractError {
    /// Stable public error code for this violation.
    #[must_use]
    pub fn code(&self) -> &'static str {
        if self.request {
            "invalidClassificationRequest"
        } else {
            "invalidClassificationResponse"
        }
    }
}

/// A classification vendor: its built-in defaults, wire format, and response
/// contract. Every method is called from the generic clasify engine; vendor
/// logic stays exclusively inside the impl.
pub trait ClassificationProvider: Send + Sync {
    /// Stable vendor identifier, matching the `classification.type` value.
    fn id(&self) -> &'static str;

    /// Vendor-native credential env var, accepted in addition to the generic
    /// `OCTOCODE_CLASSIFICATION_API` (e.g. `jev` also honors `OCTOCODE_JEV_KEY`).
    fn key_env(&self) -> &'static str;

    /// Default API root used when `OCTOCODE_CLASSIFICATION_API_HOST` is unset.
    fn default_host(&self) -> &'static str;

    /// Model sent to the vendor (no user-facing override).
    fn default_model(&self) -> &'static str;

    /// Path joined onto the API root to reach the vendor's endpoint.
    fn endpoint_path(&self) -> &'static str;

    /// Link to vendor setup docs; shown in `missingConfiguration` errors.
    fn docs_url(&self) -> &'static str;

    /// Build the JSON body sent for a **single-question** request.
    /// The engine calls this once per state × question cell; the shape is
    /// entirely vendor-defined.
    fn build_request(&self, state: &Value, question: &Value, model: &str) -> Value;

    /// Build the JSON body for a **multi-question batch** over the same state.
    /// `questions` is a slice of `(original_index, &question_value)` pairs;
    /// the index is available for key naming (e.g. `answer_0`, `answer_1`).
    fn build_batch_request(
        &self,
        state: &Value,
        questions: &[(usize, &Value)],
        model: &str,
    ) -> Value;

    /// Extract the single answer object from a **non-batch** vendor response.
    /// Returns `None` when the response does not contain the expected answer,
    /// which the engine treats as an `invalidClassificationResponse` error.
    fn extract_answer<'a>(&self, response: &'a Value) -> Option<&'a Value>;

    /// Return the answer key used for question at `index` inside a batch
    /// request/response (e.g. `"answer_0"` for Jev).
    fn batch_answer_key(&self, index: usize) -> String;

    /// Validate the vendor's answer shape against the requested rubric.
    fn validate_response(
        &self,
        request: &Value,
        response: &Value,
    ) -> Result<(), ProviderContractError>;
}

/// Resolve the provider for a `classification.type` selector. Unknown values
/// fall back to the default vendor (`jev`); config validation constrains the
/// resolved value to the declared enum, so the fallback only guards misuse.
#[must_use]
pub fn provider_for(_vendor: &str) -> &'static dyn ClassificationProvider {
    // Only `jev` is registered today; add a `match` arm per vendor id here.
    &jev::JEV
}
