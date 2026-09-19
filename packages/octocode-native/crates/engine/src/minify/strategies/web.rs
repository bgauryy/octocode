use crate::minify::comment_remover::remove_comments;
use crate::minify::strategies::code::minify_js_oxc;
use regex::Regex;
use std::sync::LazyLock;

// ── CSS ──────────────────────────────────────────────────────────────────────

/// Regex baseline — always available, fast.
fn minify_css_core(content: &str) -> String {
    let s = remove_comments(content, &["c-style"]);
    let rules = crate::minify::comment_remover::rules_for("c-style");
    let s = super::collapse_whitespace(&s, rules.as_ref());
    let s = super::re_tighten_punct(&s, rules.as_ref());
    s.trim().to_owned()
}

/// Lightweight best-effort CSS minification. This intentionally avoids a CSS
/// parser dependency; malformed or non-shrinking content is handled by the
/// caller's safe-original fallback.
pub fn minify_css_quality(content: &str) -> String {
    minify_css_core(content)
}

// ── HTML ─────────────────────────────────────────────────────────────────────

static TAG_GAP_WHITESPACE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r">[ \t\r\n]+<").expect("tag gap regex must compile"));

/// Regex baseline — always available.
pub fn minify_html_core(content: &str) -> String {
    let s = remove_comments(content, &["html"]);
    let rules = crate::minify::comment_remover::rules_for("html");
    let s = super::collapse_whitespace(&s, rules.as_ref());
    // `collapse_whitespace` now preserves a `\n` where a whitespace run
    // contained one (see its doc comment), so a single literal-space replace
    // no longer catches every inter-tag gap — tighten any whitespace run
    // between tags, not just a single space.
    let s = TAG_GAP_WHITESPACE.replace_all(&s, "><");
    s.trim().to_owned()
}

/// Style-aware HTML cleanup without a heavyweight HTML minifier dependency.
///
/// Full HTML minification is deceptively semantic (inline whitespace, raw-text
/// elements, optional tags, entity handling). For agent context we only need the
/// low-risk wins: remove HTML comments, collapse ordinary whitespace, and reuse
/// the existing CSS minifier inside `<style>` blocks.
pub fn minify_html_quality(content: &str) -> String {
    std::panic::catch_unwind(|| {
        let with_minified_styles = minify_style_blocks(content);
        minify_html_core(&with_minified_styles)
    })
    .unwrap_or_else(|_| minify_html_core(content))
}

// ── Embedded-language content view (HTML / Vue / Svelte) ───────────────────────

static STYLE_BLOCK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)(<style\b[^>]*>)(.*?)(</style>)").expect("style block regex must compile")
});
static WEB_BLOCK_OR_COMMENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?is)(<!--.*?-->)|(<script\b[^>]*>)(.*?)(</script\s*>)|(<style\b[^>]*>)(.*?)(</style\s*>)",
    )
    .expect("embedded web block regex must compile")
});
static ATTR_TYPE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\btype\s*=\s*["']([^"']*)["']"#).expect("type attr regex must compile")
});
static ATTR_LANG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\blang\s*=\s*["']([^"']*)["']"#).expect("lang attr regex must compile")
});

/// Readable embedded-language content view for HTML, Vue, and Svelte.
///
/// The markup structure (and its line breaks) is preserved so the output stays
/// readable for an agent. The real byte savings come from minifying the
/// embedded `<style>` blocks (lightweight CSS cleanup) and `<script>` blocks
/// (OXC, no mangle — the same treatment standalone JS/TS gets) and from
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
                minify_js_oxc(source, &script_virtual_path(open.as_str()), false)
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

fn minify_style_blocks(content: &str) -> String {
    STYLE_BLOCK
        .replace_all(content, |caps: &regex::Captures| {
            let (open, inner, close) = (&caps[1], &caps[2], &caps[3]);
            if inner.trim().is_empty() {
                return format!("{open}{inner}{close}");
            }
            format!("{open}{}{close}", minify_css_quality(inner).trim())
        })
        .into_owned()
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

/// Pick a virtual file path so oxc selects the right parser for an embedded
/// script, honoring `lang="ts"`/`type="..."` (Vue/Svelte SFCs commonly do this).
fn script_virtual_path(open_tag: &str) -> String {
    let hint = ATTR_LANG
        .captures(open_tag)
        .or_else(|| ATTR_TYPE.captures(open_tag))
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_ascii_lowercase())
        .unwrap_or_default();
    if hint.contains("tsx") {
        "embedded.tsx".to_owned()
    } else if hint.contains("ts") || hint.contains("typescript") {
        "embedded.ts".to_owned()
    } else {
        "embedded.js".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::{minify_embedded_web, minify_html_core, minify_html_quality};

    #[test]
    fn embedded_scanner_does_not_strip_comment_syntax_inside_scripts() {
        let source = "<script>const marker = '<!-- keep -->';</script><!-- drop -->";
        let output = minify_embedded_web(source, "fixture.html");
        assert!(output.contains("<!-- keep -->"));
        assert!(!output.contains("drop"));
    }

    #[test]
    fn html_core_tightens_newline_separated_tags() {
        // Regression: collapse_whitespace preserves a `\n` where a run
        // contained one, so the tag-gap tightener must handle `>\n<`, not
        // just a literal `> <`.
        let out = minify_html_core("<div>\n  <span>hi</span>\n</div>");
        assert_eq!(out, "<div><span>hi</span></div>");
    }

    #[test]
    fn html_quality_strips_comments_and_minifies_style_blocks() {
        let src = r#"
            <html>
              <head>
                <!-- comment -->
                <style>
                  .btn {
                    color: red;
                    margin: 0px 0px;
                  }
                </style>
              </head>
              <body><h1>Hi</h1></body>
            </html>
        "#;

        let out = minify_html_quality(src);

        assert!(!out.contains("comment"));
        assert!(out.contains("color:red"));
        assert!(out.len() < src.len());
    }
}
