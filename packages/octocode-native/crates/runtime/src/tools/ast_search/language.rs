//! The grammar a single file parses under: the requested language checked
//! against the file's extension, shared extensions (`.h` under C++), and the
//! note that names a C++ header parsed as C.

use octocode_contract_types::ErrorCode;

use crate::response::result::ToolError;
use crate::tools::display_name;

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
                ErrorCode::LanguageUnsupported,
                "language must name a registered grammar.",
            ));
        }
        if octocode_engine::signatures::languages::family_extensions(language).is_none() {
            return Err(ToolError::new(
                ErrorCode::LanguageUnsupported,
                format!("language \"{language}\" is not a supported structural grammar."),
            ));
        }
        // Only an extension of another grammar contradicts `parser`; one
        // that selects no grammar is parsed as `parser`, and any spelling
        // of the file's own grammar (`.cc` for a `.hpp`) reads it.
        if !octocode_engine::signatures::languages::parses_path(language, &path.to_string_lossy())
            && let Some((grammar, _)) = crate::tools::ast_rule::file_grammar(path)
        {
            return Err(ToolError::new(
                ErrorCode::LanguageMismatch,
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

/// A `.h` parsed as C (no `parser`) whose source is C++: the C parse has
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
    header_parse_note(path, source)
}

fn header_parse_note(path: &std::path::Path, source: &str) -> Option<String> {
    let errors = |language| octocode_engine::structural::count_syntax_errors(source, language);
    let (c, cpp) = (errors("c")?, errors("cpp")?);
    let repair = "set parser:\"cpp\" (parserGlobs for a directory).";
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
        Some(&cpp_keyword_names_rule()),
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

/// The rule for C++ keywords the C grammar parses as type or variable names.
fn cpp_keyword_names_rule() -> serde_json::Value {
    serde_json::json!({
        "any": [{"kind": "type_identifier"}, {"kind": "identifier"}],
        "regex": "^(class|namespace|template|typename|virtual|public|private|protected|operator|using)$"
    })
}
