use std::collections::HashMap;

use napi_derive::napi;

/// Precise position of one captured metavariable node. Line is 1-based (usable
/// as an `lspGetSemantics` `lineHint`); columns are 0-based UTF-16 code-unit
/// offsets — the same convention as `StructuralMatch.start_col` and LSP.
#[napi(object)]
pub struct MetavarRange {
    pub text: String,
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

/// One structural match. Line numbers are 1-based so `start_line` can be fed
/// directly as an `lspGetSemantics` `lineHint`; columns are 0-based UTF-16
/// code-unit offsets (converted from tree-sitter's native byte columns to match
/// the resolver, signatures, and LSP layers).
#[napi(object)]
pub struct StructuralMatch {
    pub start_line: u32,
    pub end_line: u32,
    pub start_col: u32,
    pub end_col: u32,
    pub text: String,
    /// Captured metavariables. `$X` yields a single-element list;
    /// `$$$ARGS` yields the full list of captured nodes. Keyed by the bare
    /// metavar name (no leading `$`).
    pub metavars: HashMap<String, Vec<String>>,
    /// Per-capture precise ranges, parallel to `metavars` (same keys, same
    /// order). Lets an agent hand a capture straight to LSP without re-search.
    pub metavar_ranges: HashMap<String, Vec<MetavarRange>>,
}

#[napi(object)]
pub struct StructuralDiagnostic {
    pub code: String,
    pub severity: String,
    pub stage: String,
    pub message: String,
    pub path: Option<String>,
    pub recovery: Option<String>,
}

#[napi(object)]
pub struct StructuralQueryExplanation {
    pub kind: String,
    pub source: String,
    pub literal_anchor: Option<String>,
    pub pre_filter: String,
    pub unsafe_reason: Option<String>,
    pub diagnostics: Vec<StructuralDiagnostic>,
}

/// A structural match with stable evidence metadata. Existing
/// `StructuralMatch` remains unchanged for the legacy APIs; detailed APIs add
/// IDs and confidence without forcing old callers to carry metadata.
#[napi(object)]
pub struct StructuralDetailedMatch {
    pub id: String,
    pub start_line: u32,
    pub end_line: u32,
    pub start_col: u32,
    pub end_col: u32,
    pub text: String,
    pub metavars: HashMap<String, Vec<String>>,
    pub metavar_ranges: HashMap<String, Vec<MetavarRange>>,
    pub node_kind: Option<String>,
    pub confidence: String,
}

#[napi(object)]
pub struct StructuralSearchFilesOptions {
    pub path: String,
    pub pattern: Option<String>,
    pub rule: Option<String>,
    pub include: Option<Vec<String>>,
    /// File-path globs to skip (gitignore-style, e.g. `"*.min.js"`, `"src/gen/**"`).
    /// Mirrors `localSearchCode.exclude` so OQL `scope.exclude` is honored on the
    /// structural lane — previously silently dropped (typed-contract violation).
    pub exclude: Option<Vec<String>>,
    pub exclude_dir: Option<Vec<String>>,
    /// Include hidden (dot) files. `None` preserves the default walker behavior
    /// (hidden ignored); `Some(true)` forces them in.
    pub hidden: Option<bool>,
    /// Bypass `.gitignore`/`.ignore` rules. `None` preserves defaults; `Some(true)`
    /// searches files normally hidden by ignore files (mirrors `localSearchCode.noIgnore`).
    pub no_ignore: Option<bool>,
    /// Maximum directory descent depth (0 = just the root). `None` = unbounded.
    pub max_depth: Option<u32>,
    pub max_files: Option<u32>,
    pub max_file_bytes: Option<u32>,
}

#[napi(object)]
pub struct StructuralSearchFileResult {
    pub path: String,
    pub matches: Vec<StructuralMatch>,
}

#[napi(object)]
pub struct StructuralSearchFilesResult {
    pub files: Vec<StructuralSearchFileResult>,
    pub total_matches: u32,
    pub parsed_files: u32,
    pub skipped_by_pre_filter: u32,
    /// Candidate files whose extension has no grammar — not evaluated, hence
    /// not proof of absence. Mirrors the detailed result's counter so the two
    /// shapes agree and the warning text can't collapse unevaluated into
    /// anchor-absent.
    pub skipped_unsupported: u32,
    pub skipped_unreadable: u32,
    pub skipped_large: u32,
    pub warnings: Vec<String>,
}

#[napi(object)]
pub struct StructuralSearchDetailedResult {
    pub path: String,
    pub analyzer: String,
    pub analyzer_version: String,
    pub status: String,
    pub language_id: Option<String>,
    pub query: StructuralQueryExplanation,
    pub matches: Vec<StructuralDetailedMatch>,
    pub diagnostics: Vec<StructuralDiagnostic>,
}

#[napi(object)]
pub struct StructuralSearchDetailedFileResult {
    pub path: String,
    pub status: String,
    pub language_id: Option<String>,
    pub skipped_reason: Option<String>,
    pub matches: Vec<StructuralDetailedMatch>,
    pub diagnostics: Vec<StructuralDiagnostic>,
}

#[napi(object)]
pub struct StructuralSearchFilesDetailedResult {
    pub files: Vec<StructuralSearchDetailedFileResult>,
    pub total_matches: u32,
    pub parsed_files: u32,
    pub skipped_by_pre_filter: u32,
    pub skipped_unsupported: u32,
    pub skipped_unreadable: u32,
    pub skipped_large: u32,
    pub analyzer: String,
    pub analyzer_version: String,
    pub status: String,
    pub query: StructuralQueryExplanation,
    pub diagnostics: Vec<StructuralDiagnostic>,
    pub warnings: Vec<String>,
}

pub(super) const STRUCTURAL_ANALYZER: &str = "octocode-structural";
pub(super) const STRUCTURAL_ANALYZER_VERSION: &str = env!("CARGO_PKG_VERSION");

impl StructuralDiagnostic {
    pub(super) fn new(
        code: impl Into<String>,
        severity: impl Into<String>,
        stage: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            severity: severity.into(),
            stage: stage.into(),
            message: message.into(),
            path: None,
            recovery: None,
        }
    }

    pub(super) fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    pub(super) fn with_recovery(mut self, recovery: impl Into<String>) -> Self {
        self.recovery = Some(recovery.into());
        self
    }
}

impl StructuralDetailedMatch {
    pub(super) fn from_match(
        path: &str,
        query_fingerprint: &str,
        matched: StructuralMatch,
        node_kind: String,
    ) -> Self {
        let id = stable_match_id(path, query_fingerprint, &matched);
        Self {
            id,
            start_line: matched.start_line,
            end_line: matched.end_line,
            start_col: matched.start_col,
            end_col: matched.end_col,
            text: matched.text,
            metavars: matched.metavars,
            metavar_ranges: matched.metavar_ranges,
            node_kind: Some(node_kind),
            // The octo matcher is a precise AST matcher: every match is an exact
            // tree-sitter node match, so there is no partial/fallback tier to
            // report. Keep this in sync with the narrowed `confidence` union in
            // loader/index.d.ts.
            confidence: "exact-ast".to_owned(),
        }
    }
}

pub(super) fn structural_query_fingerprint(pattern: Option<&str>, rule: Option<&str>) -> String {
    match (pattern, rule) {
        (Some(pattern), None) => stable_hash_hex(&["pattern", pattern]),
        (None, Some(rule)) => stable_hash_hex(&["rule", rule]),
        (Some(pattern), Some(rule)) => stable_hash_hex(&["both", pattern, rule]),
        (None, None) => stable_hash_hex(&["none"]),
    }
}

fn stable_match_id(path: &str, query_fingerprint: &str, matched: &StructuralMatch) -> String {
    stable_hash_hex(&[
        STRUCTURAL_ANALYZER,
        STRUCTURAL_ANALYZER_VERSION,
        path,
        query_fingerprint,
        &matched.start_line.to_string(),
        &matched.end_line.to_string(),
        &matched.start_col.to_string(),
        &matched.end_col.to_string(),
    ])
}

fn stable_hash_hex(parts: &[&str]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for part in parts {
        for byte in part.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}
