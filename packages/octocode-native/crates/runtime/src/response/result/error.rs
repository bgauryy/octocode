use super::Continuation;
use crate::id::ToolId;
use crate::id::error_codes::{self, ErrorClass};
use octocode_contract_types::ErrorCode;
use serde_json::Value;

/// How a call failed, for the caller's exit code and error state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum FailureKind {
    NotFound,
    Authentication,
    Permission,
    RateLimited,
    Execution,
}

/// The one text of a stale snapshot, whichever tool's cursor went stale.
pub(crate) const STALE_SNAPSHOT_ERROR: &str = "The snapshot does not match this source and query (they changed, or the snapshot is missing or from another call); nothing shown remains valid: run the restart call, page 1 without the snapshot.";

/// The failure an error row with `code` reports. A rejected request is an
/// execution failure here; [`crate::response::exit::ExitClass::InvalidInput`] is
/// decided from the rows.
pub(crate) fn failure_kind(code: ErrorCode) -> FailureKind {
    match error_codes::class(code) {
        ErrorClass::NotFound => FailureKind::NotFound,
        ErrorClass::Authentication => FailureKind::Authentication,
        ErrorClass::Permission => FailureKind::Permission,
        ErrorClass::RateLimited => FailureKind::RateLimited,
        ErrorClass::InvalidInput | ErrorClass::Execution => FailureKind::Execution,
    }
}

/// A local tool call that failed as a whole. The dispatcher renders it as
/// the one error row: `errorCode`, `error`, then `hints` and `next` when set.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolError {
    pub code: ErrorCode,
    pub message: String,
    pub hints: Vec<String>,
    /// Leads out of the error, e.g. a missing path's nearest existing parent.
    /// Boxed to keep the error small in `Result`s.
    pub next: Option<Box<Value>>,
    /// The coverage receipt of a classified row whose selection failed.
    pub selection: Option<Box<octocode_contract_types::Selection>>,
}

impl ToolError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            hints: Vec::new(),
            next: None,
            selection: None,
        }
    }

    /// A failed graph-facts scan: the engine's typed refusals keep their
    /// declared codes, and a walk stopped by `cancel` is a cancellation.
    pub(crate) fn graph_scan(
        error: &octocode_engine::fact_scan::GraphScanError,
        cancel: &dyn crate::execution::cancel::CancellationCheck,
    ) -> Self {
        use octocode_engine::fact_scan::GraphScanError;
        let code = match error {
            GraphScanError::UnsupportedLanguage(_) => ErrorCode::LanguageUnsupported,
            GraphScanError::InvalidGlob(_) => ErrorCode::InvalidGlob,
            GraphScanError::Failed(_) if cancel.check().is_err() => ErrorCode::Cancelled,
            GraphScanError::Failed(_) => ErrorCode::ScanFailed,
        };
        Self::new(code, error.to_string())
    }

    /// A continuation whose snapshot no longer describes the source:
    /// `restart` (usually [`restart_query`]) is `next.restart`. The runtime
    /// writes the one shared `error` text and clears `isPartial`
    /// (`response::pages::restart_stale`); a tool never words it.
    pub(crate) fn stale_snapshot(restart: Value) -> Self {
        Self {
            next: Some(Box::new(serde_json::json!({ "restart": restart }))),
            ..Self::new(ErrorCode::StaleSnapshot, STALE_SNAPSHOT_ERROR)
        }
    }

    /// The call was cancelled or ran past its deadline.
    pub fn cancelled(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Cancelled, message)
    }

    /// A path-policy refusal under the one code every local tool gives it
    /// ([`crate::policy::PolicyError::shared_code`]).
    pub fn policy(error: crate::policy::PolicyError) -> Self {
        Self::new(error.shared_code(), error.message)
    }

    /// [`Self::policy`] for a search root; a missing root also leads to a
    /// tree of its nearest existing parent (`next.viewTree`), where a typo's
    /// siblings show.
    pub(crate) fn root_policy(
        error: crate::policy::PolicyError,
        requested: &str,
        paths: &crate::policy::path::PathPolicy,
    ) -> Self {
        let missing = error.code == crate::policy::PolicyErrorCode::NotFound;
        let mut out = Self::policy(error);
        if missing && let Some(next) = missing_path_lead(paths, requested) {
            out.next = Some(Box::new(next));
        }
        out
    }
}

/// `{ "viewTree": <lead> }` to the tree of `requested`'s nearest existing
/// parent, where a mistyped name's siblings show; the one recovery form every
/// local tool gives a missing path.
pub(crate) fn missing_path_lead(
    paths: &crate::policy::path::PathPolicy,
    requested: &str,
) -> Option<serde_json::Value> {
    let parent = paths.nearest_existing_dir(requested)?;
    let lead = Continuation::new(
        ToolId::StructureSearch,
        serde_json::json!({"operation": "tree", "path": parent}),
    )
    .build();
    Some(serde_json::json!({ "viewTree": lead }))
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ToolError {}
