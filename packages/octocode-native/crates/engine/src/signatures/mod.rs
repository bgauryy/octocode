/// Largest source the parsers accept (declarations, graph facts, oxc,
/// syntax trees). Separate from the minifier's 1 MiB guard: parsing a real
/// monolithic source (TypeScript's 3 MB checker.ts) is cheap and bounded,
/// and the runtime tools share this bound.
pub const MAX_PARSE_SIZE: usize = 8 * 1024 * 1024;

pub(crate) mod extractor;
pub mod graph_facts;
pub(crate) mod js_oxc;
mod nodes;

mod js_oxc_calls;
mod js_oxc_commonjs;
mod js_oxc_receiver;
mod js_oxc_references;
mod js_oxc_shared;

/// v2: diagnostics are typed records (`code`, `message`, `line`), not text.
pub const GRAPH_FACTS_SCHEMA_VERSION: u32 = 2;

/// The declaration kinds an `extends`/`implements` clause can name or
/// declare: the nodes of a type hierarchy. One list for the graph's linking
/// and the hierarchy walks.
pub const TYPE_DECLARATION_KINDS: &[&str] =
    &["class", "interface", "struct", "trait", "type", "enum"];

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
    let ext = extension_of(file_path, true, "txt");
    let grammar = crate::text::file_extension::grammar_extension(content, &ext);
    graph_facts::extract_graph_facts_with_metadata_with_extension(content, file_path, grammar)
}

/// 0-based line where the comment block directly above a declaration
/// starting at `start` (0-based) begins, if any. Blank lines end the block;
/// Rust `#[...]` attributes between the comment and the item are skipped.
/// Comments follow the grammar's [`CommentStyle`] (so C/C++ `#include` and
/// `#define` are code), and a `*` continuation run must reach its `/*` opener.
pub(crate) fn leading_doc_line(lines: &[&str], start: usize, ext: &str) -> Option<u32> {
    let style = languages::find_entry(ext).map_or(CommentStyle::C, |entry| entry.comment_style);
    let is_comment = |line: &str| renderer::is_pure_comment(line, style);
    let mut top = start.min(lines.len());
    while ext == "rs" && top > 0 && lines[top - 1].trim_start().starts_with("#[") {
        top -= 1;
    }
    let mut doc = top;
    while doc > 0 && is_comment(lines[doc - 1]) {
        doc -= 1;
    }
    // A block's continuation line alone (` * …`) is not where it starts.
    if doc == top || (style != CommentStyle::Hash && lines[doc].trim_start().starts_with('*')) {
        return None;
    }
    u32::try_from(doc).ok()
}

/// Declarations-only facts for outlines: the light oxc path for JS/TS; other
/// languages' single tree-sitter walk already costs about the same.
#[must_use]
pub fn extract_declarations_document(
    content: &str,
    file_path: &str,
) -> Option<crate::graph::GraphFactsDocument> {
    js_oxc::extract_declarations(content, file_path)
        .or_else(|| tree_sitter_declarations(content, file_path))
}

/// [`extract_declarations_document`] parsed with the grammar of `extension`
/// (a `language` override) instead of the path's own: declarations only, no
/// call or path facts.
#[must_use]
pub fn extract_declarations_document_with_extension(
    content: &str,
    file_path: &str,
    extension: &str,
) -> Option<crate::graph::GraphFactsDocument> {
    graph_facts::extract_declaration_facts_with_extension(content, file_path, extension)
}

/// The symbols outline of `content` ([`extract_signatures_inner`]) and its
/// declarations-only facts ([`extract_declarations_document`]) from ONE
/// tree-sitter parse wherever both read the same grammar; JS/TS
/// declarations come from oxc, the outline from tree-sitter. Runs on the
/// shared deep parser stack.
pub fn extract_signatures_with_declarations(
    content: &str,
    file_path: &str,
) -> (
    Result<String, OutlineUnavailable>,
    Option<crate::graph::GraphFactsDocument>,
) {
    let (content, file_path) = (content.to_owned(), file_path.to_owned());
    crate::deep_stack::run_on_deep_stack(move || {
        Some(signatures_with_declarations(&content, &file_path))
    })
    .unwrap_or((Err(OutlineUnavailable::ParseFailed), None))
}

fn signatures_with_declarations(
    content: &str,
    file_path: &str,
) -> (
    Result<String, OutlineUnavailable>,
    Option<crate::graph::GraphFactsDocument>,
) {
    if content.len() > MAX_PARSE_SIZE {
        return (Err(OutlineUnavailable::TooLarge), None);
    }
    if let Some(declarations) = js_oxc::extract_declarations(content, file_path) {
        return (
            extract_signatures_inner(content, file_path),
            Some(declarations),
        );
    }
    let shared = std::panic::catch_unwind(|| {
        let ext = extension_of(file_path, true, "txt");
        let grammar = crate::text::file_extension::grammar_extension(content, &ext);
        let entry = languages::find_entry(grammar).ok_or(OutlineUnavailable::UnsupportedLanguage);
        // Each reader keeps the deadline it has alone; only the parse is shared.
        let deadline = || std::time::Instant::now() + extractor::AST_EXECUTION_TIMEOUT;
        let tree = entry
            .ok()
            .and_then(|entry| extractor::parse_before(content, &entry.language, deadline()));
        let declarations = graph_facts::declaration_facts_from_tree(
            content,
            file_path,
            grammar,
            tree.as_ref(),
            deadline(),
        );
        let skeleton = entry.and_then(|entry| {
            let cfg = LangExtractConfig {
                language: entry.language.clone(),
                body_query: entry.body_query,
            };
            if entry.body_query.is_empty() {
                return Err(OutlineUnavailable::UnsupportedLanguage);
            }
            let tree = tree.as_ref().ok_or(OutlineUnavailable::ParseFailed)?;
            let kept = extractor::outline_from_tree(content, &cfg, tree, deadline())
                .ok_or(OutlineUnavailable::ParseFailed)?;
            renderer::render_skeleton(&kept, entry.comment_style)
                .ok_or(OutlineUnavailable::NoDeclarations)
        });
        (skeleton, declarations)
    });
    match shared {
        Ok((skeleton, declarations)) => (
            skeleton.and_then(|skeleton| usable_outline(content, skeleton)),
            declarations,
        ),
        Err(_) => (Err(OutlineUnavailable::ParseFailed), None),
    }
}

/// [`tree_sitter_graph_facts`] without call facts (see
/// `graph_facts::extract_declaration_facts_with_extension`).
fn tree_sitter_declarations(
    content: &str,
    file_path: &str,
) -> Option<crate::graph::GraphFactsDocument> {
    let ext = extension_of(file_path, true, "txt");
    let grammar = crate::text::file_extension::grammar_extension(content, &ext);
    graph_facts::extract_declaration_facts_with_extension(content, file_path, grammar)
}

#[must_use]
pub fn extract_graph_facts(content: &str, file_path: &str) -> Option<String> {
    extract_graph_facts_with_metadata_inner(content, file_path)
        .and_then(|extraction| serde_json::to_string(&extraction.facts).ok())
}

pub mod languages;
pub(crate) mod renderer;

/// Every declaration kind the extractors emit: tree-sitter graph facts
/// (`nodes::declaration_kind` and `nodes::declaration`) and the JS/TS oxc
/// extractor (`js_oxc::symbol_kind_name`).
pub const DECLARATION_KINDS: &[&str] = &[
    "class",
    "constant",
    "constructor",
    "enum",
    "enumMember",
    "function",
    "impl",
    "interface",
    "label",
    "macro",
    "method",
    "module",
    "namespace",
    "property",
    "struct",
    "symbol",
    "trait",
    "type",
    "variable",
];

use crate::text::file_extension::extension_of;
use extractor::LangExtractConfig;
use languages::CommentStyle;

/// Why a file has no symbols outline: the typed reason a reader falls back
/// to another view.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum OutlineUnavailable {
    /// Larger than [`MAX_PARSE_SIZE`]: not parsed.
    TooLarge,
    /// No grammar with an outline for this file type.
    UnsupportedLanguage,
    /// The parser gave no tree in time.
    ParseFailed,
    /// The parse kept no declaration line.
    NoDeclarations,
    /// The outline would be no smaller than the source.
    NotSmaller,
    /// Over-long lines (minified or bundled source) make up most of the
    /// outline, so it is not a scannable list of signatures.
    Minified,
}

impl OutlineUnavailable {
    /// The reason as the readers report it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TooLarge => "too-large",
            Self::UnsupportedLanguage => "unsupported-language",
            Self::ParseFailed => "parse-failed",
            Self::NoDeclarations => "no-declarations",
            Self::NotSmaller => "not-smaller",
            Self::Minified => "minified-lines",
        }
    }
}

/// Extract a structural skeleton from `content`: `N\ttext` lines, or the
/// typed reason there is none.
pub(crate) fn extract_signatures_inner(
    content: &str,
    file_path: &str,
) -> Result<String, OutlineUnavailable> {
    if content.len() > MAX_PARSE_SIZE {
        return Err(OutlineUnavailable::TooLarge);
    }
    let skeleton = std::panic::catch_unwind(|| {
        let ext = extension_of(file_path, true, "txt");
        extract_by_ext(
            content,
            crate::text::file_extension::grammar_extension(content, &ext),
        )
    })
    .unwrap_or(Err(OutlineUnavailable::ParseFailed))?;
    usable_outline(content, skeleton)
}

/// `skeleton` as the outline of `content`, unless it is no smaller than the
/// source or mostly over-long lines.
fn usable_outline(content: &str, skeleton: String) -> Result<String, OutlineUnavailable> {
    // Return the source view when the rendered outline is no smaller. A file
    // with few body lines can grow once the outline includes line gutters.
    //
    // The comparison is on the rendered output the agent actually receives
    // (gutter included) — that is the byte count we promise never to inflate.
    // Tiny files where the per-line gutter alone tips the balance are suppressed
    // too, which is correct: a symbol outline of a handful of lines carries no
    // navigational value over just showing the lines.
    if skeleton.len() >= content.len() {
        return Err(OutlineUnavailable::NotSmaller);
    }

    // A usable outline is scannable: mostly one short signature per line.
    // Minified or bundled sources pack whole declarations onto a few enormous
    // physical lines, so per-line extraction keeps those lines verbatim and
    // the "outline" is really raw minified bytes. That holds when over-long
    // lines make up most of the outline; a few long declarations (generated
    // one-line structs) in an otherwise normal outline stay, verbatim, and
    // their range gutter makes each body one `ranges` read away.
    let long_chars: usize = skeleton
        .lines()
        .map(|line| line.chars().count())
        .filter(|chars| *chars > OUTLINE_MAX_LINE_CHARS)
        .sum();
    if long_chars.saturating_mul(2) > skeleton.chars().count() {
        return Err(OutlineUnavailable::Minified);
    }
    Ok(skeleton)
}

/// Whether `line` is a whole-line comment in the comment syntax of the
/// grammar that reads `file_path`; `None` when no grammar reads it.
#[must_use]
pub fn is_comment_line(file_path: &str, line: &str) -> Option<bool> {
    let ext = extension_of(file_path, true, "txt");
    let entry = languages::find_entry(&ext)?;
    Some(renderer::is_pure_comment(line, entry.comment_style))
}

/// Longest rendered outline line (gutter included) still treated as a scannable
/// signature. Hand-written declaration lines effectively never reach this;
/// an outline made mostly of longer lines is minified/bundled source.
const OUTLINE_MAX_LINE_CHARS: usize = 1000;

fn extract_by_ext(content: &str, ext: &str) -> Result<String, OutlineUnavailable> {
    // Tree-sitter is the only signature path — real AST parsing, no regex
    // heuristics. Languages outside the canonical registry have no outline
    // and the caller falls back to the standard/none view.
    let entry = languages::find_entry(ext)
        .filter(|entry| !entry.body_query.is_empty())
        .ok_or(OutlineUnavailable::UnsupportedLanguage)?;
    let cfg = LangExtractConfig {
        language: entry.language.clone(),
        body_query: entry.body_query,
    };
    let kept = extractor::extract_outline(content, &cfg).ok_or(OutlineUnavailable::ParseFailed)?;
    renderer::render_skeleton(&kept, entry.comment_style).ok_or(OutlineUnavailable::NoDeclarations)
}

// ── Tests ────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    fn extract_graph_facts_with_extension(
        content: &str,
        file_path: &str,
        extension: &str,
    ) -> Option<String> {
        graph_facts::extract_graph_facts_with_metadata_with_extension(content, file_path, extension)
            .and_then(|extraction| serde_json::to_string(&extraction.facts).ok())
    }

    /// The declarations-only extraction (outlines, enclosing names) skips
    /// call facts, reference counts and import uses, and keeps every other
    /// fact identical to the full graph extraction, in every grammar.
    /// A `language` override outlines declarations only: the same
    /// declarations as the full facts under that grammar, and no call facts.
    #[test]
    fn signatures_with_declarations_equal_the_two_separate_extractions() {
        for (path, source) in [
            (
                "lib.rs",
                "use std::fmt;\n\npub struct Point { x: f64 }\n\nimpl Point {\n    pub fn norm(&self) -> f64 {\n        let squared = self.x * self.x;\n        squared.sqrt()\n    }\n}\n\nfn helper() -> u8 {\n    let value = 1;\n    value + 1\n}\n",
            ),
            (
                "app.py",
                "import os\n\nclass Box:\n    def size(self):\n        total = 0\n        for item in self.items:\n            total += 1\n        return total\n",
            ),
            (
                "main.ts",
                "export function run(value: number): number {\n  const doubled = value * 2;\n  return doubled + 1;\n}\nexport class Widget {\n  render() {\n    return 1;\n  }\n}\n",
            ),
            ("notes.txt", "plain text\n"),
        ] {
            let (outline, declarations) = extract_signatures_with_declarations(source, path);
            let alone = extract_signatures_inner(source, path);
            assert_eq!(outline, alone, "{path}");
            assert_eq!(
                declarations.and_then(|facts| serde_json::to_value(facts).ok()),
                extract_declarations_document(source, path)
                    .and_then(|facts| serde_json::to_value(facts).ok()),
                "{path}"
            );
        }
    }

    #[test]
    fn an_override_outlines_declarations_without_call_facts() {
        let source = "fn make() -> u8 { helper(); 1 }\nfn helper() {}\n";
        let lean = serde_json::to_value(
            extract_declarations_document_with_extension(source, "script", "rs").expect("facts"),
        )
        .expect("json");
        let full: serde_json::Value = serde_json::from_str(
            &extract_graph_facts_with_extension(source, "script", "rs").expect("facts"),
        )
        .expect("json");
        assert_eq!(lean["declarations"], full["declarations"]);
        assert!(!full["calls"].as_array().expect("calls").is_empty());
        assert!(lean["calls"].as_array().is_none_or(Vec::is_empty), "{lean}");
    }

    #[test]
    fn declaration_facts_equal_the_graph_facts_without_calls() {
        let cases = [
            (
                "src/lib.rs",
                "mod m {\n    pub struct S;\n    impl Clone for S { fn clone(&self) -> Self { make(S) } }\n}\nuse std::fmt;\n/// doc\npub fn make(s: m::S) -> m::S { helper(); s }\nfn helper() {}\nmacro_rules! mk { () => {} }\n",
            ),
            (
                "app.py",
                "import os\nclass A(Base):\n    def run(self):\n        return os.path.join(self.x)\n\ndef top():\n    A().run()\n",
            ),
            (
                "main.go",
                "package main\nimport \"fmt\"\ntype T struct{}\nfunc (t T) Run() { fmt.Println(1) }\nfunc main() { T{}.Run() }\n",
            ),
            (
                "A.java",
                "class A extends B implements C { void run() { go(); } void go() {} }\n",
            ),
            (
                "x.c",
                "#include <stdio.h>\nstatic int add(int a, int b) { return a + b; }\nint main(void) { printf(\"%d\", add(1, 2)); }\n",
            ),
            ("large.rs", include_str!("graph_facts/mod.rs")),
        ];
        for (path, source) in cases {
            let full = tree_sitter_graph_facts(source, path).expect(path).facts;
            let lean = tree_sitter_declarations(source, path).expect(path);
            assert!(!lean.declarations.is_empty(), "{path}");
            assert!(lean.calls.is_empty(), "{path}");
            let json = |value: &crate::graph::GraphFactsDocument| {
                serde_json::to_value(value).expect("json")
            };
            let mut expected = json(&full);
            expected["calls"] = serde_json::json!([]);
            expected.as_object_mut().map(|facts| facts.remove("paths"));
            if let Some(edges) = expected["edges"].as_array_mut() {
                edges.retain(|edge| edge["relation"] != "calls");
            }
            // An edge id ends in its push position, which skipped call
            // edges shift; consumers read from/relation/to.
            let mut got = json(&lean);
            for facts in [&mut got, &mut expected] {
                for edge in facts["edges"].as_array_mut().into_iter().flatten() {
                    edge.as_object_mut().map(|edge| edge.remove("id"));
                }
            }
            assert_eq!(got, expected, "{path}");
        }
    }

    fn extract(content: &str, path: &str) -> Option<String> {
        extract_signatures_inner(content, path).ok()
    }

    /// A few over-long declaration lines (generated one-line structs) keep
    /// the outline, verbatim; an outline made mostly of them is minified.
    #[test]
    fn long_declaration_lines_keep_the_outline_unless_they_dominate_it() {
        let mut source = String::new();
        for i in 0..200 {
            source.push_str(&format!(
                "pub fn run{i}(value: usize) -> usize {{\n    let doubled = value * 2;\n    doubled + {i}\n}}\n\n"
            ));
        }
        let fields = (0..120)
            .map(|i| format!("pub field_{i}: usize"))
            .collect::<Vec<_>>()
            .join(", ");
        let long = format!("pub struct Generated {{ {fields} }}");
        assert!(long.len() > OUTLINE_MAX_LINE_CHARS);
        source.push_str(&long);
        source.push('\n');
        let outline = extract_signatures_inner(&source, "lib.rs").expect("outline");
        assert!(
            outline.contains(&long),
            "the long declaration stays verbatim"
        );
        assert!(outline.contains("pub fn run199(value: usize) -> usize"));

        let minified = format!("{long}\n{long}\npub fn tail() {{\n    let x = 1;\n}}\n");
        assert_eq!(
            extract_signatures_inner(&minified, "lib.rs"),
            Err(OutlineUnavailable::Minified)
        );
    }

    /// Comment lines follow the file's own grammar: `#` comments Python,
    /// not Rust; `--` comments neither.
    #[test]
    fn comment_lines_follow_the_files_grammar() {
        assert_eq!(is_comment_line("a.py", "# note"), Some(true));
        assert_eq!(is_comment_line("a.rs", "# note"), Some(false));
        assert_eq!(is_comment_line("a.rs", "/// doc"), Some(true));
        assert_eq!(is_comment_line("a.rs", "-- x"), Some(false));
        assert_eq!(is_comment_line("notes.txt", "// x"), None);
    }

    /// Each missing outline names its own reason.
    #[test]
    fn a_missing_outline_names_its_reason() {
        assert_eq!(
            extract_signatures_inner("# Title\n\nText\n", "README.md"),
            Err(OutlineUnavailable::UnsupportedLanguage)
        );
        assert_eq!(
            extract_signatures_inner(&"x".repeat(MAX_PARSE_SIZE + 1), "big.rs"),
            Err(OutlineUnavailable::TooLarge)
        );
        assert_eq!(
            extract_signatures_inner("fn a() {}\n", "tiny.rs"),
            Err(OutlineUnavailable::NotSmaller)
        );
        assert_eq!(OutlineUnavailable::Minified.as_str(), "minified-lines");
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
        let names = extract_declarations_document(FLOW_HOOKS, "ReactHooks.js")
            .expect("declarations")
            .declarations
            .into_iter()
            .map(|declaration| declaration.name)
            .collect::<Vec<_>>();
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
    /// Each tree-sitter skeleton keeps declarations and drops bodies.
    #[test]
    fn tree_sitter_skeletons_keep_signatures_and_drop_bodies() {
        let cases: [(&str, &str, &[&str], &[&str]); 6] = [
            (
                "calc.ts",
                "\nexport function add(a: number, b: number): number {\n  return a + b;\n}\n\nexport class Calc {\n  value: number = 0;\n  multiply(x: number): number {\n    return this.value * x;\n  }\n}\n",
                // function, class, field, method signature
                &["add", "Calc", "value", "multiply"],
                &["return a + b", "this.value * x"],
            ),
            (
                "foo.py",
                "\nimport os\n\nclass Foo:\n    name: str\n\n    def bar(self, x: int) -> str:\n        return str(x)\n\ndef top_level():\n    pass\n",
                &["import os", "class Foo", "def bar", "def top_level"],
                &["return str", "pass"],
            ),
            (
                "geo.rs",
                "\npub fn greet(name: &str) -> String {\n    format!(\"Hello, {}\", name)\n}\n\npub struct Point { x: f64, y: f64 }\n\nimpl Point {\n    pub fn distance(&self, other: &Point) -> f64 {\n        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()\n    }\n}\n",
                &["greet"],
                &["format!"],
            ),
            (
                "main.go",
                "\npackage main\n\nimport \"fmt\"\n\nfunc Add(a, b int) int {\n    return a + b\n}\n\ntype Server struct {\n    Port int\n}\n\nfunc (s *Server) Start() error {\n    fmt.Println(\"starting\")\n    return nil\n}\n",
                &["Add"],
                &["Println"],
            ),
            (
                "Calculator.java",
                "\npublic class Calculator {\n    private int value;\n\n    public Calculator(int initial) {\n        this.value = initial;\n    }\n\n    public int add(int x) {\n        return value + x;\n    }\n}\n",
                &["Calculator", "add"],
                &["return value"],
            ),
            (
                "math.c",
                "\n#include <stdio.h>\n\nint add(int a, int b) {\n    return a + b;\n}\n\nvoid greet(const char *name) {\n    printf(\"Hello, %s\\n\", name);\n}\n",
                &["add"],
                &["printf"],
            ),
        ];
        for (file, src, kept, dropped) in cases {
            let s = extract(src, file).unwrap_or_else(|| panic!("{file} must extract"));
            for text in kept {
                assert!(s.contains(text), "{file}: {text} preserved: '{s}'");
            }
            for text in dropped {
                assert!(!s.contains(text), "{file}: body {text} dropped: '{s}'");
            }
        }
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

    /// Rust declaration extents (0-based lines) that block reads
    /// widen to: whole fns, impl blocks, and their methods.
    #[test]
    fn rust_declaration_ranges_cover_whole_items() {
        let source = "pub fn first(value: usize) -> usize {\n    let mut total = 0;\n    for step in 0..value {\n        total += step;\n    }\n    total\n}\n\npub struct Point;\n\nimpl Point {\n    pub fn origin() -> Self {\n        Point\n    }\n}\n";
        let spans: Vec<(String, u32, u32)> = extract_declarations_document(source, "src/lib.rs")
            .expect("declarations")
            .declarations
            .into_iter()
            .map(|declaration| {
                (
                    declaration.name,
                    declaration.range.start.line,
                    declaration.range.end.line,
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
        let facts = extract_declarations_document(&body, "src/lib.rs").expect("declarations");
        let ranges: Vec<(u32, u32)> = facts
            .declarations
            .iter()
            .map(|d| (d.range.start.line, d.range.end.line))
            .collect();
        assert_eq!(ranges, vec![(0, 20), (22, 25)], "{facts:?}");
    }

    /// `(name, 0-based first line, 0-based last line, 0-based name line, doc line)`
    /// of every declaration of `source`.
    fn extents(source: &str, path: &str) -> Vec<(String, u32, u32, u32, Option<u32>)> {
        extract_declarations_document(source, path)
            .expect("declarations")
            .declarations
            .into_iter()
            .map(|d| {
                (
                    d.name,
                    d.range.start.line,
                    d.range.end.line,
                    d.selection_range.start.line,
                    d.doc_line,
                )
            })
            .collect()
    }

    /// A declaration wrapped by its header (`template <…>` in C++ and CUDA, the
    /// `@decorator` lines in Python) spans the header: its range starts at the
    /// header and its doc comment is the one above it, while the selection
    /// stays on the name.
    #[test]
    fn wrapped_declaration_ranges_include_their_header() {
        let template = "// doc
template <typename T>
__global__ void
k(T x) {
}

template <typename T>
struct S {
  template <typename U>
  void m(U u) {}
};

void plain() {}
";
        for path in ["a.cu", "a.cpp"] {
            let found = extents(template, path);
            let of = |name: &str| found.iter().find(|d| d.0 == name).expect(name).clone();
            let k = of("k");
            assert_eq!(
                (k.1, k.2, k.3, k.4),
                (1, 4, 3, Some(0)),
                "{path}: {found:?}"
            );
            assert_eq!(of("S").1, 6, "{path}: {found:?}");
            assert_eq!((of("m").1, of("m").3), (8, 9), "{path}: {found:?}");
            assert_eq!(of("plain").1, 12, "{path}: {found:?}");
        }
        let python = "# doc
@dec
@dec2(1)
def foo():
    pass

class A:
    @property
    def b(self):
        return 1
";
        let found = extents(python, "a.py");
        let of = |name: &str| found.iter().find(|d| d.0 == name).expect(name).clone();
        assert_eq!(
            (of("foo").1, of("foo").3, of("foo").4),
            (1, 3, Some(0)),
            "{found:?}"
        );
        assert_eq!((of("b").1, of("b").3), (7, 8), "{found:?}");
    }
}
