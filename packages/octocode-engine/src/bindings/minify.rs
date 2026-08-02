use crate::bindings::comment_groups::parse_comment_groups;
use crate::bindings::tasks::MinifyContentTask;
use crate::types::{FileTypeMinifyConfig, MinifyResult};
use napi::bindgen_prelude::AsyncTask;
use napi_derive::napi;

/// Full minification, synchronous. Content above the 1MB guard is returned
/// unchanged; unknown file types use the general strategy.
#[napi(js_name = "minifyContentSync")]
pub fn minify_content_sync(content: String, file_path: String) -> String {
    crate::minifier::minify_content_sync_inner(&content, &file_path)
}

/// Synchronous full minification result.
#[napi(js_name = "minifyContentResult")]
pub fn minify_content_result(content: String, file_path: String) -> MinifyResult {
    crate::minifier::minify_content_result_inner(&content, &file_path)
}

/// Full minification on libuv's worker pool.
/// Returns a Promise from JavaScript and does not block the event loop.
#[napi(js_name = "minifyContent")]
pub fn minify_content(content: String, file_path: String) -> AsyncTask<MinifyContentTask> {
    AsyncTask::new(MinifyContentTask { content, file_path })
}

/// Full minification that never grows the content — returns the minified
/// form only when it is shorter, otherwise the original. Panic-contained.
#[napi(js_name = "applyMinification")]
pub fn apply_minification(content: String, file_path: String) -> String {
    // See `signatures::run_on_deep_stack`: the JS/TS strategy here parses with
    // oxc, and napi's calling thread has less native stack headroom than a
    // test thread — pathologically nested input can crash the whole process
    // on this synchronous, main-thread-blocking entry point.
    crate::signatures::run_on_deep_stack(move || {
        crate::apply::apply_minification_inner(&content, &file_path)
    })
}

/// Agent-readable "standard" view: strips comments and blank-line noise while
/// preserving indentation and code shape. Capped at 1MB; panic-contained.
#[napi(js_name = "applyContentViewMinification")]
pub fn apply_content_view_minification(content: String, file_path: String) -> String {
    crate::signatures::run_on_deep_stack(move || {
        crate::apply::apply_content_view_minification_inner(&content, &file_path)
    })
}

/// `commentTypes` accepts a single string or array of strings.
#[napi(js_name = "removeComments")]
pub fn remove_comments(content: String, comment_types: serde_json::Value) -> String {
    let groups = parse_comment_groups(&Some(comment_types));
    remove_comments_for_groups(&content, &groups)
}

/// Conservative strategy: strip the configured comment groups, collapse blank
/// runs, preserve indentation.
#[napi(js_name = "minifyConservativeCore")]
pub fn minify_conservative_core(content: String, config: FileTypeMinifyConfig) -> String {
    with_comment_refs(&config, |comments| {
        crate::strategies::minify_conservative(&content, comments)
    })
}

/// Aggressive strategy: strip comments, collapse all whitespace, tighten
/// punctuation. Lossy — for token-budget views only.
#[napi(js_name = "minifyAggressiveCore")]
pub fn minify_aggressive_core(content: String, config: FileTypeMinifyConfig) -> String {
    with_comment_refs(&config, |comments| {
        crate::strategies::minify_aggressive(&content, comments)
    })
}

/// Compact JSON to a single line. JSONC/JSON5 noise (comments, trailing
/// commas) is stripped before parsing; unparseable input is returned trimmed.
#[napi(js_name = "minifyJsonCore")]
pub fn minify_json_core(content: String) -> MinifyResult {
    json_minify_result(crate::strategies::minify_json_core_inner(&content))
}

/// Readable JSON view: keeps formatting, strips JSONC noise and trailing
/// whitespace, collapses blank runs. Valid JSON passes through unchanged.
#[napi(js_name = "minifyJsonReadable")]
pub fn minify_json_readable(content: String) -> MinifyResult {
    json_minify_result(crate::strategies::minify_json_readable_inner(&content))
}

/// Whitespace-only code cleanup: trim line ends, collapse 3+ blank lines,
/// preserve indentation.
#[napi(js_name = "minifyCodeCore")]
pub fn minify_code_core(content: String) -> String {
    crate::strategies::minify_code_core(&content)
}

/// Generic text cleanup for unknown file types: trim + collapse blank runs.
#[napi(js_name = "minifyGeneralCore")]
pub fn minify_general_core(content: String) -> String {
    crate::strategies::minify_general_core(&content)
}

/// Markdown view: drops HTML comments, badges, and generated TOCs; compacts
/// tables and headings; preserves code fences and frontmatter verbatim.
#[napi(js_name = "minifyMarkdownCore")]
pub fn minify_markdown_core(content: String) -> String {
    crate::strategies::minify_markdown_core(&content)
}

/// Lightweight CSS cleanup (comment strip + whitespace). See
/// `minifyCSSQuality` for the lightningcss-backed variant.
#[napi(js_name = "minifyCSSCore")]
pub fn minify_css_core(content: String) -> String {
    crate::strategies::minify_css_core(&content)
}

/// Lightweight HTML/XML cleanup (comment strip + whitespace). See
/// `minifyHTMLQuality` for the style-aware built-in variant.
#[napi(js_name = "minifyHTMLCore")]
pub fn minify_html_core(content: String) -> String {
    crate::strategies::minify_html_core(&content)
}

/// Heuristic JS minifier (comment strip + whitespace tightening) used when
/// the OXC pipeline declines the input.
#[napi(js_name = "minifyJavaScriptCore")]
pub fn minify_javascript_core(content: String) -> String {
    crate::strategies::minify_javascript_core(&content)
}

/// CSS minification via lightningcss — parser-grade, strips comments and
/// redundant units.
#[napi(js_name = "minifyCSSQuality")]
pub fn minify_css_quality(content: String) -> String {
    crate::strategies::minify_css_quality(&content)
}

/// Style-aware HTML cleanup: strips comments, tightens whitespace, and minifies
/// embedded `<style>` blocks through the existing CSS pipeline.
#[napi(js_name = "minifyHTMLQuality")]
pub fn minify_html_quality(content: String) -> String {
    crate::strategies::minify_html_quality(&content)
}

/// Remove Python docstrings (module/class/function level) while preserving
/// all runtime code.
#[napi(js_name = "stripPythonDocstrings")]
pub fn strip_python_docstrings(content: String) -> String {
    crate::comment_remover::strip_python_docstrings(&content)
}

fn remove_comments_for_groups(content: &str, groups: &[String]) -> String {
    let refs: Vec<&str> = groups.iter().map(String::as_str).collect();
    crate::comment_remover::remove_comments(content, &refs)
}

fn with_comment_refs<R>(
    config: &FileTypeMinifyConfig,
    run: impl FnOnce(Option<&[&str]>) -> R,
) -> R {
    let groups = parse_comment_groups(&config.comments);
    let refs: Vec<&str> = groups.iter().map(String::as_str).collect();
    run((!refs.is_empty()).then_some(refs.as_slice()))
}

fn json_minify_result((content, failed): (String, bool)) -> MinifyResult {
    MinifyResult {
        content,
        failed,
        r#type: "json".to_owned(),
        reason: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn remove_comments_accepts_string_or_array_union() {
        let src = "int x; // c\nint y;";
        let from_string = remove_comments(src.into(), json!("c-style"));
        let from_array = remove_comments(src.into(), json!(["c-style"]));
        assert_eq!(from_string, from_array);
        assert!(!from_string.contains("// c"));
    }

    #[test]
    fn remove_comments_returns_content_unchanged_on_invalid_union_shape() {
        assert_eq!(remove_comments("hello".into(), json!(42)), "hello");
        assert_eq!(
            remove_comments("hello".into(), json!({"bad": true})),
            "hello"
        );
    }

    #[test]
    fn minify_json_core_wrapper_shapes_minify_result() {
        let r = minify_json_core("{\"a\": 1 }".into());
        assert!(!r.failed);
        assert_eq!(r.r#type, "json");
        assert!(r.reason.is_none());
    }
}
