pub(crate) mod declarations_cache;
mod matches;
mod output;
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
        if let Some(code) = error.shared_code() {
            return Self::new(code, error.message);
        }
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

/// The source astSearch parses, read bounded at [`MAX_PARSE_SOURCE_BYTES`];
/// `None` when the file is larger.
pub(super) fn read_parse_source(path: &std::path::Path) -> Result<Option<Vec<u8>>, AstError> {
    use crate::tools::source::{BoundedRead, read_bounded};
    match read_bounded(path, MAX_PARSE_SOURCE_BYTES) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(BoundedRead::TooLarge(_)) => Ok(None),
        Err(BoundedRead::NotRegular) => Err(io_error(
            &display_name(path),
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "not a regular file"),
        )),
        Err(BoundedRead::Io(error)) => Err(io_error(&display_name(path), error)),
    }
}

/// The terminal row of a source larger than [`MAX_PARSE_SOURCE_BYTES`].
pub(super) fn source_limit(path: &str) -> Value {
    serde_json::json!({"status":"error","path":path,"errorCode":"ast.source.limit","error":"Source exceeds the native parser byte limit.","isPartial":true,"terminalLimit":true})
}

/// An I/O failure on `path` (as the caller wrote it); a missing path says
/// so in the path policy's words.
pub(super) fn io_error(path: &str, error: std::io::Error) -> AstError {
    match error.kind() {
        std::io::ErrorKind::NotFound => path_not_found(path),
        std::io::ErrorKind::PermissionDenied => {
            AstError::new("permissionDenied", error.to_string())
        }
        std::io::ErrorKind::InvalidInput => AstError::new("invalidInput", error.to_string()),
        _ => AstError::new("ast.execution.io", error.to_string()),
    }
}

/// `pathNotFound` with the path policy's message.
fn path_not_found(path: &str) -> AstError {
    AstError::new(
        crate::policy::PATH_NOT_FOUND,
        format!("Path does not exist: {path}"),
    )
}

pub(super) fn native_error(error: impl ToString) -> AstError {
    let message = error.to_string();
    if let Some(rest) = message.strip_prefix('[')
        && let Some((code, _)) = rest.split_once(']')
        && (code.starts_with("structural.") || code.starts_with("ast."))
    {
        let code = match code {
            "structural.query.compileFailed" | "structural.query.invalid" => {
                crate::tools::ast_rule::INVALID_PATTERN
            }
            other => other,
        };
        return AstError::new(code.to_owned(), crate::tools::ast_rule::untagged(&message));
    }
    let lower = message.to_ascii_lowercase();
    if lower.contains("no such file") || lower.contains("not found") {
        // The engine quotes the path it could not access ('…').
        let quoted = message
            .split_once('\'')
            .and_then(|(_, rest)| rest.split_once('\''))
            .map(|(path, _)| path.to_owned());
        return match quoted {
            Some(path) => path_not_found(&path),
            None => AstError::new(
                crate::policy::PATH_NOT_FOUND,
                format!("Path does not exist ({message})"),
            ),
        };
    }
    let code = if lower.contains("permission denied") {
        "permissionDenied"
    } else if lower.contains("invalid") && lower.contains("regex") {
        "invalidPattern"
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

pub(super) use crate::tools::display_name;

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
                "language must name a registered grammar.",
            ));
        }
        let selected = crate::tools::ast_rule::language_extensions(language).ok_or_else(|| {
            AstError::new(
                "ast.language.unsupported",
                format!("language \"{language}\" is not a supported structural grammar."),
            )
        })?;
        if !crate::tools::ast_rule::has_extension_in(path, &selected)
            && !cpp_header_override(path, selector)
        {
            return Err(AstError::new(
                "ast.language.mismatch",
                format!(
                    "{} is not a {language} source file; omit language or choose the grammar matching its extension.",
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
pub(crate) use output::Output;
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
            json!({"operation":"symbols","mainGoal": "test", "reasoning":"test","path":".","unknown":true}),
        ] {
            assert!(serde_json::from_value::<AstSearchQuery>(row).is_err());
        }

        let cancelled = execute_row(
            json!({"operation":"symbols","mainGoal": "test", "reasoning":"test","path":"."}),
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
        let (paths, security) = crate::tools::test_support::simple_policy(root.path());

        let syntax = execute_row(
            json!({
                "operation":"syntaxTree","mainGoal": "test", "reasoning":"test",
                "path":source.to_string_lossy()
            }),
            &paths,
            &security,
            &crate::tools::cancel::NeverCancel,
        )
        .expect("syntax tree");
        assert_eq!(syntax["operation"], "syntaxTree");
        assert!(syntax.get("treeKind").is_none(), "{syntax}");
        // One compact row per node: id, kind (an anonymous token quoted),
        // 1-based line:0-based column span, and `^parent`. Byte offsets are
        // verbose (the verbose stage drops `nodeBytes` unless `debug`).
        let nodes = syntax["nodes"].as_array().expect("node rows");
        assert_eq!(nodes[0], "0 program 1:0-2:0", "{syntax}");
        assert_eq!(nodes[1], "1 export_statement 1:0-1:23 ^0", "{syntax}");
        assert!(syntax.get("nextOffset").is_none(), "{syntax}");
        let tokens = execute_row(
            json!({
                "operation":"syntaxTree","mainGoal": "test", "reasoning":"test",
                "path":source.to_string_lossy(),"namedOnly":false
            }),
            &paths,
            &security,
            &crate::tools::cancel::NeverCancel,
        )
        .expect("token tree");
        assert_eq!(tokens["nodes"][2], "2 \"export\" 1:0-1:6 ^1", "{tokens}");
        assert_eq!(syntax["nodeBytes"][0], "0-24", "{syntax}");
        assert_eq!(
            syntax["nodeBytes"].as_array().map(Vec::len),
            Some(nodes.len()),
            "{syntax}"
        );
        assert!(
            crate::tools::id::ToolId::AstSearch
                .verbose_paths()
                .contains(&"results[].data.nodeBytes"),
            "nodeBytes is verbose"
        );

        for retired in [
            json!({"operation":"tree","mainGoal": "test", "reasoning":"test","treeKind":"filesystem","path":root.path().to_string_lossy()}),
            json!({"operation":"tree","mainGoal": "test", "reasoning":"test","treeKind":"syntax","path":source.to_string_lossy()}),
            json!({"operation":"files","mainGoal": "test", "reasoning":"test","path":root.path().to_string_lossy()}),
            json!({"operation":"syntaxTree","mainGoal": "test", "reasoning":"test","path":source.to_string_lossy(),"entryType":"f"}),
            json!({"operation":"syntaxTree","mainGoal": "test", "reasoning":"test","path":source.to_string_lossy(),"sort":"size"}),
            json!({"operation":"topology","mainGoal": "test", "reasoning":"test","analysis":"dependencies","path":root.path().to_string_lossy(),"file":"fixture.ts"}),
        ] {
            assert!(
                serde_json::from_value::<AstSearchQuery>(retired).is_err(),
                "retired astSearch surface must be rejected"
            );
        }
    }

    #[test]
    fn policy_and_native_failures_keep_specific_codes() {
        // Path failures share the local tools' codes.
        let policy = AstError::from(PolicyError::new(PolicyErrorCode::SymlinkEscape, "escape"));
        assert_eq!(policy.code, "symlinkEscape");
        let missing = AstError::from(PolicyError::new(PolicyErrorCode::NotFound, "missing"));
        assert_eq!(missing.code, "pathNotFound");
        let gone = io_error(
            "src/gone.rs",
            std::io::Error::from(std::io::ErrorKind::NotFound),
        );
        assert_eq!(gone.code, "pathNotFound");
        assert_eq!(gone.message, "Path does not exist: src/gone.rs");
        let engine = native_error(
            "Cannot access structural search path '/w/x': No such file or directory (os error 2)",
        );
        assert_eq!(engine.code, "pathNotFound");
        assert_eq!(engine.message, "Path does not exist: /w/x");
        let denied = AstError::from(PolicyError::new(PolicyErrorCode::PermissionDenied, "no"));
        assert_eq!(denied.code, "permissionDenied");
        assert_eq!(
            native_error("[structural.query.compileFailed] bad pattern").code,
            "invalidPattern"
        );
        assert_eq!(
            native_error("invalid regex: unclosed group").code,
            "invalidPattern"
        );
    }
}
