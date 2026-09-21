//! Classification provider abstraction.
//!
//! `clasify` sends evidence to a classification vendor for a typed
//! judgment. Each vendor's built-in defaults (API host, model, endpoint path)
//! and response contract live behind [`ClassificationProvider`], keyed by the
//! vendor-neutral `classification.type` config selector. Adding a vendor is a
//! new trait impl plus one arm in [`provider_for`] — not edits scattered across
//! the engine and transport layers.
pub mod jev;

use octocode_engine::jev::JevError;
use serde_json::Value;

/// A classification vendor: its built-in defaults and response contract.
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
    /// Validate the vendor's answer shape against the requested rubric.
    fn validate_response(&self, request: &Value, response: &Value) -> Result<(), JevError>;
}

/// Resolve the provider for a `classification.type` selector. Unknown values
/// fall back to the default vendor (`jev`); config validation constrains the
/// resolved value to the declared enum, so the fallback only guards misuse.
#[must_use]
pub fn provider_for(vendor: &str) -> &'static dyn ClassificationProvider {
    match vendor {
        "jev" => &jev::JEV,
        _ => &jev::JEV,
    }
}
