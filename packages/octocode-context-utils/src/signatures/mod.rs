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
    let ext = get_extension_internal(file_path, true, "txt");
    if NO_SYMBOL_EXTS.contains(&ext.as_str()) {
        return Vec::new();
    }
    // tree-sitter path (highest accuracy)
    if let Some(entry) = languages::find_entry(&ext) {
        let cfg = LangExtractConfig {
            language: entry.language.clone(),
            body_query: entry.body_query,
        };
        if let Some(kept) = extract(content, &cfg) {
            return kept;
        }
    }
    // Heuristic path (30+ languages)
    heuristic::extract_heuristic(content, &ext).unwrap_or_default()
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
    let stripped = trimmed.trim_end_matches([';', ',']);
    matches!(stripped, "}" | "]" | ")" | "})" | "])" | "}]")
}

fn leading_indent_width(text: &str) -> usize {
    text.chars()
        .take_while(|ch| matches!(ch, ' ' | '\t'))
        .map(|ch| if ch == '\t' { 4 } else { 1 })
        .sum()
}

fn strip_leading_modifiers(mut text: &str) -> &str {
    const MODIFIERS: &[&str] = &[
        "public",
        "private",
        "protected",
        "internal",
        "static",
        "abstract",
        "final",
        "sealed",
        "open",
        "override",
        "async",
        "export",
        "pub",
        "mut",
        "readonly",
    ];

    loop {
        let before = text;
        for modifier in MODIFIERS {
            if let Some(rest) = text.strip_prefix(modifier) {
                if rest
                    .chars()
                    .next()
                    .is_some_and(|ch| ch.is_ascii_whitespace())
                {
                    text = rest.trim_start();
                    break;
                }
            }
        }
        if text == before {
            return text;
        }
    }
}

fn starts_with_boundary_keyword(text: &str) -> bool {
    let stripped = strip_leading_modifiers(text);
    [
        "case class ",
        "case object ",
        "data class ",
        "enum class ",
        "sealed class ",
        "abstract class ",
        "companion object",
        "class ",
        "interface ",
        "enum ",
        "record ",
        "struct ",
        "impl ",
        "trait ",
        "object ",
        "namespace ",
        "type ",
        "typealias ",
        "func ",
        "fn ",
        "fun ",
        "def ",
        "init ",
        "constructor ",
    ]
    .iter()
    .any(|prefix| stripped.starts_with(prefix))
}

fn is_nested_member_noise(text: &str, ext: &str) -> bool {
    let indent = leading_indent_width(text);
    if indent == 0 {
        return false;
    }

    let trimmed = text.trim();
    if matches!(ext, "html" | "htm" | "vue" | "svelte") {
        return false;
    }
    if matches!(ext, "css" | "scss" | "less") {
        return !trimmed.starts_with('@');
    }
    if ext == "scala" {
        let stripped = strip_leading_modifiers(trimmed);
        if stripped.starts_with("val ") || stripped.starts_with("var ") {
            return false;
        }
    }
    if trimmed.contains('(') || starts_with_boundary_keyword(trimmed) {
        return false;
    }

    matches!(
        ext,
        "ts" | "tsx"
            | "js"
            | "jsx"
            | "mjs"
            | "cjs"
            | "go"
            | "rs"
            | "java"
            | "cs"
            | "kt"
            | "kotlin"
            | "scala"
    )
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
    let ext = get_extension_internal(file_path, true, "txt");
    let offset_table = build_js_char_offset_table(content);
    let mut offsets: Vec<u32> = lines
        .iter()
        .filter(|(_, text)| {
            let t = text.trim();
            !t.is_empty() && !is_lone_delimiter(t) && !is_nested_member_noise(text, &ext)
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
            language: entry.language.clone(),
            body_query: entry.body_query,
        };
        // Prefer tree-sitter; use the centralized heuristic extractor when it
        // cannot produce a skeleton for this input.
        if let Some(kept) = extract(content, &cfg) {
            return renderer::render_skeleton(&kept, entry.comment_style);
        }
    }

    // ── heuristic path (all other supported languages + parser misses) ───────
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

    struct BoundaryFixture {
        name: &'static str,
        path: &'static str,
        source: &'static str,
        markers: &'static [&'static str],
        excluded_markers: &'static [&'static str],
    }

    fn js_offset_for_marker(source: &str, marker: &str) -> u32 {
        let byte_offset = source
            .find(marker)
            .unwrap_or_else(|| panic!("marker '{marker}' must exist in fixture"));
        source[..byte_offset]
            .chars()
            .map(char::len_utf16)
            .sum::<usize>() as u32
    }

    fn assert_boundary_fixture(fixture: &BoundaryFixture) {
        let offsets = get_semantic_boundary_offsets_inner(fixture.source, fixture.path);
        assert!(
            !offsets.is_empty(),
            "{} fixture must produce semantic boundaries",
            fixture.name
        );

        for marker in fixture.markers {
            let expected = js_offset_for_marker(fixture.source, marker);
            assert!(
                offsets.contains(&expected),
                "{} marker '{marker}' offset {expected} must be in {offsets:?}",
                fixture.name
            );
        }

        for marker in fixture.excluded_markers {
            let excluded = js_offset_for_marker(fixture.source, marker);
            assert!(
                !offsets.contains(&excluded),
                "{} marker '{marker}' offset {excluded} must not be in {offsets:?}",
                fixture.name
            );
        }
    }

    #[test]
    fn boundary_offsets_are_sorted_and_deduped() {
        let src =
            "export function foo() {\n  return 1;\n}\n\nexport function bar() {\n  return 2;\n}\n";
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
    fn semantic_boundary_fixture_suite_per_language() {
        let fixtures = [
            BoundaryFixture {
                name: "TypeScript",
                path: "fixture.ts",
                source: "export interface User {\n  id: string;\n}\n\nexport function loadUser(id: string) {\n  return id;\n}\n\nexport class UserStore {\n  get(id: string) {\n    return loadUser(id);\n  }\n}\n",
                markers: &[
                    "export interface User",
                    "export function loadUser",
                    "export class UserStore",
                ],
                excluded_markers: &["  id: string;"],
            },
            BoundaryFixture {
                name: "JavaScript",
                path: "fixture.js",
                source: "import fs from 'node:fs';\n\nexport function parseConfig(raw) {\n  return JSON.parse(raw);\n}\n\nclass Runner {\n  start() {\n    return fs.existsSync('.');\n  }\n}\n",
                markers: &["export function parseConfig", "class Runner"],
                excluded_markers: &[],
            },
            BoundaryFixture {
                name: "HTML",
                path: "fixture.html",
                source: "<!doctype html>\n<html>\n<head>\n  <meta name=\"viewport\" content=\"width=device-width\">\n  <link href=\"/app.css\" rel=\"stylesheet\">\n</head>\n<body>\n  <h1>Dashboard</h1>\n  <section id=\"reports\">\n    <p>Ready</p>\n  </section>\n  <script src=\"/app.js\"></script>\n</body>\n</html>\n",
                markers: &[
                    "<!doctype html",
                    "  <meta name",
                    "  <link href",
                    "  <h1>",
                    "  <section id",
                    "  <script src",
                ],
                excluded_markers: &[],
            },
            BoundaryFixture {
                name: "CSS",
                path: "fixture.css",
                source: ":root {\n  --gap: 1rem;\n}\n\n.card,\n.panel {\n  color: red;\n}\n\n@media (min-width: 40rem) {\n  .grid {\n    display: grid;\n  }\n}\n",
                markers: &[":root {", ".panel {", "@media"],
                excluded_markers: &["  --gap", "  .grid {"],
            },
            BoundaryFixture {
                name: "Python",
                path: "fixture.py",
                source: "class Service:\n    def run(self):\n        return 1\n\ndef top_level():\n    return Service()\n",
                markers: &["class Service", "    def run", "def top_level"],
                excluded_markers: &[],
            },
            BoundaryFixture {
                name: "Go",
                path: "fixture.go",
                source: "package main\n\ntype Server struct {\n    Port int\n}\n\nfunc NewServer() *Server {\n    return &Server{}\n}\n\nfunc (s *Server) Start() error {\n    return nil\n}\n",
                markers: &["type Server struct", "func NewServer", "func (s *Server) Start"],
                excluded_markers: &["    Port int"],
            },
            BoundaryFixture {
                name: "Rust",
                path: "fixture.rs",
                source: "pub struct Config {\n    pub port: u16,\n}\n\nimpl Config {\n    pub fn new(port: u16) -> Self {\n        Self { port }\n    }\n}\n\npub fn run(config: Config) {\n    let _ = config;\n}\n",
                markers: &["pub struct Config", "impl Config", "    pub fn new", "pub fn run"],
                excluded_markers: &["    pub port"],
            },
            BoundaryFixture {
                name: "Java",
                path: "Fixture.java",
                source: "public class Fixture {\n    public Fixture() {\n    }\n\n    public void handle() {\n        System.out.println(\"ok\");\n    }\n}\n",
                markers: &["public class Fixture", "    public Fixture", "    public void handle"],
                excluded_markers: &[],
            },
            BoundaryFixture {
                name: "Kotlin",
                path: "Fixture.kt",
                source: "class Calculator {\n    fun add(): Int {\n        return 1\n    }\n\n    private fun multiply() = 2\n\n    companion object {\n        const val PI = 3.14\n    }\n}\n",
                markers: &["class Calculator", "    fun add", "    companion object"],
                excluded_markers: &["        const val PI"],
            },
            BoundaryFixture {
                name: "Scala",
                path: "Fixture.scala",
                source: "package example\n\ncase class User(id: String)\n\nobject Service {\n  def load(id: String): User = {\n    User(id)\n  }\n\n  val value: Int = 1\n}\n",
                markers: &[
                    "package example",
                    "case class User",
                    "object Service",
                    "  def load",
                    "  val value",
                ],
                excluded_markers: &[],
            },
            BoundaryFixture {
                name: "C#",
                path: "Fixture.cs",
                source: "using System;\n\nnamespace App {\n    public class Worker {\n        public Worker() {\n        }\n\n        public void Run() {\n            Console.WriteLine(\"ok\");\n        }\n    }\n}\n",
                markers: &[
                    "using System",
                    "    public class Worker",
                    "        public Worker",
                    "        public void Run",
                ],
                excluded_markers: &[],
            },
        ];

        for fixture in fixtures {
            assert_boundary_fixture(&fixture);
        }
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
        assert!(
            offsets.is_empty(),
            "oversized input must yield empty offsets"
        );
    }

    #[test]
    fn js_char_offset_table_counts_utf16_units() {
        // ASCII-only: each char = 1 JS unit
        let src = "ab\ncd\n";
        let table = build_js_char_offset_table(src);
        // line 1: offset 0, line 2: offset 3 (a=1,b=1,\n=1), line 3: offset 6
        assert_eq!(table, vec![0, 3, 6]);
    }
}
