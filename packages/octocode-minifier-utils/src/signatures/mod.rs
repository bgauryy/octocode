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

/// Returns `(1-based line number, text)` pairs for every line that starts a
/// top-level semantic block.  Same tree-sitter / heuristic dispatch as
/// `extract_signatures_inner` but skips the renderer — callers get the raw
/// list so they can map line numbers to char offsets without string parsing.
///
/// Returns an empty Vec for data/config files (`NO_SYMBOL_EXTS`), files above
/// the 1 MB guard, and any language where extraction yields nothing.
pub fn extract_boundary_lines_inner(content: &str, file_path: &str) -> Vec<(usize, String)> {
    if content.len() > crate::minifier::MAX_SIZE {
        return Vec::new();
    }
    std::panic::catch_unwind(|| {
        let ext = get_extension_internal(file_path, true, "txt");
        if NO_SYMBOL_EXTS.contains(&ext.as_str()) {
            return Vec::new();
        }
        // tree-sitter path (highest accuracy)
        if let Some(entry) = languages::find_entry(&ext) {
            let cfg = LangExtractConfig {
                language: (entry.language_fn)(),
                body_query: entry.body_query,
                comment_style: entry.comment_style,
            };
            if let Some(kept) = extract(content, &cfg) {
                return kept;
            }
            // Fall through to heuristic on tree-sitter failure
        }
        // Heuristic path (30+ languages)
        heuristic::extract_heuristic(content, &ext).unwrap_or_default()
    })
    .unwrap_or_default()
}

/// Build a table of JS char offsets (UTF-16 code units) for each line start.
/// `table[i]` is the offset of the first char on line `i + 1` (1-based lines).
fn build_js_char_offset_table(content: &str) -> Vec<u32> {
    let mut table: Vec<u32> = vec![0]; // line 1 starts at offset 0
    let mut js_chars: u32 = 0;
    for ch in content.chars() {
        js_chars = js_chars.saturating_add(ch.len_utf16() as u32);
        if ch == '\n' {
            table.push(js_chars);
        }
    }
    table
}

/// True when `trimmed` is a lone closing delimiter — it closes a block rather
/// than starting one, so it must not be used as a chunk boundary.
/// Examples: `}`, `};`, `]);`, `)`, `})`, `})`
fn is_lone_delimiter(trimmed: &str) -> bool {
    let stripped = trimmed.trim_end_matches(|c: char| c == ';' || c == ',');
    matches!(stripped, "}" | "]" | ")" | "})" | "])" | "}]")
}

/// Convert `(line_number, text)` pairs to sorted, deduplicated JS char offsets.
///
/// Blank lines and lone closing delimiters are skipped — they are preserved by
/// the tree-sitter extractor (because they are outside function bodies) but
/// are not meaningful chunk boundaries for pagination.
///
/// The offsets align with JavaScript `string.substring()` — pass directly to
/// the TypeScript pagination layer.
pub fn get_semantic_boundary_offsets_inner(content: &str, file_path: &str) -> Vec<u32> {
    let lines = extract_boundary_lines_inner(content, file_path);
    if lines.is_empty() {
        return Vec::new();
    }
    let offset_table = build_js_char_offset_table(content);
    let mut offsets: Vec<u32> = lines
        .iter()
        .filter(|(_, text)| {
            let t = text.trim();
            !t.is_empty() && !is_lone_delimiter(t)
        })
        .filter_map(|(line_no, _)| {
            // line_no is 1-based; table[i] is 0-based index
            offset_table.get(line_no.saturating_sub(1)).copied()
        })
        .collect();
    offsets.dedup(); // keep first of any run of identical values (rare)
    offsets
}

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

    // ── get_semantic_boundary_offsets_inner ───────────────────────────────────

    #[test]
    fn boundary_offsets_are_sorted_and_deduped() {
        let src = "export function foo() {\n  return 1;\n}\n\nexport function bar() {\n  return 2;\n}\n";
        let offsets = get_semantic_boundary_offsets_inner(src, "mod.ts");
        assert!(!offsets.is_empty(), "must find boundaries in TS");
        for w in offsets.windows(2) {
            assert!(w[0] < w[1], "offsets must be strictly increasing");
        }
    }

    #[test]
    fn boundary_offsets_first_entry_is_zero_for_top_of_file_definition() {
        let src = "export function first() {\n  return 0;\n}\n\nexport function second() {\n  return 1;\n}\n";
        let offsets = get_semantic_boundary_offsets_inner(src, "a.ts");
        assert_eq!(offsets[0], 0, "first definition should start at offset 0");
    }

    #[test]
    fn boundary_offsets_align_with_function_starts() {
        let src = "function foo() {\n  const x = 1;\n}\n\nfunction bar() {\n  return 2;\n}\n";
        let offsets = get_semantic_boundary_offsets_inner(src, "util.js");
        // Second boundary should be at the start of `function bar()`
        let bar_pos = src.find("function bar").unwrap() as u32;
        assert!(
            offsets.contains(&bar_pos),
            "bar offset {bar_pos} must be in {offsets:?}"
        );
    }

    #[test]
    fn boundary_offsets_python_finds_def_and_class() {
        let src = "def foo():\n    return 1\n\ndef bar():\n    return 2\n\nclass Baz:\n    pass\n";
        let offsets = get_semantic_boundary_offsets_inner(src, "module.py");
        assert!(offsets.len() >= 3, "must find 3 boundaries in Python");
        assert_eq!(offsets[0], 0, "first def at offset 0");
    }

    #[test]
    fn boundary_offsets_go_finds_func_and_type() {
        let src = "package main\n\nfunc Foo() {}\n\nfunc Bar() {}\n\ntype S struct{}\n";
        let offsets = get_semantic_boundary_offsets_inner(src, "main.go");
        assert!(offsets.len() >= 2, "must find Foo and Bar");
    }

    #[test]
    fn boundary_offsets_empty_for_data_files() {
        for (content, path) in &[
            ("{\"key\":1}", "data.json"),
            ("key: value", "cfg.yaml"),
            ("[section]\nkey=val", "app.ini"),
        ] {
            let offsets = get_semantic_boundary_offsets_inner(content, path);
            assert!(
                offsets.is_empty(),
                "{path} must yield empty offsets (data file)"
            );
        }
    }

    #[test]
    fn boundary_offsets_empty_for_oversized_input() {
        let src = "function f() {}\n".repeat(70_000);
        let offsets = get_semantic_boundary_offsets_inner(&src, "big.ts");
        assert!(offsets.is_empty(), "oversized input must yield empty offsets");
    }

    #[test]
    fn js_char_offset_table_counts_utf16_units() {
        // ASCII-only: each char = 1 JS unit
        let src = "ab\ncd\n";
        let table = build_js_char_offset_table(src);
        // line 1: offset 0, line 2: offset 3 (a=1,b=1,\n=1), line 3: offset 6
        assert_eq!(table, vec![0, 3, 6]);
    }

    #[test]
    fn boundary_offsets_rust_finds_pub_fn_and_impl() {
        let src = "pub fn foo() {\n    let x = 1;\n}\n\nimpl Bar {\n    pub fn baz(&self) {}\n}\n";
        let offsets = get_semantic_boundary_offsets_inner(src, "lib.rs");
        assert!(!offsets.is_empty());
        // `pub fn foo` is at char 0
        assert_eq!(offsets[0], 0);
    }

    #[test]
    fn boundary_offsets_java_includes_class_and_method() {
        let src = "public class Foo {\n    public void bar() {\n        int x = 1;\n    }\n    private int baz() {\n        return 0;\n    }\n}\n";
        let offsets = get_semantic_boundary_offsets_inner(src, "Foo.java");
        // Should find at least class + 2 methods
        assert!(offsets.len() >= 2, "must find class + methods, got {offsets:?}");
    }
}
