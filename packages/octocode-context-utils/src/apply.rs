use crate::comment_remover::remove_comments;
use crate::config::{indentation_sensitive_names, minify_config};
use crate::file_extension::get_extension_internal;
use crate::minifier::{minify_content_sync_inner, MAX_SIZE};
use crate::strategies::{
    minify_code_core, minify_css_quality, minify_general_core, minify_js_oxc,
    minify_json_readable_inner, minify_markdown_core,
};

/// Full minification — return minified if shorter, else original.
pub fn apply_minification_inner(content: &str, file_path: &str) -> String {
    let minified = std::panic::catch_unwind(|| minify_content_sync_inner(content, file_path))
        .unwrap_or_else(|_| content.to_owned());
    if minified.len() < content.len() {
        minified
    } else {
        content.to_owned()
    }
}

/// Content-view minification — agent-readable, preserves indentation.
/// Agent-readable content view minification pipeline.
pub fn apply_content_view_minification_inner(content: &str, file_path: &str) -> String {
    if content.len() > MAX_SIZE {
        return content.to_owned();
    }
    let result = std::panic::catch_unwind(|| {
        let ext = get_extension_internal(file_path, true, "txt");
        let basename = file_path
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(file_path)
            .to_lowercase();

        let cfg = if indentation_sensitive_names().contains(basename.as_str()) {
            minify_config().get("sh")
        } else {
            minify_config().get(ext.as_str())
        };

        if matches!(ext.as_str(), "json" | "jsonc" | "json5") {
            let (out, _) = minify_json_readable_inner(content);
            return out;
        }

        if cfg.map(|c| c.strategy) == Some("markdown") {
            return minify_markdown_core(content);
        }

        // P2: CSS / SCSS / LESS content-view: use lightningcss (much better than blank-line collapse)
        if matches!(ext.as_str(), "css" | "scss" | "less" | "sass") {
            return minify_css_quality(content);
        }

        // JS/TS: use OXC without mangling — preserves names for agent readability
        if matches!(ext.as_str(), "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs") {
            if let Some(oxc_out) = minify_js_oxc(content, file_path, false) {
                return oxc_out;
            }
            // OXC failed — fall through to comment-strip + code-core
        }

        let stripped = if let Some(c) = cfg {
            if let Some(groups) = c.comments {
                remove_comments(content, groups)
            } else {
                content.to_owned()
            }
        } else {
            content.to_owned()
        };

        match cfg.map(|c| c.strategy) {
            None | Some("general") => minify_general_core(&stripped),
            _ => minify_code_core(&stripped),
        }
    })
    .unwrap_or_else(|_| content.to_owned());

    if result.len() < content.len() {
        result
    } else {
        content.to_owned()
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_view_keeps_valid_json_intact() {
        let out = apply_content_view_minification_inner(r#"{"a":1,"b":  2}"#, "foo.json");
        assert!(out.contains('a'));
    }

    #[test]
    fn content_view_strips_markdown_html_comments() {
        let out = apply_content_view_minification_inner(
            "# Title\n\nText <!-- hidden --> end\n",
            "readme.md",
        );
        assert!(out.contains("Title"));
        assert!(!out.contains("hidden"));
    }

    #[test]
    fn content_view_strips_ts_type_only_imports() {
        let src = "import type { Foo } from './foo';\nexport function add(a: number, b: number): number {\n  return a + b;\n}\n";
        let out = apply_content_view_minification_inner(src, "math.ts");
        assert!(
            !out.contains("import type"),
            "content-view must strip 'import type': {out}"
        );
    }

    #[test]
    fn content_view_strips_all_js_comment_classes() {
        // The "standard" view contract removes known language comments —
        // normal and jsdoc default to KEPT in oxc codegen, so this guards
        // the explicit CommentOptions in minify_js_oxc.
        let src = "import { useState } from \"react\";\n// Top-level comment that should be stripped\nexport function f() {\n  /** jsdoc to strip */\n  return useState;\n}\n";
        let out = apply_content_view_minification_inner(src, "x.tsx");
        assert!(
            !out.contains("Top-level comment"),
            "normal comments must be stripped: '{out}'"
        );
        assert!(
            !out.contains("jsdoc to strip"),
            "jsdoc comments must be stripped: '{out}'"
        );
        assert!(out.contains("useState"));
    }

    // ── CSS content view (lightningcss) ───────────────────────────────────────
    #[test]
    fn css_content_view_compresses_and_strips_comments() {
        let src = "h1 { color: red; font-weight: bold; }\np { margin: 0px; padding: 0px; }\n/* comment */\n.foo { display: flex; }";
        let out = apply_content_view_minification_inner(src, "style.css");
        assert!(
            out.len() < src.len(),
            "CSS content-view must compress ({} vs {})",
            out.len(),
            src.len()
        );
        assert!(!out.contains("/* comment */"));
    }

    #[test]
    fn scss_content_view_compresses() {
        let src = ".container {\n  display: flex;\n  /* comment */\n  flex-direction: row;\n  padding: 0px 0px;\n}\n.header { color: red; /* header comment */ }";
        let out = apply_content_view_minification_inner(src, "styles.scss");
        assert!(out.len() < src.len());
    }

    #[test]
    fn css_content_view_output_keeps_selectors_without_growing() {
        let src = "body { margin: 0; padding: 0; background-color: #fff; }\nh1 { font-size: 2em; color: #333; }";
        let out = apply_content_view_minification_inner(src, "main.css");
        assert!(out.contains("body") || out.contains("h1"));
        assert!(out.len() <= src.len());
    }

    #[test]
    fn css_content_view_strips_redundant_zero_px() {
        let src = "div { margin: 0px; padding: 0px 0px; border-width: 0px; }";
        let out = apply_content_view_minification_inner(src, "base.css");
        assert!(
            out.len() < src.len(),
            "lightningcss must strip 0px: '{out}'"
        );
        assert!(!out.contains("0px"), "0px should become 0: '{out}'");
    }

    // ── size cap ──────────────────────────────────────────────────────────────
    #[test]
    fn content_view_returns_oversized_input_untouched() {
        let src = "text  \n".repeat(180_000); // ~1.26MB — trailing spaces WOULD minify
        let out = apply_content_view_minification_inner(&src, "big.md");
        assert_eq!(out, src);
    }
}
