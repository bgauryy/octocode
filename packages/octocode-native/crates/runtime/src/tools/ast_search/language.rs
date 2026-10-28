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
    if registered(path).is_some() {
        return None;
    }
    crate::tools::ast_rule::primary_extension(language).or_else(|| {
        octocode_engine::signatures::languages::RegistryView::installed()
            .select_one(language)
            .and_then(|entry| {
                entry
                    .extensions()
                    .first()
                    .map(|extension| (*extension).to_owned())
            })
    })
}

pub(super) fn registered(
    path: &std::path::Path,
) -> Option<octocode_engine::signatures::languages::RegistryEntry<'static>> {
    let extension = octocode_engine::text::extension_of(&path.to_string_lossy(), true, "");
    octocode_engine::signatures::languages::RegistryView::installed().find(&extension)
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
    operation: octocode_engine::text::analysis::AnalysisOperation,
) -> Result<(), ToolError> {
    validate_registered_language(
        path,
        selector,
        operation,
        octocode_engine::signatures::languages::RegistryView::installed(),
    )
}

fn validate_registered_language(
    path: &std::path::Path,
    selector: Option<&str>,
    operation: octocode_engine::text::analysis::AnalysisOperation,
    registry: octocode_engine::signatures::languages::RegistryView<'_>,
) -> Result<(), ToolError> {
    let registered = || {
        registry.find(&octocode_engine::text::extension_of(
            &path.to_string_lossy(),
            true,
            "",
        ))
    };
    let operation_name = match operation {
        octocode_engine::text::analysis::AnalysisOperation::StructuralSearch => "structural search",
        octocode_engine::text::analysis::AnalysisOperation::SyntaxTree => "syntax trees",
        octocode_engine::text::analysis::AnalysisOperation::SignatureOutline => {
            "signature outlines"
        }
        octocode_engine::text::analysis::AnalysisOperation::Declarations => "declarations",
        octocode_engine::text::analysis::AnalysisOperation::GraphFacts => "graph facts",
    };
    if let Some(language) = selector {
        if language.trim().is_empty() || matches!(language, ".") {
            return Err(ToolError::new(
                ErrorCode::LanguageUnsupported,
                "language must name a registered grammar.",
            ));
        }
        let provider = registry.select_one(language);
        if !registry
            .select(language)
            .any(|entry| entry.admits(operation))
            && !provider.is_some_and(|entry| entry.admits(operation))
        {
            return Err(ToolError::new(
                if provider.is_some() {
                    ErrorCode::CapabilityUnavailable
                } else {
                    ErrorCode::LanguageUnsupported
                },
                format!("language \"{language}\" has no registered provider for {operation_name}."),
            ));
        }
        // Only an extension of another grammar contradicts `parser`; one
        // that selects no grammar is parsed as `parser`, and any spelling
        // of the file's own grammar (`.cc` for a `.hpp`) reads it.
        if !octocode_engine::signatures::languages::parses_path(language, &path.to_string_lossy())
            && let Some(entry) = registered()
            && !provider.is_some_and(|selected| selected.extensions() == entry.extensions())
        {
            let grammar = match entry {
                octocode_engine::signatures::languages::RegistryEntry::Grammar(entry) => {
                    entry.name.to_ascii_lowercase()
                }
                octocode_engine::signatures::languages::RegistryEntry::Facts(entry) => {
                    entry.name.to_owned()
                }
            };
            return Err(ToolError::new(
                ErrorCode::LanguageMismatch,
                format!(
                    "{} has the {grammar} extension {}, not {language}; omit parser or set it to {grammar}.",
                    display_name(path),
                    extension_label(path)
                ),
            ));
        }
    } else if registered().is_some_and(|entry| !entry.admits(operation)) {
        return Err(ToolError::new(
            ErrorCode::CapabilityUnavailable,
            format!(
                "No registered {operation_name} provider supports {}.",
                display_name(path)
            ),
        ));
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

#[cfg(test)]
mod capability_tests {
    use super::*;
    use octocode_engine::signatures::languages::{FactProvider, RegistryView};
    use octocode_engine::text::analysis::AnalysisOperation;
    #[test]
    fn a_known_provider_without_the_requested_operation_is_distinct_from_an_unknown_language() {
        let providers = [FactProvider {
            id: "heldout-declarations",
            version: "1",
            name: "declfixture",
            extensions: &["declfixture"],
            capabilities: octocode_engine::types::GrammarFactCapabilities {
                declarations: true,
                calls: false,
                imports: false,
                exports: false,
                heritage: false,
            },
            extract: |_, _, _| Ok(None),
        }];
        let registry = RegistryView::new(&[], &providers).expect("registry");
        let path = std::path::Path::new("example.declfixture");
        for selector in [None, Some("declfixture")] {
            assert!(
                validate_registered_language(
                    path,
                    selector,
                    AnalysisOperation::Declarations,
                    registry
                )
                .is_ok()
            );
            for operation in [
                AnalysisOperation::SyntaxTree,
                AnalysisOperation::StructuralSearch,
                AnalysisOperation::SignatureOutline,
            ] {
                assert_eq!(
                    validate_registered_language(path, selector, operation, registry)
                        .expect_err("unsupported operation")
                        .code,
                    ErrorCode::CapabilityUnavailable
                );
            }
        }
        assert_eq!(
            validate_registered_language(
                path,
                Some("missing"),
                AnalysisOperation::Declarations,
                registry
            )
            .expect_err("unknown provider")
            .code,
            ErrorCode::LanguageUnsupported
        );
    }
}
