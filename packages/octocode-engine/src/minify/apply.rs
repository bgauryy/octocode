use crate::comment_remover::remove_comments;
use crate::file_extension::get_extension_internal;
use crate::minifier::{get_file_config, minify_content_sync_inner, MAX_SIZE};
use crate::strategies::{
    minify_code_core, minify_css_quality, minify_embedded_web, minify_general_core, minify_js_oxc,
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
        // Resolve the file-type config through the SAME shared resolver the full
        // minify path uses (`minifier::get_file_config`) rather than re-deriving
        // the basename / indentation-sensitive lookup here. The two entry points
        // otherwise drift silently — e.g. a new indentation-sensitive name added
        // to `get_file_config` would never reach the content view. `ext` is still
        // derived locally for the family checks below, because css and html both
        // map to the "aggressive" strategy and strategy alone can't tell them
        // apart (css → css_quality, html → embedded_web).
        let cfg = get_file_config(file_path);

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

        // HTML / Vue / Svelte content-view: keep the markup readable but minify
        // the embedded <style> (lightningcss) and <script> (oxc) blocks where
        // the compressible bytes actually live. A generic comment-strip barely
        // touches these files (benchmark: ~0% content-view cut).
        if matches!(ext.as_str(), "html" | "htm" | "vue" | "svelte") {
            return minify_embedded_web(content, file_path);
        }

        // JS/TS: use OXC without mangling — preserves names for agent readability
        if crate::file_extension::is_js_ts_extension(&ext) {
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

    // ── HTML / Vue / Svelte embedded content view ────────────────────────────
    #[test]
    fn html_content_view_minifies_embedded_style_and_script_and_keeps_markup() {
        let src = "<!doctype html>\n<html>\n<head>\n  <!-- nav styles -->\n  <style>\n    .card {\n      color: red;\n      padding: 0px;\n    }\n  </style>\n</head>\n<body>\n  <h1>Dashboard</h1>\n  <script>\n    function greet(name) {\n      // say hello\n      console.log(\"hi \" + name);\n    }\n  </script>\n</body>\n</html>\n";
        let out = apply_content_view_minification_inner(src, "page.html");
        assert!(out.len() < src.len(), "HTML embedded view must compress");
        assert!(
            out.contains("<h1>Dashboard</h1>"),
            "markup preserved: {out}"
        );
        assert!(!out.contains("nav styles"), "HTML comment dropped: {out}");
        assert!(!out.contains("// say hello"), "JS comment dropped: {out}");
        assert!(out.contains("greet"), "JS identifiers preserved: {out}");
    }

    #[test]
    fn vue_sfc_content_view_minifies_ts_script_and_style() {
        let src = "<template>\n  <section id=\"app\">{{ title }}</section>\n</template>\n\n<style scoped>\n.title {\n  font-weight: bold;\n  margin: 0px;\n}\n</style>\n\n<script lang=\"ts\">\nexport default {\n  data(): { title: string } {\n    // initial state\n    return { title: \"Hi\" };\n  },\n};\n</script>\n";
        let out = apply_content_view_minification_inner(src, "App.vue");
        assert!(out.len() < src.len(), "Vue SFC view must compress: {out}");
        assert!(out.contains("id=\"app\""), "template preserved: {out}");
        assert!(!out.contains("initial state"), "TS comment dropped: {out}");
    }

    #[test]
    fn svelte_external_script_left_intact() {
        let src =
            "<script src=\"/vendor.js\"></script>\n<h1>Title</h1>\n<!-- footer -->\n<p>Body</p>\n";
        let out = apply_content_view_minification_inner(src, "Page.svelte");
        assert!(
            out.contains("src=\"/vendor.js\""),
            "external script kept: {out}"
        );
        assert!(!out.contains("footer"), "comment dropped: {out}");
        assert!(out.contains("<h1>Title</h1>"));
    }

    // ── shared config resolver (drift guard) ────────────────────────────────
    #[test]
    fn content_view_uses_shared_config_resolver_for_indentation_sensitive_names() {
        // "Makefile" has no extension; only the shared `get_file_config` maps
        // indentation-sensitive basenames to the "sh" config (hash comments).
        // If the content view re-derived config inline and drifted, the comment
        // would survive. This guards that both entry points share one resolver.
        let src = "all:\n\t# build step comment\n\techo hi\n";
        let out = apply_content_view_minification_inner(src, "Makefile");
        assert!(
            !out.contains("build step comment"),
            "hash comment must be stripped via the shared sh config: {out}"
        );
        assert!(out.contains("echo hi"), "recipe body preserved: {out}");
    }

    // ── size cap ──────────────────────────────────────────────────────────────
    #[test]
    fn content_view_returns_oversized_input_untouched() {
        let src = "text  \n".repeat(180_000); // ~1.26MB — trailing spaces WOULD minify
        let out = apply_content_view_minification_inner(&src, "big.md");
        assert_eq!(out, src);
    }
}
