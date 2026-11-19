use super::Continuation;
use crate::id::ToolId;
use octocode_contract_types::ErrorCode;
use serde_json::Value;

/// The one text of a stale snapshot, whichever tool's cursor went stale.
pub const STALE_SNAPSHOT_ERROR: &str = "The snapshot does not match this source and query (they changed, or the snapshot is missing or from another call); nothing shown remains valid: run the restart call, page 1 without the snapshot.";

/// A local tool call that failed as a whole. The dispatcher renders it as
/// the one error row: `errorCode`, `error`, then `hints` and `next` when set.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolError {
    pub code: ErrorCode,
    pub message: String,
    pub hints: Vec<String>,
    /// Leads out of the error, e.g. a missing path's nearest existing parent.
    /// Boxed to keep the error small in `Result`s.
    pub calls: Option<Box<Value>>,
    /// The coverage receipt of a classified row whose selection failed.
    pub selection: Option<Box<octocode_contract_types::Selection>>,
}

impl ToolError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            hints: Vec::new(),
            calls: None,
            selection: None,
        }
    }

    /// An engine error as a tool error, by its typed kind (never by message
    /// text): the one mapping every local tool shares; a tool words only its
    /// own cases (a missing path's sentence, a held-past-budget refusal)
    /// before it.
    pub fn engine(error: &octocode_engine::error::Error) -> Self {
        use octocode_engine::error::ErrorKind;
        let code = match error.kind() {
            ErrorKind::NotFound => ErrorCode::NotFound,
            ErrorKind::PermissionDenied => ErrorCode::PermissionDenied,
            ErrorKind::InvalidPattern => ErrorCode::InvalidPattern,
            ErrorKind::InvalidGlob => ErrorCode::InvalidGlob,
            ErrorKind::InvalidInput => ErrorCode::InvalidInput,
            ErrorKind::Timeout => ErrorCode::Timeout,
            ErrorKind::Cancelled => ErrorCode::Cancelled,
            ErrorKind::SourceCorrespondence(error) => return Self::source(*error.clone()),
            _ => ErrorCode::ExecutionFailed,
        };
        Self::new(code, error.to_string())
    }

    /// A failed graph-facts scan: the engine's typed refusals keep their
    /// declared codes, and a walk stopped by `cancel` is a cancellation.
    pub fn graph_scan(
        error: &octocode_engine::fact_scan::GraphScanError,
        cancel: &dyn crate::execution::cancel::CancellationCheck,
    ) -> Self {
        use octocode_engine::fact_scan::GraphScanError;
        let code = match error {
            GraphScanError::Source(error) => return Self::source(error.clone()),
            GraphScanError::UnsupportedLanguage(_) => ErrorCode::LanguageUnsupported,
            GraphScanError::InvalidGlob(_) => ErrorCode::InvalidGlob,
            GraphScanError::Failed(_) if cancel.check().is_err() => ErrorCode::Cancelled,
            GraphScanError::Failed(_) => ErrorCode::ScanFailed,
        };
        Self::new(code, error.to_string())
    }

    pub fn source(error: octocode_engine::text::analysis::SourceError) -> Self {
        use octocode_engine::text::{Refusal, analysis::SourceError};
        let code = match &error {
            SourceError::Cancelled => ErrorCode::Cancelled,
            SourceError::Deadline => ErrorCode::Timeout,
            SourceError::LimitExceeded(_) => ErrorCode::ScopeTooBroad,
            SourceError::Refused(Refusal::Binary) => ErrorCode::BinaryFileUnsupported,
            SourceError::Refused(Refusal::NotUnicode) => ErrorCode::Decode,
            SourceError::InvalidMap(_) => ErrorCode::ExecutionFailed,
        };
        Self::new(code, error.to_string())
    }

    /// A continuation whose snapshot no longer describes the source:
    /// `restart` (usually [`restart_query`](super::restart_query)) is `next.restart`. The `error`
    /// text is the one shared sentence ([`STALE_SNAPSHOT_ERROR`]), and the
    /// runtime requires the restart of every such row
    /// (`response::rows::require_restart`); a tool never words it.
    pub fn stale_snapshot(restart: Value) -> Self {
        Self {
            calls: Some(Box::new(serde_json::json!({ "restart": restart }))),
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
    pub fn root_policy(
        error: crate::policy::PolicyError,
        requested: &str,
        paths: &crate::policy::path::PathPolicy,
    ) -> Self {
        let missing = error.code == crate::policy::PolicyErrorCode::NotFound;
        let mut out = Self::policy(error);
        if missing && let Some(next) = missing_path_lead(paths, requested) {
            out.calls = Some(Box::new(next));
        }
        out
    }
}

/// `{ "viewTree": <lead> }` to the tree of `requested`'s nearest existing
/// parent, where a mistyped name's siblings show; the one recovery form every
/// local tool gives a missing path.
pub fn missing_path_lead(
    paths: &crate::policy::path::PathPolicy,
    requested: &str,
) -> Option<serde_json::Value> {
    let parent = paths.nearest_existing_dir(requested)?;
    let lead = Continuation::related(
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

#[cfg(test)]
mod source_error_tests;
