use super::*;
use std::collections::HashSet;
use tree_sitter::{Parser, Query};

#[test]
fn excluded_grammars_report_unsupported_across_native_capabilities() {
    for ext in [
        "sh", "bash", "zsh", "vue", "svelte", "astro", "dart", "less", "ml", "mli", "jl", "r",
        "erl", "hrl", "ex", "exs", "tf", "hcl", "tfvars", "proto", "toml", "lua", "zig", "rb",
        "rake", "gemspec", "ru", "scss", "sql", "swift", "yaml", "yml", "css", "htm", "html",
        "json", "jsonc", "kt", "kts", "php",
    ] {
        assert!(find_entry(ext).is_none(), ".{ext} must not load a grammar");
        assert!(!supported_extensions().contains(&ext));
        assert!(!supported_extensions().contains(&ext));
        let file = format!("fixture.{ext}");
        assert!(crate::lsp::grammar::grammar_for_file(&file).is_none());
        let result = crate::structural::search_detailed(
            "target(value);",
            &file,
            ext,
            Some("target($ARG)"),
            None,
        );
        assert_eq!(result.status, "unsupported", ".{ext}");
        assert_eq!(
            result.diagnostics[0].code,
            "structural.language.unsupported"
        );
    }
}

#[test]
fn configured_capabilities_match_the_exact_first_class_extension_set() {
    let mut expected = vec![
        "c", "cjs", "cts", "go", "h", "java", "js", "jsx", "mjs", "mts", "py", "pyi", "rs", "ts",
        "tsx",
    ];
    if cfg!(feature = "tree-sitter-asm") {
        expected.extend(["asm", "assembly", "s"]);
    }
    if cfg!(feature = "tree-sitter-cpp") {
        expected.extend(["cc", "cpp", "cxx", "hh", "hpp", "hxx"]);
    }
    if cfg!(feature = "tree-sitter-c-sharp") {
        expected.push("cs");
    }
    if cfg!(feature = "tree-sitter-cuda") {
        expected.extend(["cu", "cuh"]);
    }
    if cfg!(feature = "tree-sitter-scala") {
        expected.extend(["sbt", "sc", "scala"]);
    }
    expected.sort_unstable();
    for (name, mut actual) in [
        (
            "structural",
            supported_extensions()
                .into_iter()
                .map(str::to_owned)
                .collect(),
        ),
        (
            "signature",
            supported_extensions()
                .into_iter()
                .map(str::to_owned)
                .collect(),
        ),
        (
            "graph",
            crate::signatures::graph_facts::graph_fact_extensions(),
        ),
    ] {
        actual.sort_unstable();
        assert_eq!(actual, expected, "{name} capability drift");
    }
}

#[test]
fn removed_languages_have_no_analysis_minifier_or_builtin_server_route() {
    for ext in ["toml", "lua", "zig"] {
        let file = format!("fixture.{ext}");
        assert!(crate::signatures::extract_signatures_inner("target(value);", &file).is_none());
        assert!(crate::signatures::extract_graph_facts("target(value);", &file).is_none());
        assert!(
            !crate::signatures::graph_facts::graph_fact_extensions()
                .iter()
                .any(|item| item == ext)
        );
        assert!(!crate::minify::config::minify_config().contains_key(ext));
        assert!(crate::lsp::config::detect_language_id(&file).is_none());
    }
    assert!(crate::minify::comment_remover::rules_for("lua").is_none());
}

#[test]
fn modern_language_constructs_parse_without_errors() {
    let cases = [
        (
            "ts",
            "const options = { mode: 'fast' } satisfies Record<string, string>;",
        ),
        (
            "tsx",
            "const View = () => <section>{items?.map(item => <span key={item.id}>{item.name}</span>)}</section>;",
        ),
        (
            "py",
            "type Vector[T] = list[T]\ndef first[T](values: Vector[T]) -> T:\n    return values[0]\n",
        ),
        (
            "go",
            "package main\nfunc First[T any](values []T) T { return values[0] }\n",
        ),
        (
            "rs",
            "fn main() { if let Some(x) = Some(1) && x > 0 { println!(\"{x}\"); } }",
        ),
        (
            "java",
            "record Point(int x, int y) { int sum() { return switch (x) { case 0 -> y; default -> x + y; }; } }",
        ),
        (
            "cs",
            "class Point(int x, int y) { public int[] Values => [x, y]; }",
        ),
        (
            "cpp",
            "template<typename T> concept Number = requires(T x) { x + x; };\ntemplate<Number T> T add(T x) { return x + x; }",
        ),
        (
            "cu",
            "__global__ void kernel(int *values) { values[threadIdx.x] += 1; }\nvoid launch(int *values) { kernel<<<1, 32>>>(values); }\n",
        ),
        ("asm", "target:\n  mov %rax, %rbx\n  ret\n"),
    ];
    for (ext, source) in cases {
        let Some(entry) = find_entry(ext) else {
            continue; // Optional grammar compiled out in a minimal build.
        };
        let mut parser = Parser::new();
        parser.set_language(&entry.language).expect("grammar ABI");
        let tree = parser.parse(source, None).expect("modern syntax tree");
        assert!(
            !tree.root_node().has_error(),
            ".{ext}: {}",
            tree.root_node().to_sexp()
        );
    }
}

// One valid source per grammar. Every alias is exercised against its grammar's
// fixture; adding a registry entry without a fixture fails instead of silently
// leaving a newly advertised language untested.
fn fixture(ext: &str) -> &'static str {
    match ext {
        "ts" => {
            "export function target(value: number): number {\n  const body_marker = helper(value);\n  return body_marker;\n}\n"
        }
        "tsx" => {
            "export function target(value: number) {\n  const body_marker = helper(value);\n  return <div>{body_marker}</div>;\n}\n"
        }
        "js" => {
            "export function target(value) {\n  const body_marker = helper(value);\n  return body_marker;\n}\n"
        }
        "py" => "def target(value):\n    body_marker = helper(value)\n    return body_marker\n",
        "go" => {
            "package fixture\nfunc target(value int) int {\n  body_marker := helper(value)\n  return body_marker\n}\n"
        }
        "rs" => {
            "fn target(value: i32) -> i32 {\n  let body_marker = helper(value);\n  body_marker\n}\n"
        }
        "java" => {
            "class Fixture {\n  int target(int value) {\n    int body_marker = helper(value);\n    return body_marker;\n  }\n}\n"
        }
        "c" => {
            "int target(int value) {\n  int body_marker = helper(value);\n  return body_marker;\n}\n"
        }
        "cpp" => {
            "class Fixture {\npublic:\n  int target(int value) {\n    int body_marker = helper(value);\n    return body_marker;\n  }\n};\n"
        }
        "cs" => {
            "class Fixture {\n  public int target(int value) {\n    int body_marker = helper(value);\n    return body_marker;\n  }\n}\n"
        }
        "scala" => {
            "object Fixture {\n  def target(value: Int): Int = {\n    val body_marker = helper(value)\n    body_marker\n  }\n}\n"
        }
        "cu" => {
            "__global__ void target(int *value) {\n  int body_marker = helper(*value);\n  *value = body_marker;\n}\n"
        }
        "asm" => "target:\n  mov body_marker, %rax\n  call helper\n  ret\n",
        _ => panic!("missing grammar fixture for .{ext}"),
    }
}

#[test]
fn every_registered_grammar_and_alias_parses_and_searches_real_source() {
    let mut extensions = HashSet::new();
    for entry in all_entries() {
        let source = fixture(entry.extensions[0]);
        let mut parser = Parser::new();
        parser
            .set_language(&entry.language)
            .expect("compatible grammar ABI");
        let tree = parser.parse(source, None).expect("grammar parses fixture");
        assert!(
            !tree.root_node().has_error(),
            ".{} fixture has parse errors: {}",
            entry.extensions[0],
            tree.root_node().to_sexp()
        );
        // Root-kind search must recover the complete input, not merely return
        // success with an empty result after dispatching to the wrong grammar.
        let rule = format!("rule:\n  kind: {}\n", tree.root_node().kind());
        for ext in entry.extensions {
            assert!(extensions.insert(*ext), "duplicate grammar alias .{ext}");
            let matches = crate::structural::search(source, ext, None, Some(&rule))
                .unwrap_or_else(|error| panic!(".{ext}: {error}"));
            assert_eq!(matches.len(), 1, ".{ext}: exact root match");
            assert_eq!(matches[0].text.trim_end(), source.trim_end(), ".{ext}");

            let graph_json =
                crate::signatures::extract_graph_facts(source, &format!("fixture.{ext}"))
                    .unwrap_or_else(|| panic!(".{ext}: advertised graph extraction unavailable"));
            let graph: serde_json::Value = serde_json::from_str(&graph_json)
                .unwrap_or_else(|error| panic!(".{ext}: invalid graph JSON: {error}"));
            assert!(
                graph["declarations"]
                    .as_array()
                    .is_some_and(|declarations| declarations
                        .iter()
                        .any(|declaration| declaration["name"] == "target")),
                ".{ext}: target declaration missing from graph facts: {graph_json}"
            );
            if entry.extensions[0] != "asm" {
                assert!(
                    graph["calls"]
                        .as_array()
                        .is_some_and(|calls| !calls.is_empty()),
                    ".{ext}: helper call missing from graph facts: {graph_json}"
                );
            }

            if let Some(language_id) = entry.language_id {
                let file = format!("fixture.{ext}");
                let grammar = crate::lsp::grammar::grammar_for_file(&file)
                    .unwrap_or_else(|| panic!("missing LSP grammar for .{ext}"));
                assert_eq!(grammar.language_id, language_id, ".{ext}");
                assert!(
                    grammar
                        .parse_before(
                            "",
                            std::time::Instant::now()
                                + crate::signatures::extractor::AST_EXECUTION_TIMEOUT
                        )
                        .is_some(),
                    ".{ext}: LSP parser ABI"
                );
            }
        }
    }
    assert_eq!(extensions.len(), supported_extensions().len());
}

#[test]
fn every_advertised_signature_query_compiles_and_removes_a_real_body() {
    let mut failures = Vec::new();
    for entry in all_entries()
        .iter()
        .filter(|entry| !entry.body_query.is_empty())
    {
        let ext = entry.extensions[0];
        if let Err(error) = Query::new(&entry.language, entry.body_query) {
            failures.push(format!(".{ext}: invalid body query: {error}"));
            continue;
        }
        let source = fixture(ext);
        assert!(
            source.contains("body_marker"),
            ".{ext}: fixture body required"
        );
        let config = crate::signatures::extractor::LangExtractConfig {
            language: entry.language.clone(),
            body_query: entry.body_query,
        };
        let kept = crate::signatures::extractor::extract(source, &config)
            .unwrap_or_else(|| panic!(".{ext}: expected signature extraction"));
        let outline = kept
            .iter()
            .map(|(_, line)| line.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if !outline.contains("target") || outline.contains("body_marker") {
            failures.push(format!(
                ".{ext}: signature must keep target and drop body:\n{outline}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn grouped_go_imports_emit_each_spec_once() {
    let source =
        "package main\nimport (\n  \"fmt\"\n  lbl \"example.com/app/labels\"\n)\nimport \"os\"\n";
    let facts: serde_json::Value = serde_json::from_str(
        &crate::signatures::extract_graph_facts(source, "main.go").expect("facts"),
    )
    .expect("json");
    let imports = facts["imports"]
        .as_array()
        .expect("imports")
        .iter()
        .map(|import| {
            (
                import["specifier"].as_str().unwrap_or_default().to_owned(),
                import["line"].as_u64().unwrap_or(0),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        imports,
        vec![
            ("fmt".into(), 3),
            ("example.com/app/labels".into(), 4),
            ("os".into(), 6)
        ]
    );
}

#[test]
fn signature_outline_descends_into_top_level_iifes() {
    for wrapper in [
        "(function(){\n%\n})();\n",
        "(() => {\n%\n})();\n",
        "!function(){\n%\n}();\n",
        "(function(){\n%\n}).call(this);\n",
        // Nested wrappers (firebug-lite: an IIFE inside the file's IIFE).
        "(function(){\nvar outer = 1;\n(function(){\n%\n})();\n})();\n",
    ] {
        let source = wrapper.replace(
            '%',
            "function helper(n) {\n  return n + SECRET_BODY_LINE;\n}\nvar FBL = {};",
        );
        let outline =
            crate::signatures::extract_signatures_inner(&source, "umd.js").expect("outline");
        assert!(
            outline.contains("function helper(n)"),
            "{wrapper}: {outline}"
        );
        assert!(outline.contains("var FBL"), "{wrapper}: {outline}");
        assert!(
            !outline.contains("SECRET_BODY_LINE"),
            "inner bodies still drop: {outline}"
        );
    }
}

#[test]
fn deeply_nested_iife_outline_stays_inside_the_deadline() {
    // Every body capture climbed `Node::parent()` (a root-down search per
    // hop) through each enclosing IIFE wrapper.
    let depth = 400;
    let mut source = String::new();
    for level in 0..depth {
        source.push_str(&format!(
            "(function(){{\nfunction helper{level}(n) {{\n  return n + SECRET_BODY_LINE;\n}}\n"
        ));
    }
    source.push_str(&"})();\n".repeat(depth));
    let started = std::time::Instant::now();
    let outline =
        crate::signatures::extract_signatures_inner(&source, "nested.js").expect("outline");
    let elapsed = started.elapsed();
    assert!(outline.contains("function helper0(n)"), "{outline}");
    assert!(outline.contains("function helper399(n)"), "{outline}");
    assert!(!outline.contains("SECRET_BODY_LINE"), "{outline}");
    assert!(
        elapsed < std::time::Duration::from_secs(1),
        "took {elapsed:?}"
    );
}

#[test]
fn c_family_function_names_come_from_the_declarator_not_the_return_type() {
    let names = |source: &str, path: &str| -> Vec<String> {
        let facts: serde_json::Value = serde_json::from_str(
            &crate::signatures::extract_graph_facts(source, path).expect("facts"),
        )
        .expect("json");
        facts["declarations"]
            .as_array()
            .expect("declarations")
            .iter()
            .filter(|d| d["kind"] == "function")
            .map(|d| d["name"].as_str().unwrap_or_default().to_owned())
            .collect()
    };
    assert_eq!(
        names(
            "static int __init sched_fair_init(void) { return 0; }\nstruct rq *this_rq(void) { return 0; }\n",
            "fair.c"
        ),
        vec!["sched_fair_init", "this_rq"]
    );
    assert_eq!(
        names(
            "Tensor cat(const Tensor& a) { return a; }\nauto Engine::execute(int n) -> int { return n; }\nstd::string name() { return \"\"; }\nconst Tensor& ref(const Tensor& t) { return t; }\n",
            "ops.cpp"
        ),
        vec!["cat", "execute", "name", "ref"]
    );
    // Header prototypes are the API outline; function-pointer variables and
    // locals are not.
    assert_eq!(
        names(
            "void f(void);\nstruct rq *g(int a);\nint (*fp)(int);\nint x;\nvoid h(void) { void local(void); }\n",
            "api.h"
        ),
        vec!["f", "g", "h"]
    );
    assert_eq!(
        names(
            "namespace at { Tensor& add_(Tensor& self); }\nextern \"C\" { int c_api(void); }\n",
            "ops.hpp"
        ),
        vec!["add_", "c_api"]
    );
}

#[test]
fn declarations_carry_the_comment_block_directly_above_them() {
    let doc_lines = |source: &str, path: &str| -> Vec<(String, Option<u64>)> {
        let facts: serde_json::Value = serde_json::from_str(
            &crate::signatures::extract_declarations(source, path).expect("facts"),
        )
        .expect("json");
        facts["declarations"]
            .as_array()
            .expect("declarations")
            .iter()
            .map(|d| {
                (
                    d["name"].as_str().unwrap_or_default().to_owned(),
                    d["docLine"].as_u64(),
                )
            })
            .collect()
    };
    // JSDoc block (oxc path), and a declaration with no comment.
    assert_eq!(
        doc_lines(
            "/**\n * Delays calls.\n */\nfunction debounce() {}\n\nfunction plain() {}\n",
            "a.js"
        ),
        vec![("debounce".into(), Some(0)), ("plain".into(), None)]
    );
    // Rust: doc comments above attributes still belong to the item.
    assert_eq!(
        doc_lines("/// Spawns.\n#[inline]\npub fn spawn() {}\n", "lib.rs"),
        vec![("spawn".into(), Some(0))]
    );
    // Python `#` comments; C `#define` is code, not a comment.
    assert_eq!(
        doc_lines("# Adds.\ndef add(a, b):\n    return a + b\n", "m.py"),
        vec![("add".into(), Some(0))]
    );
    let c = doc_lines("#define N 1\nint f(void) { return N; }\n", "a.c");
    assert!(
        c.iter().any(|(name, doc)| name == "f" && doc.is_none()),
        "{c:?}"
    );
}

#[test]
fn csharp_outlines_namespaces_records_properties_and_delegates() {
    let source = "namespace Acme.Core\n{\n    /// <summary>A person.</summary>\n    public record Person(string Name);\n    public class Resolver\n    {\n        public int Count { get; set; }\n        public void Resolve() {}\n    }\n    public delegate void Handler();\n}\n";
    let facts: serde_json::Value = serde_json::from_str(
        &crate::signatures::extract_declarations(source, "a.cs").expect("facts"),
    )
    .expect("json");
    let rows = facts["declarations"]
        .as_array()
        .expect("declarations")
        .iter()
        .map(|d| {
            (
                d["kind"].as_str().unwrap_or_default().to_owned(),
                d["name"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect::<Vec<_>>();
    for expected in [
        ("namespace", "Acme.Core"),
        ("class", "Person"),
        ("class", "Resolver"),
        ("property", "Count"),
        ("method", "Resolve"),
        ("type", "Handler"),
    ] {
        assert!(
            rows.iter()
                .any(|(kind, name)| kind == expected.0 && name == expected.1),
            "{expected:?} missing from {rows:?}"
        );
    }
    let person = facts["declarations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["name"] == "Person")
        .unwrap();
    assert_eq!(person["docLine"], 2, "/// XML doc attaches to the record");
}

#[test]
fn go_type_declarations_emit_each_spec_once_without_self_parents() {
    let source = "package main\ntype engineMetrics struct {\n  n int\n}\ntype (\n  ErrA string\n  ErrB string\n)\n";
    let facts: serde_json::Value = serde_json::from_str(
        &crate::signatures::extract_graph_facts(source, "main.go").expect("facts"),
    )
    .expect("json");
    let rows: Vec<(String, Option<String>)> = facts["declarations"]
        .as_array()
        .expect("declarations")
        .iter()
        .filter(|d| d["kind"] == "type")
        .map(|d| {
            (
                d["name"].as_str().unwrap_or_default().to_owned(),
                d["parent"].as_str().map(str::to_owned),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("engineMetrics".to_owned(), None),
            ("ErrA".to_owned(), None),
            ("ErrB".to_owned(), None),
        ],
        "{facts}"
    );
}

/// A function in a type body is a `method` in every grammar (the JS/TS
/// extractor's label), so `kinds:["method"]` finds Python and Rust methods.
#[test]
fn functions_in_type_bodies_are_methods_in_every_grammar() {
    let kinds = |source: &str, path: &str| -> Vec<(String, String)> {
        let facts: serde_json::Value = serde_json::from_str(
            &crate::signatures::extract_declarations(source, path).expect("facts"),
        )
        .expect("json");
        facts["declarations"]
            .as_array()
            .expect("declarations")
            .iter()
            .map(|d| {
                (
                    d["name"].as_str().unwrap_or_default().to_owned(),
                    d["kind"].as_str().unwrap_or_default().to_owned(),
                )
            })
            .collect()
    };
    let python = kinds(
        "def top():\n    def inner():\n        pass\n\nclass QuerySet:\n    def get_or_create(self):\n        def helper():\n            pass\n\n    @property\n    def size(self):\n        return 1\n",
        "q.py",
    );
    for (name, kind) in [
        ("top", "function"),
        ("inner", "function"),
        ("get_or_create", "method"),
        ("helper", "function"),
        ("size", "method"),
    ] {
        assert!(
            python.contains(&(name.to_owned(), kind.to_owned())),
            "{name}: {python:?}"
        );
    }
    let rust = kinds(
        "fn free() {}\nstruct S;\nimpl S { fn operation(&self) { fn nested() {} } }\ntrait T { fn required(&self); fn provided(&self) {} }\nmod m { fn in_mod() {} }\n",
        "lib.rs",
    );
    for (name, kind) in [
        ("free", "function"),
        ("operation", "method"),
        ("nested", "function"),
        ("provided", "method"),
        ("in_mod", "function"),
    ] {
        assert!(
            rust.contains(&(name.to_owned(), kind.to_owned())),
            "{name}: {rust:?}"
        );
    }
    let go = kinds(
        "package p\nfunc Free() {}\nfunc (s *S) Method() {}\n",
        "p.go",
    );
    assert!(
        go.contains(&("Free".to_owned(), "function".to_owned())),
        "{go:?}"
    );
    assert!(
        go.contains(&("Method".to_owned(), "method".to_owned())),
        "{go:?}"
    );
}
