use crate::minify::comment_remover::remove_comments;
use crate::minify::strategies::code::minify_javascript_core;
use regex::Regex;
use std::sync::LazyLock;

// ── CSS ──────────────────────────────────────────────────────────────────────

/// Lightweight best-effort CSS minification (regex baseline). This
/// intentionally avoids a CSS parser dependency; malformed or non-shrinking
/// content is handled by the caller's safe-original fallback.
pub fn minify_css_quality(content: &str) -> String {
    let s = remove_comments(content, &["c-style"]);
    let rules = crate::minify::comment_remover::rules_for("c-style");
    let s = super::collapse_whitespace(&s, rules);
    let s = super::re_tighten_punct(&s, rules);
    s.trim().to_owned()
}

// ── Embedded-language content view (HTML / Vue / Svelte) ───────────────────────

// Constant patterns: a compile failure is a build-time programming error, not a
// runtime condition — `expect` is the correct fail-loud contract here.
#[allow(clippy::expect_used)]
static WEB_BLOCK_OR_COMMENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?is)(<!--.*?-->)|(<script\b[^>]*>)(.*?)(</script\s*>)|(<style\b[^>]*>)(.*?)(</style\s*>)",
    )
    .expect("embedded web block regex must compile")
});
#[allow(clippy::expect_used)]
static ATTR_TYPE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\btype\s*=\s*["']([^"']*)["']"#).expect("type attr regex must compile")
});

/// Readable embedded-language content view for HTML, Vue, and Svelte.
///
/// The markup structure (and its line breaks) is preserved so the output stays
/// readable for an agent. The real byte savings come from minifying the
/// embedded `<style>` blocks (lightweight CSS cleanup) and `<script>` blocks
/// (comment-strip + whitespace tighten — the same treatment standalone JS/TS
/// gets, identifiers and line structure preserved) and from
/// dropping HTML comments outside raw script/style blocks. Markup whitespace is
/// preserved. This bounded non-recursive scanner does not claim parser-grade
/// HTML correctness; malformed or unclosed blocks are left unchanged.
pub fn minify_embedded_web(content: &str, _file_path: &str) -> String {
    let mut output = String::with_capacity(content.len());
    let mut offset = 0;
    for captures in WEB_BLOCK_OR_COMMENT.captures_iter(content) {
        let Some(whole) = captures.get(0) else {
            continue;
        };
        output.push_str(&content[offset..whole.start()]);
        if captures.get(1).is_some() {
            // Drop an HTML comment outside a raw block.
        } else if let (Some(open), Some(inner), Some(close)) =
            (captures.get(2), captures.get(3), captures.get(4))
        {
            output.push_str(open.as_str());
            let source = inner.as_str();
            let compacted = if script_is_javascript(open.as_str()) {
                Some(minify_javascript_core(source))
            } else {
                None
            };
            output.push_str(
                compacted
                    .as_deref()
                    .filter(|candidate| candidate.len() < source.len())
                    .unwrap_or(source),
            );
            output.push_str(close.as_str());
        } else if let (Some(open), Some(inner), Some(close)) =
            (captures.get(5), captures.get(6), captures.get(7))
        {
            output.push_str(open.as_str());
            let source = inner.as_str();
            let compacted = minify_css_quality(source);
            output.push_str(if compacted.len() < source.len() {
                &compacted
            } else {
                source
            });
            output.push_str(close.as_str());
        }
        offset = whole.end();
    }
    output.push_str(&content[offset..]);
    output
}

/// True when a `<script>` open tag denotes inline JavaScript/TypeScript that is
/// safe to run through oxc. External (`src=`) and non-JS payloads return false.
fn script_is_javascript(open_tag: &str) -> bool {
    let lower = open_tag.to_ascii_lowercase();
    if lower.contains("src=") || lower.contains("src =") {
        return false;
    }
    match ATTR_TYPE.captures(open_tag).and_then(|c| c.get(1)) {
        None => true,
        Some(m) => matches!(
            m.as_str().trim().to_ascii_lowercase().as_str(),
            "" | "text/javascript"
                | "application/javascript"
                | "module"
                | "text/babel"
                | "text/jsx"
                | "text/typescript"
                | "application/typescript"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::minify_embedded_web;

    #[test]
    fn embedded_scanner_does_not_strip_comment_syntax_inside_scripts() {
        let source = "<script>const marker = '<!-- keep -->';</script><!-- drop -->";
        let output = minify_embedded_web(source, "fixture.html");
        assert!(output.contains("<!-- keep -->"));
        assert!(!output.contains("drop"));
    }
}
