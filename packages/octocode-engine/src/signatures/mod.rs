pub mod extractor;
pub mod graph_facts;
pub mod js_oxc;

pub(crate) fn extract_graph_facts_inner(content: &str, file_path: &str) -> Option<String> {
    js_oxc::extract_graph_facts(content, file_path)
        .or_else(|| graph_facts::extract_graph_facts(content, file_path))
}
pub mod languages;
pub mod renderer;

use crate::text::file_extension::get_extension_internal;
use extractor::{extract, LangExtractConfig};

/// Runs `f` (a parse + AST/tree walk) on a dedicated thread with a much
/// larger stack than napi's calling thread provides.
///
/// A recursive-descent parser (oxc) or a naive recursive walk over a parsed
/// tree (this crate's own symbol/skeleton renderers) can blow a default-size
/// native stack on pathologically nested input — thousands of levels of `(`,
/// `[`, or `{` — crashing the whole process with SIGSEGV, a fault
/// `catch_unwind` cannot intercept (unlike a parser panic/ICE, which the
/// `catch_unwind` wrapping each caller already contains). This matters most
/// for the actual napi calling thread specifically, which has measurably less
/// native stack headroom than e.g. a `cargo test` thread — content that
/// parses fine in a unit test can still crash the real process. A bigger
/// stack does not make unbounded recursion safe in the limit, but it raises
/// the input depth needed to trigger it by roughly the same factor as the
/// size increase — the standard mitigation for wrapping a recursive-descent
/// parser or tree walker.
pub(crate) fn run_on_deep_stack<T: Send + Default + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
) -> T {
    const STACK_SIZE: usize = 64 * 1024 * 1024;
    match std::thread::Builder::new().stack_size(STACK_SIZE).spawn(f) {
        Ok(handle) => handle.join().unwrap_or_default(),
        Err(_) => T::default(),
    }
}

pub const SIGNATURES_ONLY_HINT: &str = concat!(
    "Signatures/outline only — bodies and comments omitted; ",
    "the whole skeleton is returned in one response (never paginated). ",
    "Left gutter shows original line numbers; use startLine/endLine to read a body."
);

/// Returns `(1-based line number, text)` pairs for every line that starts a
/// top-level semantic block. Tree-sitter only — same dispatch as
/// `extract_signatures_inner` but skips the renderer, so callers can map line
/// numbers to char offsets without string parsing.
///
/// Returns an empty Vec for files above the 1 MB guard and any language without
/// a registered signature body query (there is no
/// regex/heuristic fallback — only real AST parsing produces boundaries).
pub fn extract_boundary_lines_inner(content: &str, file_path: &str) -> Vec<(usize, String)> {
    if content.len() > crate::minify::minifier::MAX_SIZE {
        return Vec::new();
    }
    // Wrap the tree-sitter parser path in `catch_unwind` so a parser panic on
    // adversarial input is converted into a clean empty fallback rather than
    // unwinding across the napi FFI boundary and aborting Node. Mirrors the
    // guard on the sibling `extract_signatures_inner`.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let ext = get_extension_internal(file_path, true, "txt");
        // Tree-sitter is the ONLY signature path. Grammars wired for structural
        // search only (empty body_query: markup, stylesheets, and data) and
        // languages with no grammar produce no boundaries.
        let Some(entry) = languages::find_entry(&ext).filter(|e| !e.body_query.is_empty()) else {
            return Vec::new();
        };
        let cfg = LangExtractConfig {
            language: entry.language.clone(),
            body_query: entry.body_query,
        };
        extract(content, &cfg).unwrap_or_default()
    }))
    .unwrap_or_default()
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
    let offset_table = crate::text::utf8_offsets::LineIndex::new(content);
    let offset_table = offset_table.line_starts_utf16();
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
    if content.len() > crate::minify::minifier::MAX_SIZE {
        return None;
    }
    let skeleton = std::panic::catch_unwind(|| {
        let ext = get_extension_internal(file_path, true, "txt");
        extract_by_ext(content, &ext)
    })
    .unwrap_or(None)?;

    // Return the source view when the rendered outline is no smaller. A file
    // with few body lines can grow once the outline includes line gutters.
    //
    // The comparison is on the rendered output the agent actually receives
    // (gutter included) — that is the byte count we promise never to inflate.
    // Tiny files where the per-line gutter alone tips the balance are suppressed
    // too, which is correct: a symbol outline of a handful of lines carries no
    // navigational value over just showing the lines.
    if skeleton.len() >= content.len() {
        return None;
    }
    Some(skeleton)
}

fn extract_by_ext(content: &str, ext: &str) -> Option<String> {
    // Tree-sitter is the ONLY signature path — real AST parsing, no regex
    // heuristics. Grammars wired for structural search only (empty body_query)
    // and any language without a grammar return
    // None, and the caller falls back to the standard/none view of the file.
    let entry = languages::find_entry(ext).filter(|e| !e.body_query.is_empty())?;
    let cfg = LangExtractConfig {
        language: entry.language.clone(),
        body_query: entry.body_query,
    };
    let kept = extract(content, &cfg)?;
    renderer::render_skeleton(&kept, entry.comment_style)
}

// ── Tests ────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    fn extract(content: &str, path: &str) -> Option<String> {
        extract_signatures_inner(content, path)
    }

    /// Regression: the tree-sitter boundary extractor must never abort the
    /// process on adversarial input — a parser panic must be caught and turned
    /// into an empty fallback by the `catch_unwind` guard on
    /// `extract_boundary_lines_inner`. We feed a barrage of malformed sources
    /// and assert only that each call returns without aborting.
    #[test]
    fn boundary_extractor_never_aborts_on_malformed_input() {
        let adversarial = [
            "",
            "\u{0}\u{0}\u{0}\u{0}",
            "function broken( { [ unterminated",
            "}}}};;;;export export export",
            "\u{feff}\u{202e}const x =;",
            "class { { { {",
            "import type type from from from",
        ];
        for src in adversarial {
            let lines = extract_boundary_lines_inner(src, "x.ts");
            // Reachable only if no abort occurred.
            let _ = lines.len();
            let offsets = get_semantic_boundary_offsets_inner(src, "x.tsx");
            let _ = offsets.len();
        }
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

    // Data / config / unsupported prose have no signature grammar.
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
            assert!(extract_boundary_lines_inner(content, path).is_empty());
            assert!(get_semantic_boundary_offsets_inner(content, path).is_empty());
        }
    }

    #[test]
    fn structural_only_grammars_never_produce_signature_views() {
        for entry in languages::all_entries()
            .iter()
            .filter(|entry| entry.body_query.is_empty())
        {
            for ext in entry.extensions {
                let path = format!("fixture.{ext}");
                let content = "function example() {\n  return value;\n}\n";
                assert!(extract_signatures_inner(content, &path).is_none(), ".{ext}");
                assert!(
                    extract_boundary_lines_inner(content, &path).is_empty(),
                    ".{ext}"
                );
            }
        }
    }

    #[test]
    fn unsupported_or_nonshrinking_outlines_return_none() {
        // Unsupported/structural-only languages have no outline. The tiny Lua
        // example also returns None because rendering would not shrink it.
        for (content, path) in &[
            ("local x = 1\nfunction f() return x end\n", "a.lua"),
            ("-module(d).\nrev(L) -> L.\n", "a.erl"),
            ("CREATE TABLE t (id INT);\n", "a.sql"),
            ("# Title\n\nText\n", "README.md"),
            ("type Query { user: User }\n", "schema.graphql"),
        ] {
            assert!(
                extract(content, path).is_none(),
                "{path}: no grammar or structural-only → must return None (no regex fallback)"
            );
        }
    }

    #[test]
    fn ruby_extracts_method_signatures() {
        let src =
            "def greet\n  puts 'hi'\nend\n\ndef farewell(name)\n  puts \"bye #{name}\"\nend\n";
        let s = extract(src, "a.rb").expect("ruby must extract");
        assert!(s.contains("def greet"), "method sig preserved");
        assert!(s.contains("def farewell"), "method sig preserved");
        assert!(!s.contains("puts"), "body dropped");
    }

    #[test]
    fn skeleton_never_grows_beyond_source() {
        // The anti-growth guard: a tree-sitter language whose file barely
        // compresses (a config-shaped `.cjs` that is one big object literal with
        // no function bodies to drop) must return None rather than an outline
        // that is not smaller than the source. Use a realistically sized input so
        // the verdict reflects real compression, not per-line gutter noise.
        let cjs = "module.exports = {\n".to_string()
            + &"  presets: [['@babel/preset-env', { targets: { node: 'current' } }]],\n  plugins: ['@babel/plugin-transform-runtime'],\n".repeat(40)
            + "};\n";
        if let Some(skeleton) = extract(&cjs, "babel.config.cjs") {
            assert!(
                skeleton.len() < cjs.len(),
                "skeleton ({} bytes) must be smaller than source ({} bytes)",
                skeleton.len(),
                cjs.len()
            );
        }
    }

    #[test]
    fn well_structured_code_still_gets_a_skeleton_after_guard() {
        // The guard is content-driven, not extension-driven: a body-heavy file is
        // dropped, but a signature-dense file that genuinely compresses survives.
        let src = "export function add(a: number, b: number): number {\n  const sum = a + b;\n  console.log(sum);\n  return sum;\n}\n\nexport function sub(a: number, b: number): number {\n  const diff = a - b;\n  console.log(diff);\n  return diff;\n}\n";
        let s = extract(src, "math.ts").expect("dense TS must still extract");
        assert!(s.len() < src.len(), "skeleton must compress");
        assert!(s.contains("add") && s.contains("sub"));
    }

    #[test]
    fn tree_sitter_code_extracts_and_drops_bodies() {
        // A body-bearing source in a tree-sitter language compresses and extracts.
        assert!(extract(
            "export function add(a: number, b: number): number {\n  const sum = a + b;\n  return sum;\n}\n",
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
            #[cfg(feature = "tree-sitter-c-sharp")]
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
        // ASCII-only: each char = 1 JS unit. Covered directly (and more
        // thoroughly, incl. surrogate pairs) by
        // text::utf8_offsets::tests::line_index_utf16_line_starts_*; this
        // test pins that `get_semantic_boundary_offsets_inner`'s call site
        // still gets the same table shape from the shared LineIndex.
        let src = "ab\ncd\n";
        let index = crate::text::utf8_offsets::LineIndex::new(src);
        // line 1: offset 0, line 2: offset 3 (a=1,b=1,\n=1), line 3: offset 6
        assert_eq!(index.line_starts_utf16(), &[0, 3, 6]);
    }
}
