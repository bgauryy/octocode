use crate::minify::config::{indentation_sensitive_names, minify_config, FileTypeConfig};
use crate::minify::strategies::{
    minify_aggressive, minify_conservative, minify_css_quality, minify_general_core,
    minify_html_core, minify_html_quality, minify_javascript_core, minify_js_oxc,
    minify_json_core_inner, minify_markdown_core,
};
use crate::text::file_extension::get_extension_internal;
use crate::types::MinifyResult;

pub(crate) const MAX_SIZE: usize = 1024 * 1024; // 1 MB guard, shared by all FFI content entry points

pub fn get_file_config(file_path: &str) -> Option<&'static FileTypeConfig> {
    let ext = get_extension_internal(file_path, true, "txt");
    let basename = file_path
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(file_path)
        .to_lowercase();

    if indentation_sensitive_names().contains(basename.as_str()) {
        return minify_config().get("sh"); // hash comments, conservative
    }

    minify_config().get(ext.as_str())
}

pub fn comment_groups(cfg: &FileTypeConfig) -> Vec<&'static str> {
    cfg.comments.map(|c| c.to_vec()).unwrap_or_default()
}

/// Synchronous full minification.
pub fn minify_content_sync_inner(content: &str, file_path: &str) -> String {
    if content.len() > MAX_SIZE {
        return content.to_owned();
    }
    dispatch_inner(content, file_path).content
}

/// Full minification returning MinifyResult.
pub fn minify_content_result_inner(content: &str, file_path: &str) -> MinifyResult {
    let content_size = content.len();
    if content_size > MAX_SIZE {
        return MinifyResult::fail(
            content.to_owned(),
            format!(
                "File too large: {:.2}MB exceeds 1MB limit",
                content_size as f64 / 1_048_576.0
            ),
        );
    }

    let out = dispatch_inner(content, file_path);
    if out.failed {
        MinifyResult {
            content: out.content,
            failed: true,
            r#type: out.strategy.to_owned(),
            reason: out.reason.map(str::to_owned),
        }
    } else {
        MinifyResult::ok(out.content, out.strategy)
    }
}

struct DispatchResult {
    content: String,
    strategy: &'static str,
    failed: bool,
    reason: Option<&'static str>,
}

impl DispatchResult {
    fn ok(content: String, strategy: &'static str) -> Self {
        Self {
            content,
            strategy,
            failed: false,
            reason: None,
        }
    }

    fn json(content: String, failed: bool) -> Self {
        Self {
            content,
            strategy: "json",
            failed,
            reason: failed.then_some("Invalid JSON"),
        }
    }
}

fn dispatch_inner(content: &str, file_path: &str) -> DispatchResult {
    let Some(cfg) = get_file_config(file_path) else {
        return DispatchResult::ok(minify_general_core(content), "general");
    };
    let ext = get_extension_internal(file_path, true, "txt");
    let grps = comment_groups(cfg);

    match cfg.strategy {
        "oxc" | "conservative" => {
            let out = if crate::text::file_extension::is_js_ts_extension(&ext) {
                minify_js_oxc(content, file_path, true)
                    .unwrap_or_else(|| minify_javascript_core(content))
            } else {
                minify_conservative(content, Some(&grps))
            };
            DispatchResult::ok(out, cfg.strategy)
        }
        "aggressive" => {
            let out = if matches!(ext.as_str(), "css" | "less" | "scss") {
                minify_css_quality(content)
            } else if matches!(ext.as_str(), "html" | "htm") {
                minify_html_quality(content)
            } else if matches!(ext.as_str(), "xml" | "svg") {
                minify_html_core(content)
            } else {
                minify_aggressive(content, Some(&grps))
            };
            DispatchResult::ok(out, "aggressive")
        }
        "json" => {
            let (out, failed) = minify_json_core_inner(content);
            DispatchResult::json(out, failed)
        }
        "markdown" => DispatchResult::ok(minify_markdown_core(content), "markdown"),
        _ => DispatchResult::ok(minify_general_core(content), "general"),
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    // ── TS type stripping in the full-minify path (OXC) ───────────────────────
    #[test]
    fn full_minify_strips_import_type() {
        let src = "import type { Foo } from './foo';\nimport { bar } from './bar';\nexport function greet(name: string): void {\n  bar();\n}\n";
        let out = minify_content_sync_inner(src, "greet.ts");
        assert!(
            !out.contains("import type"),
            "must strip 'import type': {out}"
        );
    }

    #[test]
    fn full_minify_strips_interfaces() {
        let src = "interface User { name: string; age: number; }\nexport function getName(u: User): string { return u.name; }\n";
        let out = minify_content_sync_inner(src, "user.ts");
        assert!(!out.contains("interface"), "must strip interfaces: {out}");
    }

    #[test]
    fn full_minify_strips_type_aliases() {
        let src = "type Id = string | number;\nexport function process(id: Id): string { return String(id); }\n";
        let out = minify_content_sync_inner(src, "util.ts");
        assert!(!out.contains("type Id"), "must strip type aliases: {out}");
    }

    #[test]
    fn full_minify_preserves_runtime_code_after_type_stripping() {
        let src = "import type { Opts } from './opts';\ninterface Config { host: string; }\ntype Port = number;\nexport function connect(host: string, port: number): boolean {\n  return host.length > 0 && port > 0;\n}\n";
        let out = minify_content_sync_inner(src, "connect.ts");
        assert!(
            out.contains("connect"),
            "runtime function must survive: {out}"
        );
        assert!(!out.contains("import type"));
        assert!(!out.contains("interface"));
    }

    // ── dispatch routing preserves UTF-8 on the aggressive path ───────────────
    #[test]
    fn lua_dispatch_preserves_non_ascii() {
        let out = minify_content_sync_inner("local s = \"café → naïve\" { x = 1 }", "a.lua");
        assert!(
            out.contains("café → naïve"),
            "aggressive dispatch corrupted UTF-8: '{out}'"
        );
        assert!(!out.contains('Ã'), "Latin-1 mojibake detected: '{out}'");
    }

    #[test]
    fn sync_and_result_paths_share_dispatch_outputs() {
        for (path, src) in [
            ("data.json", "{\"a\": 1}"),
            ("readme.md", "# Title\n\nBody text\n"),
            ("style.css", "h1 { color: red; margin: 0px; }"),
            (
                "script.ts",
                "export function add(a: number, b: number) { return a + b; }",
            ),
            ("notes.txt", "hello\n\n\nworld"),
        ] {
            let sync = minify_content_sync_inner(src, path);
            let result = minify_content_result_inner(src, path);
            assert!(!result.failed, "{path} should minify successfully");
            assert_eq!(sync, result.content, "{path} dispatch diverged");
        }
    }

    #[test]
    fn rich_ast_aliases_have_explicit_minify_strategies() {
        let cases = [
            (
                "module.mts",
                "import type { Foo } from './foo';\nexport function f(x: Foo): string { return String(x); }\n",
                "conservative",
                "import type",
            ),
            (
                "module.cts",
                "import type { Foo } from './foo';\nexport function f(x: Foo): string { return String(x); }\n",
                "conservative",
                "import type",
            ),
            (
                "types.pyi",
                "# comment\nclass User: ...\n",
                "conservative",
                "# comment",
            ),
            (
                "header.hh",
                "// comment\nint add(int a, int b);\n",
                "conservative",
                "comment",
            ),
            (
                "header.hxx",
                "// comment\nint add(int a, int b);\n",
                "conservative",
                "comment",
            ),
        ];

        for (path, src, expected_strategy, removed_text) in cases {
            let result = minify_content_result_inner(src, path);
            assert!(!result.failed, "{path} should minify successfully");
            assert_eq!(
                result.r#type, expected_strategy,
                "{path} must not fall through to general text minify"
            );
            assert!(
                !result.content.contains(removed_text),
                "{path} should use its language-aware minify path: {}",
                result.content
            );
        }
    }

    #[test]
    fn minify_outputs_are_deterministic_for_representative_strategies() {
        let cases = [
            (
                "script.ts",
                "export function add(a: number, b: number) { return a + b; }\n",
            ),
            ("data.jsonc", "{\n  // comment\n  \"a\": 1,\n}\n"),
            ("style.css", ".card { color: red; margin: 0px; }\n"),
            (
                "page.html",
                "<style>.x { margin: 0px; }</style><h1>Hi</h1>\n",
            ),
            ("readme.md", "# Title\n\nBody text\n\n"),
            ("query.sql", "-- comment\nSELECT * FROM users;\n"),
            ("note.txt", "hello\n\n\nworld\n"),
        ];

        for (path, src) in cases {
            let first = minify_content_result_inner(src, path);
            assert!(!first.failed, "{path} should minify successfully");
            for _ in 0..8 {
                let next = minify_content_result_inner(src, path);
                assert_eq!(next.failed, first.failed, "{path} failed flag drifted");
                assert_eq!(next.r#type, first.r#type, "{path} strategy drifted");
                assert_eq!(next.content, first.content, "{path} output drifted");
            }
        }
    }

    #[test]
    fn invalid_json_result_is_marked_failed() {
        let r = minify_content_result_inner("{ invalid json", "bad.json");
        assert!(r.failed);
        assert_eq!(r.r#type, "json");
        assert_eq!(r.reason.as_deref(), Some("Invalid JSON"));
    }

    // ── size cap contract ─────────────────────────────────────────────────────
    #[test]
    fn oversized_input_flagged_failed_with_content_untouched() {
        let big = "x".repeat(MAX_SIZE + 1);
        let r = minify_content_result_inner(&big, "big.txt");
        assert!(r.failed);
        assert_eq!(r.content, big);
    }

    #[test]
    fn oversized_input_returned_unchanged_by_sync_path() {
        let big = "x".repeat(MAX_SIZE + 1);
        assert_eq!(minify_content_sync_inner(&big, "big.txt"), big);
    }
}
