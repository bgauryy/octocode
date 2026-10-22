#[cfg(test)]
mod fidelity_tests;
#[cfg(test)]
mod file_language_tests;
#[cfg(test)]
mod lead_tests;
#[cfg(test)]
mod match_tests;
mod matches;
mod memo;
#[cfg(test)]
mod outline_tests;
mod output;
#[cfg(test)]
mod paging_tests;
#[cfg(test)]
mod policy_tests;
mod region;
#[cfg(test)]
mod region_tests;
#[cfg(test)]
mod rule_tests;
mod scan_window;
mod symbols;
#[cfg(test)]
mod symbols_tests;
mod syntax;
#[cfg(test)]
mod test_rows;
mod unwalked;
#[cfg(test)]
mod unwalked_tests;

use crate::response::result::Confidence;
use serde_json::Value;

use matches::MatchQuery;
pub use octocode_contract_types::AstSearchQuery;

use crate::policy::PolicyError;
#[cfg(test)]
use crate::policy::PolicyErrorCode;
use crate::response::result::ToolError;

/// A path-policy refusal. Path-shape refusals without a shared code (empty,
/// looping, over-long, not a regular file, binary, unreadable) are one code.
pub(super) fn policy_error(error: PolicyError) -> ToolError {
    ToolError::policy(error, "pathValidationFailed")
}

/// The source astSearch parses, read bounded at [`MAX_PARSE_SOURCE_BYTES`];
/// `None` when the file is larger.
pub(super) fn read_parse_source(path: &std::path::Path) -> Result<Option<Vec<u8>>, ToolError> {
    use crate::tools::source::{ReadError, read_bounded};
    match read_bounded(path, MAX_PARSE_SOURCE_BYTES) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(ReadError::TooLarge(_)) => Ok(None),
        Err(ReadError::NotRegular) => Err(io_error(
            &display_name(path),
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "not a regular file"),
        )),
        Err(ReadError::Io(error)) => Err(io_error(&display_name(path), error)),
    }
}

/// The terminal row of a source larger than [`MAX_PARSE_SOURCE_BYTES`]; a
/// searched literal (a symbol name, a pattern without metavariables) leads
/// to a text search of it in the same file.
pub(super) fn source_limit(path: &str, literal: Option<&str>) -> Value {
    let mut row = serde_json::json!({"status":"error","path":path,"errorCode":"fileTooLarge","error":"Source exceeds the native parser byte limit."});
    crate::response::paging::terminal_limit(&mut row, "fileTooLarge", &[]);
    if let Some(lead) = literal.and_then(|literal| text_search(path, literal)) {
        row["next"] = serde_json::json!({ "findText": lead });
    }
    row
}

/// A localSearch of `literal` in `path`: the reach a parse limit leaves.
pub(super) fn text_search(path: &str, literal: &str) -> Option<Value> {
    let literal = literal.trim();
    if literal.is_empty() || literal.contains('\n') {
        return None;
    }
    let mut row = serde_json::json!({"path": path, "matchString": literal});
    if regex::escape(literal) != literal {
        row["regex"] = serde_json::json!("literal");
    }
    Some(
        crate::response::result::Continuation::new(crate::id::ToolId::LocalSearch, row)
            .confidence(Confidence::Medium)
            .build(),
    )
}

/// A pattern's text when it names no metavariable: the literal a text
/// search can still find.
pub(super) fn pattern_literal(pattern: &str) -> Option<&str> {
    (!pattern.contains('$')).then_some(pattern)
}

/// An I/O failure on `path` (as the caller wrote it); a missing path says
/// so in the path policy's words.
pub(super) fn io_error(path: &str, error: std::io::Error) -> ToolError {
    match error.kind() {
        std::io::ErrorKind::NotFound => path_not_found(path),
        std::io::ErrorKind::PermissionDenied => {
            ToolError::new("permissionDenied", error.to_string())
        }
        std::io::ErrorKind::InvalidInput => ToolError::new("invalidInput", error.to_string()),
        _ => ToolError::new("fileAccessFailed", error.to_string()),
    }
}

/// `pathNotFound` with the path policy's message.
fn path_not_found(path: &str) -> ToolError {
    ToolError::new(
        crate::policy::PATH_NOT_FOUND,
        format!("Path does not exist: {path}"),
    )
}

/// An engine error as a tool error: the engine's typed kind names its code
/// (never the message text).
pub(super) fn native_error(error: octocode_engine::error::Error) -> ToolError {
    use octocode_engine::error::ErrorKind;
    let message = error.to_string();
    let code = match error.kind() {
        ErrorKind::NotFound => {
            return ToolError::new(
                crate::policy::PATH_NOT_FOUND,
                format!("Path does not exist ({message})"),
            );
        }
        ErrorKind::PermissionDenied => "permissionDenied",
        ErrorKind::InvalidPattern => crate::tools::ast_rule::INVALID_PATTERN,
        ErrorKind::InvalidGlob => "invalidGlob",
        ErrorKind::InvalidInput => "invalidInput",
        ErrorKind::Timeout => "timeout",
        ErrorKind::Cancelled => "cancelled",
        _ => "executionFailed",
    };
    ToolError::new(code, message)
}

/// The declared `errorCode` of an engine diagnostic code (`structural.…` or
/// an already-declared code): the engine's diagnostic names are its own, the
/// public set is the contract's.
pub(super) fn engine_error_code(code: &str) -> &'static str {
    match code {
        octocode_engine::structural::COMPILE_FAILED
        | octocode_engine::structural::QUERY_INVALID
        | octocode_engine::structural::RULE_INVALID => crate::tools::ast_rule::INVALID_PATTERN,
        "structural.language.unsupported" | "syntaxTree.language.unsupported" => {
            "languageUnsupported"
        }
        "structural.content.tooLarge" | "structural.file.tooLarge" => "fileTooLarge",
        "syntaxTree.options.lineOutOfRange" => "invalidInput",
        "structural.file.unreadable" => "fileReadFailed",
        octocode_engine::structural::PARSE_INTERRUPTED
        | octocode_engine::structural::MATCH_DEADLINE => "timeout",
        declared => crate::id::error_codes::ALL
            .iter()
            .find(|(code, _)| *code == declared)
            .map_or("executionFailed", |(code, _)| code),
    }
}

pub(super) use crate::tools::display_name;

/// The parser of a file whose extension the selected family shares with
/// another grammar (`.h` is C's, and C++'s family parses it too), as the
/// grammar registry decides; `None` when the file's own extension decides.
pub(super) fn shared_extension_parser(
    path: &std::path::Path,
    selector: Option<&str>,
) -> Option<String> {
    let selector = selector?;
    let path = path.to_string_lossy();
    let extension = octocode_engine::text::extension_of(&path, true, "");
    let parser = octocode_engine::signatures::languages::parser_for_path(selector, &path);
    (octocode_engine::signatures::languages::parses_path(selector, &path) && parser != extension)
        .then_some(parser)
}

/// The parser extension of a single file whose own extension does not pick
/// the requested grammar: a shared extension parsed by the selected family
/// (`.h` under C++), and the grammar's own extension for a file whose
/// extension selects no grammar (an extensionless script, a `.txt`).
/// `None`: the file's extension decides.
pub(super) fn parser_override(path: &std::path::Path, selector: Option<&str>) -> Option<String> {
    if let Some(parser) = shared_extension_parser(path, selector) {
        return Some(parser);
    }
    let language = selector?;
    if crate::tools::ast_rule::file_grammar(path).is_some() {
        return None;
    }
    crate::tools::ast_rule::primary_extension(language)
}

/// A file's extension as a message names it: `.ts`, or `no extension`.
pub(super) fn extension_label(path: &std::path::Path) -> String {
    match octocode_engine::text::extension_of(&path.to_string_lossy(), true, "").as_str() {
        "" => "no extension".to_owned(),
        extension => format!(".{extension}"),
    }
}

pub(super) fn validate_file_language(
    path: &std::path::Path,
    selector: Option<&str>,
) -> Result<(), ToolError> {
    if let Some(language) = selector {
        if language.trim().is_empty() || matches!(language, ".") {
            return Err(ToolError::new(
                "languageUnsupported",
                "language must name a registered grammar.",
            ));
        }
        if crate::tools::ast_rule::language_extensions(language).is_none() {
            return Err(ToolError::new(
                "languageUnsupported",
                format!("language \"{language}\" is not a supported structural grammar."),
            ));
        }
        // Only an extension of another grammar contradicts `language`; one
        // that selects no grammar is parsed as `language`, and any spelling
        // of the file's own grammar (`.cc` for a `.hpp`) reads it.
        if !octocode_engine::signatures::languages::parses_path(language, &path.to_string_lossy())
            && let Some((grammar, _)) = crate::tools::ast_rule::file_grammar(path)
        {
            return Err(ToolError::new(
                "languageMismatch",
                format!(
                    "{} has the {grammar} extension {}, not {language}; omit language or set it to {grammar}.",
                    display_name(path),
                    extension_label(path)
                ),
            ));
        }
    }
    Ok(())
}

/// Diagnostic code of [`cpp_header_note`].
pub(super) const CPP_HEADER_NOTE: &str = "structural.header.cpp";

/// A `.h` parsed as C (no `language`) whose source is C++: the C parse has
/// syntax errors the C++ one has not, or reads a C++ keyword (`class`,
/// `namespace`, …) as a name, so its declarations are misread. The note
/// suggests the C++ parser.
pub(super) fn cpp_header_note(
    path: &std::path::Path,
    selector: Option<&str>,
    source: &str,
) -> Option<String> {
    if selector.is_some()
        || octocode_engine::text::extension_of(&path.to_string_lossy(), true, "") != "h"
    {
        return None;
    }
    // An optional note: a parser panic leaves the row's matches intact.
    crate::tools::leads::best_effort(|| header_parse_note(path, source)).flatten()
}

fn header_parse_note(path: &std::path::Path, source: &str) -> Option<String> {
    let errors = |language| octocode_engine::structural::count_syntax_errors(source, language);
    let (c, cpp) = (errors("c")?, errors("cpp")?);
    let repair = "set language:\"cpp\" (languageGlobs for a directory).";
    if c > cpp {
        return Some(format!(
            "{} parses as C with {c} syntax error(s) and as C++ with {cpp}; {repair}",
            display_name(path)
        ));
    }
    if cpp > 0 {
        return None;
    }
    let keyword = octocode_engine::structural::search_detailed(
        source,
        &path.to_string_lossy(),
        Some("c"),
        None,
        Some(CPP_KEYWORD_NAMES),
    )
    .matches
    .into_iter()
    .next()?
    .text;
    Some(format!(
        "{} spells C++ `{keyword}`, which the C parser reads as a name; {repair}",
        display_name(path)
    ))
}

/// C++ keywords the C grammar parses as type or variable names.
const CPP_KEYWORD_NAMES: &str = r#"{"any":[{"kind":"type_identifier"},{"kind":"identifier"}],"regex":"^(class|namespace|template|typename|virtual|public|private|protected|operator|using)$"}"#;

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

pub type AstResult = Result<Value, ToolError>;

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
    cancellation: &dyn crate::execution::cancel::CancellationCheck,
    response_window: Option<usize>,
) -> AstResult {
    match query {
        AstSearchQuery::Symbols(query) => {
            execute_symbols(query, paths, security, cancellation, response_window)
        }
        AstSearchQuery::MatchPattern(query) => execute_match(
            MatchQuery::Pattern(query),
            paths,
            security,
            cancellation,
            response_window,
        ),
        AstSearchQuery::MatchRule(query) => execute_match(
            MatchQuery::Rule(query),
            paths,
            security,
            cancellation,
            response_window,
        ),
        AstSearchQuery::SyntaxTree(query) => {
            execute_syntax(query, paths, security, cancellation, response_window)
        }
    }
}

pub use matches::execute_match;
pub(crate) use output::Output;
pub use symbols::execute_symbols;
pub use syntax::execute_syntax;

/// A dot-named entry (`.git`, `.env`): out of view of a walk without
/// `hidden`.
pub(super) fn is_dot_entry(path: &std::path::Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with('.'))
}

#[cfg(test)]
mod tests {

    use serde_json::json;

    use super::*;
    use crate::{
        execution::cancel::CancellationCheck,
        policy::path::{PathPolicy, PathPolicyConfig},
        security::ContentSecurity,
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
        execute_ast(&query, paths, security, cancellation, None)
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
        assert_eq!(cancelled.code, "cancelled");
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
            &crate::execution::cancel::NeverCancel,
        )
        .expect("syntax tree");
        assert_eq!(syntax["operation"], "syntaxTree");
        assert!(syntax.get("treeKind").is_none(), "{syntax}");
        // One compact row per node: id, kind (an anonymous token quoted),
        // 1-based line:column span, and `^parent`. Byte offsets are
        // verbose (the verbose stage drops `returnedBytes` unless `debug`).
        let nodes = syntax["nodes"].as_array().expect("node rows");
        assert_eq!(nodes[0], "0 program 1:1-2:1", "{syntax}");
        assert_eq!(nodes[1], "1 export_statement 1:1-1:24 ^0", "{syntax}");
        assert!(syntax.get("nextOffset").is_none(), "{syntax}");
        let tokens = execute_row(
            json!({
                "operation":"syntaxTree","mainGoal": "test", "reasoning":"test",
                "path":source.to_string_lossy(),"namedOnly":false
            }),
            &paths,
            &security,
            &crate::execution::cancel::NeverCancel,
        )
        .expect("token tree");
        assert_eq!(tokens["nodes"][2], "2 \"export\" 1:1-1:7 ^1", "{tokens}");
        assert_eq!(syntax["returnedBytes"][0], "0-24", "{syntax}");
        assert_eq!(
            syntax["returnedBytes"].as_array().map(Vec::len),
            Some(nodes.len()),
            "{syntax}"
        );
        assert!(
            crate::response::extras::all_paths(crate::id::ToolId::AstSearch)
                .contains(&"results[].data.returnedBytes"),
            "returnedBytes is verbose"
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
        let policy = policy_error(PolicyError::new(PolicyErrorCode::SymlinkEscape, "escape"));
        assert_eq!(policy.code, "symlinkEscape");
        let missing = policy_error(PolicyError::new(PolicyErrorCode::NotFound, "missing"));
        assert_eq!(missing.code, "pathNotFound");
        let gone = io_error(
            "src/gone.rs",
            std::io::Error::from(std::io::ErrorKind::NotFound),
        );
        assert_eq!(gone.code, "pathNotFound");
        assert_eq!(gone.message, "Path does not exist: src/gone.rs");
        use octocode_engine::error::{Error, ErrorKind};
        let engine = native_error(Error::with_kind(
            ErrorKind::NotFound,
            "Cannot access structural search path '/w/x'",
        ));
        assert_eq!(engine.code, "pathNotFound");
        assert_eq!(
            engine.message,
            "Path does not exist (Cannot access structural search path '/w/x')"
        );
        let denied = policy_error(PolicyError::new(PolicyErrorCode::PermissionDenied, "no"));
        assert_eq!(denied.code, "permissionDenied");
        // The kind decides, never the words of the message (a bracketed
        // code in the text included).
        assert_eq!(
            native_error(Error::new("[structural.query.compileFailed] bad pattern")).code,
            "executionFailed"
        );
        assert_eq!(
            native_error(Error::with_kind(ErrorKind::InvalidPattern, "bad")).code,
            "invalidPattern"
        );
        assert_eq!(
            native_error(Error::with_kind(ErrorKind::PermissionDenied, "no")).code,
            "permissionDenied"
        );
        assert_eq!(
            native_error(Error::with_kind(ErrorKind::InvalidGlob, "bad")).code,
            "invalidGlob"
        );
        assert_eq!(native_error(Error::cancelled()).code, "cancelled");
        assert_eq!(
            native_error(Error::new("invalid regex: unclosed group")).code,
            "executionFailed"
        );
    }
}
