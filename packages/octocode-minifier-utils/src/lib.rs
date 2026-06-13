// napi-rs requires owned String/Value parameters at the FFI boundary, which
// trips needless_pass_by_value on every exported fn — allowed crate-wide.
#![allow(clippy::needless_pass_by_value)]

mod apply;
mod comment_remover;
mod config;
mod file_extension;
mod minifier;
mod signatures;
mod strategies;
mod types;
mod yaml_utils;

use napi_derive::napi;
use types::{FileTypeMinifyConfig, GetExtensionOptions, MinifyResult, YamlConversionConfig};

// ── Re-exports so the TS shim can see them ────────────────────────────────────
#[napi]
pub const SIGNATURES_ONLY_HINT: &str = signatures::SIGNATURES_ONLY_HINT;

// ── File extension ────────────────────────────────────────────────────────────
/// Extract the file extension from a path (dotfile-aware).
/// Options control lowercasing and the fallback used when no extension exists.
#[napi(js_name = "getExtension")]
pub fn get_extension(file_path: String, options: Option<GetExtensionOptions>) -> String {
    let lowercase = options.as_ref().and_then(|o| o.lowercase).unwrap_or(false);
    let fallback  = options.as_ref().and_then(|o| o.fallback.as_deref()).unwrap_or("");
    file_extension::get_extension_internal(&file_path, lowercase, fallback)
}

// ── Minification ──────────────────────────────────────────────────────────────
/// Full minification, synchronous. Content above the 1MB guard is returned
/// unchanged; unknown file types fall back to the general strategy.
#[napi(js_name = "minifyContentSync")]
pub fn minify_content_sync(content: String, file_path: String) -> String {
    minifier::minify_content_sync_inner(&content, &file_path)
}

/// Sync equivalent of TS `minifyContent` — returns MinifyResult.
/// The JS shim wraps this in Promise.resolve() so callers can await it.
#[napi(js_name = "minifyContentResult")]
pub fn minify_content_result(content: String, file_path: String) -> MinifyResult {
    minifier::minify_content_result_inner(&content, &file_path)
}

/// Full minification that never grows the content — returns the minified
/// form only when it is shorter, otherwise the original. Panic-contained.
#[napi(js_name = "applyMinification")]
pub fn apply_minification(content: String, file_path: String) -> String {
    apply::apply_minification_inner(&content, &file_path)
}

/// Agent-readable "standard" view: strips comments and blank-line noise while
/// preserving indentation and code shape. Capped at 1MB; panic-contained.
#[napi(js_name = "applyContentViewMinification")]
pub fn apply_content_view_minification(content: String, file_path: String) -> String {
    apply::apply_content_view_minification_inner(&content, &file_path)
}

// ── Fine-grained strategy exports ─────────────────────────────────────────────

/// `commentTypes` accepts a single string or array of strings.
#[napi(js_name = "removeComments")]
pub fn remove_comments(content: String, comment_types: serde_json::Value) -> String {
    let groups: Vec<String> = match comment_types {
        serde_json::Value::String(s) => vec![s],
        serde_json::Value::Array(arr) => arr
            .into_iter()
            .filter_map(|v| v.as_str().map(|s| s.to_owned()))
            .collect(),
        _ => return content,
    };
    let refs: Vec<&str> = groups.iter().map(|s| s.as_str()).collect();
    comment_remover::remove_comments(&content, &refs)
}

/// Conservative strategy: strip the configured comment groups, collapse blank
/// runs, preserve indentation.
#[napi(js_name = "minifyConservativeCore")]
pub fn minify_conservative_core(content: String, config: FileTypeMinifyConfig) -> String {
    let groups = parse_comment_groups(&config.comments);
    let refs: Vec<&str> = groups.iter().map(|s| s.as_str()).collect();
    strategies::minify_conservative(&content, if refs.is_empty() { None } else { Some(&refs) })
}

/// Aggressive strategy: strip comments, collapse all whitespace, tighten
/// punctuation. Lossy — for token-budget views only.
#[napi(js_name = "minifyAggressiveCore")]
pub fn minify_aggressive_core(content: String, config: FileTypeMinifyConfig) -> String {
    let groups = parse_comment_groups(&config.comments);
    let refs: Vec<&str> = groups.iter().map(|s| s.as_str()).collect();
    strategies::minify_aggressive(&content, if refs.is_empty() { None } else { Some(&refs) })
}

/// Compact JSON to a single line. JSONC/JSON5 noise (comments, trailing
/// commas) is stripped before parsing; unparseable input is returned trimmed.
#[napi(js_name = "minifyJsonCore")]
pub fn minify_json_core(content: String) -> MinifyResult {
    let (out, failed) = strategies::minify_json_core_inner(&content);
    MinifyResult { content: out, failed, r#type: "json".to_owned(), reason: None }
}

/// Readable JSON view: keeps formatting, strips JSONC noise and trailing
/// whitespace, collapses blank runs. Valid JSON passes through unchanged.
#[napi(js_name = "minifyJsonReadable")]
pub fn minify_json_readable(content: String) -> MinifyResult {
    let (out, failed) = strategies::minify_json_readable_inner(&content);
    MinifyResult { content: out, failed, r#type: "json".to_owned(), reason: None }
}

/// Whitespace-only code cleanup: trim line ends, collapse 3+ blank lines,
/// preserve indentation.
#[napi(js_name = "minifyCodeCore")]
pub fn minify_code_core(content: String) -> String {
    strategies::minify_code_core(&content)
}

/// Generic text cleanup for unknown file types: trim + collapse blank runs.
#[napi(js_name = "minifyGeneralCore")]
pub fn minify_general_core(content: String) -> String {
    strategies::minify_general_core(&content)
}

/// Markdown view: drops HTML comments, badges, and generated TOCs; compacts
/// tables and headings; preserves code fences and frontmatter verbatim.
#[napi(js_name = "minifyMarkdownCore")]
pub fn minify_markdown_core(content: String) -> String {
    strategies::minify_markdown_core(&content)
}

/// Lightweight CSS cleanup (comment strip + whitespace). See
/// `minifyCSSQuality` for the lightningcss-backed variant.
#[napi(js_name = "minifyCSSCore")]
pub fn minify_css_core(content: String) -> String {
    strategies::minify_css_core(&content)
}

/// Lightweight HTML/XML cleanup (comment strip + whitespace). See
/// `minifyHTMLQuality` for the minify-html-backed variant.
#[napi(js_name = "minifyHTMLCore")]
pub fn minify_html_core(content: String) -> String {
    strategies::minify_html_core(&content)
}

/// Heuristic JS fallback (comment strip + whitespace tightening) used when
/// the OXC pipeline declines the input.
#[napi(js_name = "minifyJavaScriptCore")]
pub fn minify_javascript_core(content: String) -> String {
    strategies::minify_javascript_core(&content)
}

/// CSS minification via lightningcss — parser-grade, strips comments and
/// redundant units.
#[napi(js_name = "minifyCSSQuality")]
pub fn minify_css_quality(content: String) -> String {
    strategies::minify_css_quality(&content)
}

/// HTML minification via minify-html — parser-grade comment and whitespace
/// removal.
#[napi(js_name = "minifyHTMLQuality")]
pub fn minify_html_quality(content: String) -> String {
    strategies::minify_html_quality(&content)
}

/// Remove Python docstrings (module/class/function level) while preserving
/// all runtime code.
#[napi(js_name = "stripPythonDocstrings")]
pub fn strip_python_docstrings(content: String) -> String {
    comment_remover::strip_python_docstrings(&content)
}

// ── Signature extraction ──────────────────────────────────────────────────────

/// Structural skeleton with an `NNN| ` line-number gutter: tree-sitter for
/// the top-10 languages, heuristics for the rest. Returns `null` for data,
/// config, and prose formats and for content above the 1MB guard.
#[napi(js_name = "extractSignatures")]
pub fn extract_signatures(content: String, file_path: String) -> Option<String> {
    signatures::extract_signatures_inner(&content, &file_path)
}

/// Returns all extensions that have signature extraction support
/// (tree-sitter languages + heuristic-covered languages).
#[napi(js_name = "getSupportedSignatureExtensions")]
pub fn get_supported_signature_extensions() -> Vec<String> {
    // Tree-sitter covered
    let mut exts: Vec<String> = signatures::languages::supported_extensions()
        .into_iter()
        .map(|s| s.to_owned())
        .collect();

    // Heuristic-covered extensions (matching heuristic.rs extract_heuristic routes)
    const HEURISTIC_ONLY: &[&str] = &[
        "kt", "kotlin", "scala",          // JVM family
        "rb",                               // Ruby
        "php",                              // PHP
        "swift",                            // Swift
        "css", "scss", "less",             // CSS family
        "html", "htm",                      // HTML
        "sql", "tsql", "plsql",            // SQL
        "vue", "svelte",                    // SFC components
        "ex", "exs",                        // Elixir
        "hs", "lhs",                        // Haskell
        "lua",                              // Lua
        "erl", "hrl",                       // Erlang
    ];
    for ext in HEURISTIC_ONLY {
        if !exts.iter().any(|e| e == ext) {
            exts.push(ext.to_string());
        }
    }
    exts.sort();
    exts
}

// ── YAML ──────────────────────────────────────────────────────────────────────

/// Serialize a JSON value to YAML — the formatter for every MCP tool
/// response. Optional key sorting and priority-key ordering; multiline
/// strings become block scalars. Emission is locked by yaml_utils tests.
#[napi(js_name = "jsonToYamlString")]
pub fn json_to_yaml_string(json_object: serde_json::Value, config: Option<YamlConversionConfig>) -> String {
    let sort_keys     = config.as_ref().and_then(|c| c.sort_keys).unwrap_or(false);
    let priority_keys = config
        .as_ref()
        .and_then(|c| c.keys_priority.as_deref())
        .map(|v| v.to_vec())
        .unwrap_or_default();
    yaml_utils::json_to_yaml_string_inner(json_object, sort_keys, &priority_keys)
}

// ── Config introspection (benchmark / tooling) ──────────────────────────────────

/// Returns the full MINIFY_CONFIG as a JS-compatible object.
/// Shape: `{ fileTypes: Record<string, { strategy: string, comments: string | string[] | null }> }`
#[napi(js_name = "getMINIFY_CONFIG")]
pub fn get_minify_config() -> serde_json::Value {
    let file_types: std::collections::HashMap<String, serde_json::Value> = config::minify_config()
        .iter()
        .map(|(ext, cfg)| {
            let comments: serde_json::Value = match cfg.comments {
                None => serde_json::Value::Null,
                Some(groups) if groups.len() == 1 =>
                    serde_json::Value::String(groups[0].to_string()),
                Some(groups) => serde_json::Value::Array(
                    groups.iter().map(|g| serde_json::Value::String((*g).to_string())).collect(),
                ),
            };
            (ext.to_string(), serde_json::json!({ "strategy": cfg.strategy, "comments": comments }))
        })
        .collect();
    serde_json::json!({ "fileTypes": file_types })
}


// ── Private helpers ───────────────────────────────────────────────────────────

fn parse_comment_groups(val: &Option<serde_json::Value>) -> Vec<String> {
    match val {
        None => vec![],
        Some(serde_json::Value::String(s)) => vec![s.clone()],
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_owned()))
            .collect(),
        _ => vec![],
    }
}

// ── Tests — FFI-boundary glue only ───────────────────────────────────────────
// Logic tests live next to the code they cover (strategies.rs, apply.rs,
// minifier.rs, signatures/mod.rs, comment_remover.rs, file_extension.rs,
// yaml_utils.rs). This module covers only the lib.rs argument-parsing glue.
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn get_extension_applies_option_defaults_when_options_omitted() {
        assert_eq!(get_extension("foo.ts".into(), None), "ts");
        assert_eq!(get_extension("Makefile".into(), None), "");
    }

    #[test]
    fn get_extension_honors_lowercase_and_fallback_options() {
        let opts = GetExtensionOptions { lowercase: Some(true), fallback: Some("txt".into()) };
        assert_eq!(get_extension("Foo.TS".into(), Some(opts)), "ts");
        let opts = GetExtensionOptions { lowercase: None, fallback: Some("txt".into()) };
        assert_eq!(get_extension("Makefile".into(), Some(opts)), "txt");
    }

    #[test]
    fn remove_comments_accepts_string_or_array_union() {
        let src = "int x; // c\nint y;";
        let from_string = remove_comments(src.into(), json!("c-style"));
        let from_array  = remove_comments(src.into(), json!(["c-style"]));
        assert_eq!(from_string, from_array);
        assert!(!from_string.contains("// c"));
    }

    #[test]
    fn remove_comments_returns_content_unchanged_on_invalid_union_shape() {
        assert_eq!(remove_comments("hello".into(), json!(42)), "hello");
        assert_eq!(remove_comments("hello".into(), json!({"bad": true})), "hello");
    }

    #[test]
    fn parse_comment_groups_handles_all_union_shapes() {
        assert!(parse_comment_groups(&None).is_empty());
        assert_eq!(parse_comment_groups(&Some(json!("hash"))), vec!["hash"]);
        assert_eq!(parse_comment_groups(&Some(json!(["a", "b"]))), vec!["a", "b"]);
        assert!(parse_comment_groups(&Some(json!(7))).is_empty());
    }

    #[test]
    fn minify_json_core_wrapper_shapes_minify_result() {
        let r = minify_json_core("{\"a\": 1 }".into());
        assert!(!r.failed);
        assert_eq!(r.r#type, "json");
        assert!(r.reason.is_none());
    }

    #[test]
    fn supported_signature_extensions_are_sorted_and_complete() {
        let exts = get_supported_signature_extensions();
        for required in ["ts", "py", "rs", "vue", "svelte"] {
            assert!(exts.iter().any(|e| e == required), "missing {required}");
        }
        let mut sorted = exts.clone();
        sorted.sort();
        assert_eq!(exts, sorted, "extension list must be sorted");
    }

    #[test]
    fn json_to_yaml_string_defaults_when_config_omitted() {
        let out = json_to_yaml_string(json!({"k": "v"}), None);
        assert_eq!(out, "k: v\n");
    }
}
