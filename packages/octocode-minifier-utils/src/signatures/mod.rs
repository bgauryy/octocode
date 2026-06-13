pub mod extractor;
pub mod heuristic;
pub mod languages;
pub mod renderer;

use crate::file_extension::get_extension_internal;
use extractor::{extract, LangExtractConfig};

pub const SIGNATURES_ONLY_HINT: &str = concat!(
    "Signatures/outline only — bodies and comments omitted; ",
    "the whole skeleton is returned in one response (never paginated). ",
    "Left gutter shows original line numbers; use startLine/endLine to read a body."
);

/// Extract a structural skeleton from `content`.
/// Returns `NNN| text` rendered string or `None`.
pub fn extract_signatures_inner(content: &str, file_path: &str) -> Option<String> {
    if content.len() > crate::minifier::MAX_SIZE {
        return None;
    }
    std::panic::catch_unwind(|| {
        let ext = get_extension_internal(file_path, true, "txt");
        extract_by_ext(content, &ext)
    })
    .unwrap_or(None)
}

/// Extensions where symbol extraction has no semantic value:
/// data/config formats have key-value pairs, not code signatures;
/// most prose formats have no reliable navigation anchors.
/// Code languages (Lua, Erlang, Clojure, VB) are intentionally excluded
/// even when their heuristic grows output — the skeleton is still useful.
const NO_SYMBOL_EXTS: &[&str] = &[
    // Data / config — no code signatures whatsoever
    "json",
    "jsonc",
    "json5",
    "yaml",
    "yml",
    "toml",
    "ini",
    "cfg",
    "conf",
    "config",
    "properties",
    "env",
    "csv",
    "tsv",
    "xml",
    "svg",
    // Prose/docs without a dedicated outline extractor.
    "rst",
    "txt",
    "log",
];

fn extract_by_ext(content: &str, ext: &str) -> Option<String> {
    // P0: never extract symbols for formats with no code signatures
    if NO_SYMBOL_EXTS.contains(&ext) {
        return None;
    }

    // ── tree-sitter path (top-10 languages) ─────────────────────────────────
    if let Some(entry) = languages::find_entry(ext) {
        let cfg = LangExtractConfig {
            language: (entry.language_fn)(),
            body_query: entry.body_query,
            comment_style: entry.comment_style,
        };
        // Try tree-sitter; fall back to heuristic on failure.
        if let Some(kept) = extract(content, &cfg) {
            return renderer::render_skeleton(&kept, entry.comment_style);
        }
        // Fall through to heuristic
    }

    // ── heuristic path (all other languages + TS fallback) ───────────────────
    let comment_style = comment_style_for(ext);
    let kept = heuristic::extract_heuristic(content, ext)?;
    renderer::render_skeleton(&kept, comment_style)
}

fn comment_style_for(ext: &str) -> &'static str {
    match ext {
        "py" | "rb" | "sh" | "bash" | "zsh" | "fish" | "coffee" | "r" | "nim" | "jl" | "pl"
        | "pm" | "ex" | "exs" | "cr" | "pp" => "hash",
        "hs" | "lhs" | "lua" | "erl" | "hrl" => "hash",
        "html" | "htm" | "vue" | "svelte" => "html",
        "sql" | "tsql" | "plsql" => "sql",
        "php" => "c-hash",
        "md" | "markdown" => "none",
        _ => "c",
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    fn extract(content: &str, path: &str) -> Option<String> {
        extract_signatures_inner(content, path)
    }

    // ── tree-sitter languages ─────────────────────────────────────────────────
    #[test]
    fn typescript_skeleton_keeps_signatures_drops_bodies() {
        let src = "\nexport function add(a: number, b: number): number {\n  return a + b;\n}\n\nexport class Calc {\n  value: number = 0;\n  multiply(x: number): number {\n    return this.value * x;\n  }\n}\n";
        let s = extract(src, "calc.ts").expect("TS must extract");
        assert!(s.contains("add"), "function preserved");
        assert!(s.contains("Calc"), "class preserved");
        assert!(s.contains("value"), "field preserved");
        assert!(s.contains("multiply"), "method sig preserved");
        assert!(!s.contains("return a + b"), "body dropped");
        assert!(!s.contains("this.value * x"), "body dropped");
    }

    #[test]
    fn python_skeleton_keeps_imports_classes_and_defs() {
        let src = "\nimport os\n\nclass Foo:\n    name: str\n\n    def bar(self, x: int) -> str:\n        return str(x)\n\ndef top_level():\n    pass\n";
        let s = extract(src, "foo.py").expect("python must extract");
        assert!(s.contains("import os"), "must keep import");
        assert!(s.contains("class Foo"), "must keep class");
        assert!(s.contains("def bar"), "must keep method sig");
        assert!(s.contains("def top_level"), "must keep top-level def");
        assert!(!s.contains("return str"), "body dropped");
        assert!(!s.contains("pass"), "body dropped");
    }

    #[test]
    fn python_one_line_def_keeps_its_signature_row() {
        let src = "def f(): return 1\n\ndef g():\n    return 2\n";
        let s = extract(src, "one.py").expect("must extract");
        assert!(
            s.contains("def f(): return 1"),
            "one-liner signature dropped: '{s}'"
        );
        assert!(s.contains("def g():"));
        assert!(
            !s.contains("return 2"),
            "multi-line body must still drop: '{s}'"
        );
    }

    #[test]
    fn rust_skeleton_drops_fn_bodies() {
        let src = "\npub fn greet(name: &str) -> String {\n    format!(\"Hello, {}\", name)\n}\n\npub struct Point { x: f64, y: f64 }\n\nimpl Point {\n    pub fn distance(&self, other: &Point) -> f64 {\n        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()\n    }\n}\n";
        let s = extract(src, "geo.rs").expect("rust must extract");
        assert!(s.contains("greet"));
        assert!(!s.contains("format!"), "body dropped");
    }

    #[test]
    fn go_skeleton_drops_fn_bodies() {
        let src = "\npackage main\n\nimport \"fmt\"\n\nfunc Add(a, b int) int {\n    return a + b\n}\n\ntype Server struct {\n    Port int\n}\n\nfunc (s *Server) Start() error {\n    fmt.Println(\"starting\")\n    return nil\n}\n";
        let s = extract(src, "main.go").expect("go must extract");
        assert!(s.contains("Add") || s.contains("func"));
        assert!(!s.contains("Println"), "body dropped");
    }

    #[test]
    fn java_skeleton_drops_method_bodies() {
        let src = "\npublic class Calculator {\n    private int value;\n\n    public Calculator(int initial) {\n        this.value = initial;\n    }\n\n    public int add(int x) {\n        return value + x;\n    }\n}\n";
        let s = extract(src, "Calculator.java").expect("java must extract");
        assert!(s.contains("Calculator") || s.contains("add"));
        assert!(!s.contains("return value"), "body dropped");
    }

    #[test]
    fn c_skeleton_drops_fn_bodies() {
        let src = "\n#include <stdio.h>\n\nint add(int a, int b) {\n    return a + b;\n}\n\nvoid greet(const char *name) {\n    printf(\"Hello, %s\\n\", name);\n}\n";
        let s = extract(src, "math.c").expect("c must extract");
        assert!(s.contains("add") || s.contains("int"));
        assert!(!s.contains("printf"), "body dropped");
    }

    // ── NO_SYMBOL_EXTS denylist: data / config / unsupported prose return None ─
    #[test]
    fn data_and_unsupported_prose_formats_return_none() {
        let cases: &[(&str, &str)] = &[
            ("{\"key\":\"value\",\"count\":42}", "data.json"),
            ("// comment\n{\"a\": 1}", "tsconfig.json"),
            ("key: value\ncount: 42", "config.yaml"),
            ("name: my-app\nversion: 1.0.0", "package.yml"),
            ("[package]\nname = \"foo\"", "Cargo.toml"),
            ("[section]\nkey = value", "config.ini"),
            ("Title\n=====\n\nProse.", "docs.rst"),
        ];
        for (content, path) in cases {
            assert!(
                extract(content, path).is_none(),
                "{path} has no code signatures — must return None"
            );
        }
    }

    #[test]
    fn markdown_skeleton_keeps_headings_links_and_list_items() {
        let src = r#"---
title: Guide
draft: false
---

# Project

Intro with [Docs](https://example.com/docs) and [API][api].

## Install ##

- yarn install
* cargo test

```ts
export function hidden() {
  return 1;
}
```

Details that should not be part of the outline.

API
===

[api]: ./api.md
"#;
        let s = extract(src, "README.md").expect("markdown must extract");
        assert!(s.contains("frontmatter: title"));
        assert!(s.contains("# Project"));
        assert!(s.contains("links: [Docs](https://example.com/docs), [API][api]"));
        assert!(s.contains("## Install"));
        assert!(s.contains("- yarn install"));
        assert!(s.contains("* cargo test"));
        assert!(s.contains("code fence: ts"));
        assert!(s.contains("# API"));
        assert!(s.contains("link ref: [api]: ./api.md"));
        assert!(!s.contains("hidden"));
        assert!(!s.contains("Details that should not"));
    }

    #[test]
    fn code_formats_still_extract_despite_denylist() {
        assert!(extract(
            "CREATE TABLE users (id INT, name VARCHAR(255));",
            "schema.sql"
        )
        .is_some());
        assert!(extract(
            "export function add(a: number, b: number): number { return a + b; }",
            "math.ts"
        )
        .is_some());
    }

    // ── size cap ──────────────────────────────────────────────────────────────
    #[test]
    fn oversized_input_returns_none_without_parsing() {
        let src = "function f(){ return 1; }\n".repeat(45_000); // ~1.17MB
        assert!(extract(&src, "big.ts").is_none());
    }
}
