use crate::minify::comment_remover::remove_comments;
use crate::minify::strategies::code::minify_js_oxc;
use regex::Regex;
use std::sync::LazyLock;

// ── CSS ──────────────────────────────────────────────────────────────────────

/// Regex baseline — always available, fast.
pub fn minify_css_core(content: &str) -> String {
    let s = remove_comments(content, &["c-style"]);
    let rules = crate::minify::comment_remover::rules_for("c-style");
    let s = super::collapse_whitespace(&s, rules.as_ref());
    let s = super::re_tighten_punct(&s, rules.as_ref());
    s.trim().to_owned()
}

/// High-quality CSS minification via lightningcss (100× better than regex).
/// Uses `minify_css_core` on parse or panic error.
///
/// Gated on the opt-in `css-quality` feature. With the feature off, lightningcss
/// is not compiled in and this degrades to the `minify_css_core` regex baseline
/// — every caller (`minify_css_quality` binding, `minify_html_quality`,
/// `minify_embedded_web`/`minify_style_blocks`) transparently gets the fallback.
#[cfg(feature = "css-quality")]
pub fn minify_css_quality(content: &str) -> String {
    std::panic::catch_unwind(|| {
        use lightningcss::stylesheet::{ParserOptions, PrinterOptions, StyleSheet};
        match StyleSheet::parse(content, ParserOptions::default()) {
            Ok(ss) => ss
                .to_css(PrinterOptions {
                    minify: true,
                    ..Default::default()
                })
                .map(|out| out.code.to_string())
                .unwrap_or_else(|_| minify_css_core(content)),
            Err(_) => minify_css_core(content),
        }
    })
    .unwrap_or_else(|_| minify_css_core(content))
}

/// Fallback when the `css-quality` feature is disabled (lightningcss not built):
/// the always-available regex baseline. Keeps CSS/SCSS/LESS + embedded `<style>`
/// minification working — just without parser-grade normalization (e.g. `0px`→`0`).
#[cfg(not(feature = "css-quality"))]
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
/// embedded `<style>` blocks (lightningcss) and `<script>` blocks (oxc, no
/// mangle — same treatment standalone JS/TS gets in the content view) and from
/// dropping syntax-identified HTML comments. Markup whitespace is preserved.
/// A generic comment-strip
/// barely touches these files because the compressible bytes live inside the
/// embedded languages, not the markup. Each sub-minifier falls back to the
/// original block text when it cannot handle the content.
pub fn minify_embedded_web(content: &str, _file_path: &str) -> String {
    let mut parser = tree_sitter::Parser::new();
    if parser
        .set_language(&tree_sitter_html::LANGUAGE.into())
        .is_err()
    {
        return content.to_owned();
    }
    let Some(tree) = parser.parse(content, None) else {
        return content.to_owned();
    };
    if tree.root_node().has_error() {
        return content.to_owned();
    }
    let mut edits = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        match node.kind() {
            "comment" => edits.push((node.start_byte(), node.end_byte(), String::new())),
            "script_element" | "style_element" => {
                let mut cursor = node.walk();
                let children: Vec<_> = node.named_children(&mut cursor).collect();
                let open = children.iter().find(|n| n.kind() == "start_tag");
                let inner = children.iter().find(|n| n.kind() == "raw_text");
                if let (Some(open), Some(inner)) = (open, inner) {
                    let tag = &content[open.byte_range()];
                    let source = &content[inner.byte_range()];
                    let compacted = if node.kind() == "style_element" {
                        Some(minify_css_quality(source))
                    } else if script_is_javascript(tag) {
                        minify_js_oxc(source, &script_virtual_path(tag), false)
                    } else {
                        None
                    };
                    if let Some(compacted) = compacted.filter(|s| s.len() < source.len()) {
                        edits.push((inner.start_byte(), inner.end_byte(), compacted));
                    }
                }
            }
            _ => {
                let mut cursor = node.walk();
                stack.extend(node.named_children(&mut cursor));
            }
        }
    }
    edits.sort_by_key(|e| e.0);
    let mut output = String::with_capacity(content.len());
    let mut offset = 0;
    for (start, end, replacement) in edits {
        output.push_str(&content[offset..start]);
        output.push_str(&replacement);
        offset = end;
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
    use super::{minify_html_core, minify_html_quality};

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
