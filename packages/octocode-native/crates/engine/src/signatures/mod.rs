mod deep_stack;

/// Largest source the parsers accept (declarations, graph facts, oxc,
/// syntax trees). Separate from the minifier's 1 MiB guard: parsing a real
/// monolithic source (TypeScript's 3 MB checker.ts) is cheap and bounded,
/// and the runtime tools share this bound.
pub const MAX_PARSE_SIZE: usize = 8 * 1024 * 1024;

pub mod extractor;
pub mod graph_facts;
pub mod js_oxc;
mod nodes;

pub(crate) use deep_stack::run_on_deep_stack;
mod js_oxc_calls;
mod js_oxc_commonjs;
mod js_oxc_receiver;
mod js_oxc_references;
mod js_oxc_shared;

pub(crate) const GRAPH_FACTS_SCHEMA_VERSION: u32 = 1;

/// An empty native syntax facts document for `file`, parsed as `language`.
fn native_graph_facts(language: String, file: &str) -> crate::graph::GraphFactsDocument {
    crate::graph::GraphFactsDocument {
        kind: "graphFacts".to_owned(),
        schema_version: GRAPH_FACTS_SCHEMA_VERSION,
        source: "native-ast".to_owned(),
        language,
        file: file.to_owned(),
        ..Default::default()
    }
}

pub(crate) struct GraphFactsExtraction {
    pub facts: crate::graph::GraphFactsDocument,
    /// One syntax-aware value-reference count per declaration id, so the
    /// dead-code analysis can tell value escapes from declarations and calls.
    pub reference_counts: Vec<crate::types::GraphReferenceCount>,
}

pub(crate) fn extract_graph_facts_with_metadata_inner(
    content: &str,
    file_path: &str,
) -> Option<GraphFactsExtraction> {
    js_oxc::extract_graph_facts_with_metadata(content, file_path)
        .or_else(|| tree_sitter_graph_facts(content, file_path))
}

/// Tree-sitter graph facts with the grammar chosen from the path, except that
/// Flow-typed JavaScript is read with the TSX grammar: the JS grammar recovers
/// Flow annotations as garbage declarations (`if` "functions").
fn tree_sitter_graph_facts(content: &str, file_path: &str) -> Option<GraphFactsExtraction> {
    let ext = get_extension_internal(file_path, true, "txt");
    let grammar = crate::text::file_extension::grammar_extension(content, &ext);
    graph_facts::extract_graph_facts_with_metadata_with_extension(content, file_path, grammar)
}

/// 0-based line where the comment block directly above a declaration
/// starting at `start` (0-based) begins, if any. Blank lines end the block;
/// Rust `#[...]` attributes between the comment and the item are skipped. `#`
/// is a comment only in Python (C/C++ `#include`/`#define` are code), and a
/// `*` continuation run must reach its `/*` opener.
pub(crate) fn leading_doc_line(lines: &[&str], start: usize, ext: &str) -> Option<u32> {
    let is_comment = |line: &str| {
        let line = line.trim_start();
        if ext == "py" {
            line.starts_with('#')
        } else {
            line.starts_with("//") || line.starts_with("/*") || line.starts_with('*')
        }
    };
    let mut top = start.min(lines.len());
    while ext == "rs" && top > 0 && lines[top - 1].trim_start().starts_with("#[") {
        top -= 1;
    }
    let mut doc = top;
    while doc > 0 && is_comment(lines[doc - 1]) {
        doc -= 1;
    }
    if doc == top || (ext != "py" && lines[doc].trim_start().starts_with('*')) {
        return None;
    }
    u32::try_from(doc).ok()
}

/// Declarations-only facts for outlines: the light oxc path for JS/TS; other
/// languages' single tree-sitter walk already costs about the same.
#[must_use]
pub fn extract_declarations(content: &str, file_path: &str) -> Option<String> {
    js_oxc::extract_declarations(content, file_path)
        .or_else(|| tree_sitter_graph_facts(content, file_path).map(|extraction| extraction.facts))
        .and_then(|facts| serde_json::to_string(&facts).ok())
}

#[must_use]
pub fn extract_graph_facts(content: &str, file_path: &str) -> Option<String> {
    extract_graph_facts_with_metadata_inner(content, file_path)
        .and_then(|extraction| serde_json::to_string(&extraction.facts).ok())
}

#[must_use]
pub fn extract_graph_facts_with_extension(
    content: &str,
    file_path: &str,
    extension: &str,
) -> Option<String> {
    graph_facts::extract_graph_facts_with_metadata_with_extension(content, file_path, extension)
        .and_then(|extraction| serde_json::to_string(&extraction.facts).ok())
}
pub mod languages;
pub mod renderer;

use crate::text::file_extension::get_extension_internal;
use extractor::LangExtractConfig;

/// Extract a structural skeleton from `content`.
/// Returns `NNN| text` rendered string or `None`.
pub fn extract_signatures_inner(content: &str, file_path: &str) -> Option<String> {
    if content.len() > MAX_PARSE_SIZE {
        return None;
    }
    let skeleton = std::panic::catch_unwind(|| {
        let ext = get_extension_internal(file_path, true, "txt");
        extract_by_ext(
            content,
            crate::text::file_extension::grammar_extension(content, &ext),
        )
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

    // A usable outline is scannable: one short signature per line. Minified or
    // bundled sources pack whole declarations onto a few enormous physical
    // lines, so per-line extraction keeps those lines verbatim and returns a
    // "skeleton" the agent cannot read. Detect that here and suppress it so the
    // caller falls back to the standard view (which can be range-read) instead
    // of promising an outline that is really raw minified bytes.
    if skeleton
        .lines()
        .any(|line| line.chars().count() > OUTLINE_MAX_LINE_CHARS)
    {
        return None;
    }
    Some(skeleton)
}

/// Longest rendered outline line (gutter included) still treated as a scannable
/// signature. Hand-written declaration lines effectively never reach this;
/// anything longer signals minified/bundled source with no navigational value.
const OUTLINE_MAX_LINE_CHARS: usize = 1000;

fn extract_by_ext(content: &str, ext: &str) -> Option<String> {
    // Tree-sitter is the only signature path — real AST parsing, no regex
    // heuristics. Languages outside the canonical registry return None and the
    // caller falls back to the standard/none view.
    let entry = languages::find_entry(ext)?;
    let cfg = LangExtractConfig {
        language: entry.language.clone(),
        body_query: entry.body_query,
    };
    let kept = extractor::extract_outline(content, &cfg)?;
    renderer::render_skeleton(&kept, entry.comment_style)
}

// ── Tests ────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    fn extract(content: &str, path: &str) -> Option<String> {
        extract_signatures_inner(content, path)
    }

    /// Gutters are unpadded so the content-view minifier (which trims
    /// leading whitespace on some lines) cannot leave them uneven.
    #[test]
    fn skeleton_gutters_are_uniform_after_minification() {
        let mut source = String::new();
        for i in 0..40 {
            source.push_str(&format!(
                "impl S{i} {{\n    pub fn run(&self) -> usize {{\n        let x = {i};\n        x + 1\n    }}\n}}\n\n"
            ));
        }
        let mut python = String::new();
        for i in 0..40 {
            python.push_str(&format!(
                "class C{i}:\n    def run(self):\n        x = {i}\n        y = x + 1\n        return y\n\n"
            ));
        }
        let mut views = Vec::new();
        for (path, text) in [("lib.rs", &source), ("mod.py", &python)] {
            let skeleton = extract(text, path).expect("skeleton");
            let minified =
                crate::minify::apply::apply_content_view_minification_inner(&skeleton, path);
            views.push(skeleton);
            views.push(minified);
        }
        for view in &views {
            for line in view.lines() {
                assert!(
                    line.split_once('\t')
                        .is_some_and(|(n, _)| n.parse::<usize>().is_ok()),
                    "{line:?} in\n{view}"
                );
            }
        }
    }

    /// Flow-typed JS in the shape of React's `ReactHooks.js`: `import type`,
    /// Flow function types (`S => S`) and annotated hooks.
    const FLOW_HOOKS: &str = r#"/**
 * @flow
 */

import type {Dispatcher} from 'react-reconciler/src/ReactInternalTypes';
import ReactSharedInternals from 'shared/ReactSharedInternals';

type BasicStateAction<S> = (S => S) | S;
type Dispatch<A> = A => void;

function resolveDispatcher() {
  const dispatcher = ReactSharedInternals.H;
  if (__DEV__) {
    if (dispatcher === null) {
      console.error('Invalid hook call.');
    }
  }
  return ((dispatcher: any): Dispatcher);
}

export function useState<S>(
  initialState: (() => S) | S,
): [S, Dispatch<BasicStateAction<S>>] {
  const dispatcher = resolveDispatcher();
  return dispatcher.useState(initialState);
}

export function useReducer<S, I, A>(
  reducer: (S, A) => S,
  initialArg: I,
  init?: I => S,
): [S, Dispatch<A>] {
  const dispatcher = resolveDispatcher();
  if (__DEV__) {
    console.log('reducer');
  }
  return dispatcher.useReducer(reducer, initialArg, init);
}

export function useRef<T>(initialValue: T): {current: T} {
  const dispatcher = resolveDispatcher();
  if (__DEV__) {
    console.log('ref');
  }
  return dispatcher.useRef(initialValue);
}

export function useEffect(
  create: () => (() => void) | void,
  deps: Array<mixed> | void | null,
): void {
  if (__DEV__) {
    if (create == null) {
      console.warn('React Hook useEffect requires an effect callback.');
    }
  }
  const dispatcher = resolveDispatcher();
  return dispatcher.useEffect(create, deps);
}
"#;

    fn declaration_names(raw: &str) -> Vec<String> {
        let facts: serde_json::Value = serde_json::from_str(raw).expect("facts JSON");
        facts["declarations"]
            .as_array()
            .expect("declarations")
            .iter()
            .filter_map(|d| d["name"].as_str().map(str::to_owned))
            .collect()
    }

    #[test]
    fn flow_js_declarations_list_the_exported_hooks_not_keywords() {
        let raw = extract_declarations(FLOW_HOOKS, "ReactHooks.js").expect("declarations");
        let names = declaration_names(&raw);
        for hook in [
            "useState",
            "useReducer",
            "useRef",
            "useEffect",
            "resolveDispatcher",
        ] {
            assert!(names.iter().any(|n| n == hook), "missing {hook}: {names:?}");
        }
        assert!(!names.iter().any(|n| n == "if"), "{names:?}");
    }

    #[test]
    fn js_grammar_recovery_never_names_a_declaration_after_a_keyword() {
        // The plain JS grammar cannot read Flow; whatever it recovers, a
        // statement keyword must never surface as a declaration name.
        let raw =
            extract_graph_facts_with_extension(FLOW_HOOKS, "ReactHooks.js", "js").expect("facts");
        let names = declaration_names(&raw);
        assert!(
            !names
                .iter()
                .any(|n| STATEMENT_KEYWORD_PROBE.contains(&n.as_str())),
            "{names:?}"
        );
    }
    const STATEMENT_KEYWORD_PROBE: &[&str] = &["if", "for", "while", "switch", "return"];

    #[test]
    fn flow_js_symbols_view_is_an_outline_not_a_body_dump() {
        let outline = extract(FLOW_HOOKS, "ReactHooks.js").expect("outline");
        assert!(outline.contains("export function useState"), "{outline}");
        assert!(
            !outline.contains("dispatcher.useReducer"),
            "bodies must be elided: {outline}"
        );
        assert!(outline.len() * 2 < FLOW_HOOKS.len(), "{outline}");
    }

    #[test]
    fn graph_fact_metadata_preserves_json_and_export_order_across_producers() {
        for (source, path, expected_names, producer_field) in [
            (
                "export function first() { return 1; } export const second = first();",
                "main.ts",
                vec!["first", "second"],
                "commonJs",
            ),
            (
                "pub fn first() {} pub struct Second; fn private() {}",
                "lib.rs",
                vec!["first", "Second", "private"],
                "modules",
            ),
        ] {
            let extraction = extract_graph_facts_with_metadata_inner(source, path)
                .expect("graph facts with metadata");
            let counted: Vec<&str> = extraction
                .reference_counts
                .iter()
                .map(|count| {
                    count
                        .declaration_id
                        .split(['#', '@'])
                        .nth(1)
                        .unwrap_or_default()
                })
                .collect();
            assert_eq!(counted, expected_names);
            let facts_json = serde_json::to_string(&extraction.facts).expect("facts JSON");
            assert_eq!(
                facts_json,
                extract_graph_facts(source, path).expect("legacy JSON facts")
            );
            let json: serde_json::Value =
                serde_json::from_str(&facts_json).expect("valid facts JSON");
            assert!(json.get(producer_field).is_some());
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
        }
    }

    #[test]
    fn unsupported_or_nonshrinking_outlines_return_none() {
        // Languages outside the first-class registry have no outline.
        for (content, path) in &[
            ("local x = 1\nfunction f() return x end\n", "a.lua"),
            ("-module(d).\nrev(L) -> L.\n", "a.erl"),
            ("CREATE TABLE t (id INT);\n", "a.sql"),
            ("# Title\n\nText\n", "README.md"),
            ("type Query { user: User }\n", "schema.graphql"),
        ] {
            assert!(
                extract(content, path).is_none(),
                "{path}: no first-class grammar → must return None (no regex fallback)"
            );
        }
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

    /// Rust declaration extents (0-based lines) that block reads and
    /// readBlock leads widen to: whole fns, impl blocks, and their methods.
    #[test]
    fn rust_declaration_ranges_cover_whole_items() {
        let source = "pub fn first(value: usize) -> usize {\n    let mut total = 0;\n    for step in 0..value {\n        total += step;\n    }\n    total\n}\n\npub struct Point;\n\nimpl Point {\n    pub fn origin() -> Self {\n        Point\n    }\n}\n";
        let raw = extract_declarations(source, "src/lib.rs").expect("declarations");
        let facts: serde_json::Value = serde_json::from_str(&raw).expect("json");
        let spans: Vec<(String, u64, u64)> = facts["declarations"]
            .as_array()
            .expect("declarations")
            .iter()
            .map(|declaration| {
                (
                    declaration["name"].as_str().unwrap_or_default().to_owned(),
                    declaration["range"]["start"]["line"]
                        .as_u64()
                        .unwrap_or(u64::MAX),
                    declaration["range"]["end"]["line"]
                        .as_u64()
                        .unwrap_or(u64::MAX),
                )
            })
            .collect();
        for expected in [("first", 0, 6), ("origin", 11, 13)] {
            assert!(
                spans
                    .iter()
                    .any(|(name, start, end)| (name.as_str(), *start, *end) == expected),
                "{expected:?} in {spans:?}"
            );
        }
        assert!(
            spans
                .iter()
                .any(|(_, start, end)| (*start, *end) == (10, 14)),
            "impl block 10-14 in {spans:?}"
        );
        let mut body = String::from(
            "pub fn first(value: usize) -> usize {
",
        );
        for line in 1..=18 {
            body.push_str(&format!(
                "    let step{line} = value;
"
            ));
        }
        body.push_str(
            "    value
}

pub fn second() -> usize {
    let short = 1;
    short
}
",
        );
        let raw = extract_declarations(&body, "src/lib.rs").expect("declarations");
        let facts: serde_json::Value = serde_json::from_str(&raw).expect("json");
        let ranges: Vec<(u64, u64)> = facts["declarations"]
            .as_array()
            .expect("declarations")
            .iter()
            .map(|d| {
                (
                    d["range"]["start"]["line"].as_u64().unwrap_or(u64::MAX),
                    d["range"]["end"]["line"].as_u64().unwrap_or(u64::MAX),
                )
            })
            .collect();
        assert_eq!(ranges, vec![(0, 20), (22, 25)], "{raw}");
    }
}
