mod declarations_cache;
mod matches;
#[cfg(test)]
mod policy_tests;
mod symbols;
mod syntax;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use crate::contracts::tool_types::AstSearchQuery;
use matches::MatchQuery;

use crate::policy::{PolicyError, PolicyErrorCode};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AstError {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hints: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<Box<Value>>,
}

impl AstError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            hints: Vec::new(),
            next: None,
        }
    }
}
impl std::fmt::Display for AstError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for AstError {}
impl From<String> for AstError {
    fn from(message: String) -> Self {
        Self::new("ast.execution.failed", message)
    }
}
impl From<&str> for AstError {
    fn from(message: &str) -> Self {
        Self::from(message.to_owned())
    }
}

impl From<PolicyError> for AstError {
    fn from(error: PolicyError) -> Self {
        let suffix = match error.code {
            PolicyErrorCode::EmptyPath => "emptyPath",
            PolicyErrorCode::OutsideAllowedRoots => "outsideAllowedRoots",
            PolicyErrorCode::SymlinkEscape => "symlinkEscape",
            PolicyErrorCode::IgnoredPath => "ignoredPath",
            PolicyErrorCode::PermissionDenied => "permissionDenied",
            PolicyErrorCode::SymlinkLoop => "symlinkLoop",
            PolicyErrorCode::NameTooLong => "nameTooLong",
            PolicyErrorCode::NotRegular => "notRegular",
            PolicyErrorCode::NotFound => "notFound",
            PolicyErrorCode::InvalidInput => "invalidInput",
            PolicyErrorCode::InputTooLarge => "inputTooLarge",
            PolicyErrorCode::BinaryContent => "binaryContent",
            PolicyErrorCode::Io => "io",
        };
        Self::new(format!("ast.policy.{suffix}"), error.message)
    }
}

pub(super) fn cancelled(error: String) -> AstError {
    AstError::new("ast.execution.cancelled", error)
}

pub(super) fn io_error(error: std::io::Error) -> AstError {
    let code = match error.kind() {
        std::io::ErrorKind::NotFound => "ast.policy.notFound",
        std::io::ErrorKind::PermissionDenied => "ast.policy.permissionDenied",
        std::io::ErrorKind::InvalidInput => "ast.policy.invalidInput",
        _ => "ast.execution.io",
    };
    AstError::new(code, error.to_string())
}

pub(super) fn native_error(error: impl ToString) -> AstError {
    let message = error.to_string();
    if let Some(rest) = message.strip_prefix('[')
        && let Some((code, _)) = rest.split_once(']')
        && (code.starts_with("structural.") || code.starts_with("ast."))
    {
        return AstError::new(code.to_owned(), message);
    }
    let lower = message.to_ascii_lowercase();
    let code = if lower.contains("no such file") || lower.contains("not found") {
        "ast.policy.notFound"
    } else if lower.contains("permission denied") {
        "ast.policy.permissionDenied"
    } else if lower.contains("invalid") && lower.contains("regex") {
        "ast.query.invalidPattern"
    } else {
        "ast.execution.failed"
    };
    AstError::new(code, message)
}

pub(super) fn allow_discovery(
    path: &std::path::Path,
    paths: &crate::policy::path::PathPolicy,
    cancel: &dyn crate::tools::cancel::CancellationCheck,
) -> Result<bool, String> {
    cancel
        .check()
        .map_err(|message| format!("[ast.execution.cancelled] {message}"))?;
    Ok(paths.permits_discovery(path))
}

pub(super) fn display_name(path: &std::path::Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// A `.h` suffix is shared by C and C++. Keep C as the default and allow an
/// explicit C++ parser only for a single ambiguous header.
pub(super) fn cpp_header_override(path: &std::path::Path, selector: Option<&str>) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("h"))
        && selector.is_some_and(|language| {
            language.eq_ignore_ascii_case("cpp") || language.eq_ignore_ascii_case("c++")
        })
}

pub(super) fn validate_file_language(
    path: &std::path::Path,
    selector: Option<&str>,
) -> Result<(), AstError> {
    if let Some(language) = selector {
        if language.trim().is_empty() || matches!(language, ".") {
            return Err(AstError::new(
                "ast.language.unsupported",
                "langType must name a registered grammar.",
            ));
        }
        let selected = matches::language_extensions(language).ok_or_else(|| {
            AstError::new(
                "ast.language.unsupported",
                format!("langType \"{language}\" is not a supported structural grammar."),
            )
        })?;
        if !matches::has_extension_in(path, &selected) && !cpp_header_override(path, selector) {
            return Err(AstError::new(
                "ast.language.mismatch",
                format!(
                    "{} is not a {language} source file; omit langType or choose the grammar matching its extension.",
                    display_name(path)
                ),
            ));
        }
    }
    Ok(())
}

/// Row path for a descendant of a directory scope: `{root_name}/{relative}`.
/// The runtime attaches `base = parent(scope)`, so `base + path` resolves to
/// the real file for every operation (match, symbols).
pub(super) fn rooted_display(root: &std::path::Path, relative: &std::path::Path) -> String {
    let root_name = display_name(root);
    if relative.as_os_str().is_empty() {
        root_name
    } else {
        format!("{root_name}/{}", relative.to_string_lossy())
    }
}

/// Shared `ast.snapshot.changed` continuation guard payload. Emitted when a
/// page>1 request carries a snapshot that no longer matches the freshly
/// computed digest of the query shape and ordered result set — i.e. the corpus
/// or query changed underneath a continuation cursor.
pub(super) fn snapshot_changed(snapshot: &str) -> Value {
    serde_json::json!({
        "status":"error",
        "errorCode":"ast.snapshot.changed",
        "error":"The source or query changed, or this continuation omitted its snapshot. Discard earlier pages and restart.",
        "snapshot":snapshot,
        "complete":false
    })
}
pub type AstResult = Result<Value, AstError>;

/// Largest source astSearch parses (match, symbols, syntaxTree). Sized for
/// real generated and monolithic sources (TypeScript's 3 MB checker.ts, the
/// 2.3 MB lib.dom.d.ts); tree-sitter and oxc parse these in well under a
/// second. Directory scans skip larger files and report them.
pub(crate) const MAX_PARSE_SOURCE_BYTES: usize = octocode_engine::signatures::MAX_PARSE_SIZE;

/// Execute one typed row. The runtime parses the validated row with its
/// shared `parse_query`, so a shape mismatch has one code across tools.
pub fn execute_ast(
    query: &AstSearchQuery,
    paths: &crate::policy::path::PathPolicy,
    security: &crate::security::ContentSecurity,
    cancellation: &dyn crate::tools::cancel::CancellationCheck,
) -> AstResult {
    match query {
        AstSearchQuery::Symbols(query) => execute_symbols(query, paths, security, cancellation),
        AstSearchQuery::MatchPattern(query) => {
            execute_match(MatchQuery::Pattern(query), paths, security, cancellation)
        }
        AstSearchQuery::MatchRule(query) => {
            execute_match(MatchQuery::Rule(query), paths, security, cancellation)
        }
        AstSearchQuery::SyntaxTree(query) => execute_syntax(query, paths, security, cancellation),
    }
}

pub use matches::execute_match;
pub use symbols::execute_symbols;
pub use syntax::execute_syntax;

#[cfg(test)]
mod tests {

    use serde_json::json;

    use super::*;
    use crate::{
        policy::path::{PathPolicy, PathPolicyConfig},
        security::ContentSecurity,
        tools::cancel::CancellationCheck,
    };

    struct Cancelled;
    impl CancellationCheck for Cancelled {
        fn check(&self) -> Result<(), String> {
            Err("cancelled by test".to_owned())
        }
    }

    struct Active;
    impl CancellationCheck for Active {
        fn check(&self) -> Result<(), String> {
            Ok(())
        }
    }

    /// Tests speak JSON rows; the runtime owns the typed parse.
    fn execute_row(
        query: Value,
        paths: &PathPolicy,
        security: &ContentSecurity,
        cancellation: &dyn CancellationCheck,
    ) -> AstResult {
        let query: AstSearchQuery = serde_json::from_value(query).expect("typed astSearch row");
        execute_ast(&query, paths, security, cancellation)
    }

    fn context() -> (PathPolicy, ContentSecurity) {
        (
            PathPolicy::new(PathPolicyConfig::default()).expect("default path policy"),
            ContentSecurity::new(),
        )
    }

    #[test]
    fn input_shape_and_cancellation_are_rejected() {
        let (paths, security) = context();
        // Shape mismatches never reach the tool: the runtime's typed parse
        // rejects them (reported as `invalidInput`).
        for row in [
            json!({"path":"."}),
            json!({"operation":"symbols","goal": "test", "reasoning":"test","path":".","unknown":true}),
        ] {
            assert!(serde_json::from_value::<AstSearchQuery>(row).is_err());
        }

        let cancelled = execute_row(
            json!({"operation":"symbols","goal": "test", "reasoning":"test","path":"."}),
            &paths,
            &security,
            &Cancelled,
        )
        .expect_err("cancelled before filesystem access");
        assert_eq!(cancelled.code, "ast.execution.cancelled");
    }

    #[test]
    fn syntax_tree_is_the_only_tree_surface() {
        let root = tempfile::tempdir().expect("fixture directory");
        let source = root.path().join("fixture.ts");
        std::fs::write(&source, "export const value = 1;\n").expect("fixture source");
        let paths = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(root.path().to_path_buf()),
            ..Default::default()
        })
        .expect("fixture path policy");
        let security = ContentSecurity::new();

        let syntax = execute_row(
            json!({
                "operation":"syntaxTree","goal": "test", "reasoning":"test",
                "path":source.to_string_lossy()
            }),
            &paths,
            &security,
            &Active,
        )
        .expect("syntax tree");
        assert_eq!(syntax["operation"], "syntaxTree");
        assert!(syntax.get("treeKind").is_none(), "{syntax}");
        // Line/column locate nodes; byte offsets are debug-only.
        let root_node = &syntax["nodes"][0];
        assert!(root_node.get("startLine").is_some(), "{root_node}");
        assert!(root_node.get("startByte").is_none(), "{root_node}");
        assert!(root_node.get("endByte").is_none(), "{root_node}");
        let debug = execute_row(
            json!({
                "operation":"syntaxTree","goal": "test", "reasoning":"test","debug":true,
                "path":source.to_string_lossy()
            }),
            &paths,
            &security,
            &Active,
        )
        .expect("debug syntax tree");
        assert_eq!(debug["nodes"][0]["startByte"], 0, "{debug}");
        assert!(debug["nodes"][0]["endByte"].as_u64().is_some(), "{debug}");

        for retired in [
            json!({"operation":"tree","goal": "test", "reasoning":"test","treeKind":"filesystem","path":root.path().to_string_lossy()}),
            json!({"operation":"tree","goal": "test", "reasoning":"test","treeKind":"syntax","path":source.to_string_lossy()}),
            json!({"operation":"files","goal": "test", "reasoning":"test","path":root.path().to_string_lossy()}),
            json!({"operation":"syntaxTree","goal": "test", "reasoning":"test","path":source.to_string_lossy(),"entryType":"f"}),
            json!({"operation":"syntaxTree","goal": "test", "reasoning":"test","path":source.to_string_lossy(),"sort":"size"}),
            json!({"operation":"topology","goal": "test", "reasoning":"test","analysis":"dependencies","path":root.path().to_string_lossy(),"file":"fixture.ts"}),
        ] {
            assert!(
                serde_json::from_value::<AstSearchQuery>(retired).is_err(),
                "retired astSearch surface must be rejected"
            );
        }
    }

    #[test]
    fn policy_and_native_failures_keep_specific_codes() {
        let policy = AstError::from(PolicyError::new(PolicyErrorCode::SymlinkEscape, "escape"));
        assert_eq!(policy.code, "ast.policy.symlinkEscape");
        assert_eq!(
            native_error("[structural.query.compileFailed] bad pattern").code,
            "structural.query.compileFailed"
        );
        assert_eq!(
            native_error("invalid regex: unclosed group").code,
            "ast.query.invalidPattern"
        );
    }
}
