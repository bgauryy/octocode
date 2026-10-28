//! Signature outlines per grammar: names, nesting, comments and kinds.

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
    let (outline, elapsed) = crate::test_cpu::cpu_time_of(|| {
        crate::signatures::extract_signatures_inner(&source, "nested.js").expect("outline")
    });
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
    #[cfg(feature = "tree-sitter-cpp")]
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
    #[cfg(feature = "tree-sitter-cpp")]
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
        let facts: serde_json::Value = serde_json::to_value(
            crate::signatures::extract_declarations_document(source, path).expect("facts"),
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
    // The grammar's comment style decides: `.pyi` stubs are Python, and
    // assembly comments start with `;`.
    assert_eq!(
        doc_lines("# Adds.\ndef add(a: int) -> int: ...\n", "m.pyi"),
        vec![("add".into(), Some(0))]
    );
    #[cfg(feature = "tree-sitter-asm")]
    {
        let asm = doc_lines("; Entry point.\n_start:\n    ret\n", "boot.asm");
        assert!(
            asm.iter()
                .any(|(name, doc)| name == "_start" && *doc == Some(0)),
            "{asm:?}"
        );
    }
    let c = doc_lines("#define N 1\nint f(void) { return N; }\n", "a.c");
    assert!(
        c.iter().any(|(name, doc)| name == "f" && doc.is_none()),
        "{c:?}"
    );
}

#[cfg(feature = "tree-sitter-c-sharp")]
#[test]
fn csharp_outlines_namespaces_records_properties_and_delegates() {
    let source = "namespace Acme.Core\n{\n    /// <summary>A person.</summary>\n    public record Person(string Name);\n    public class Resolver\n    {\n        public int Count { get; set; }\n        public void Resolve() {}\n    }\n    public delegate void Handler();\n}\n";
    let facts: serde_json::Value = serde_json::to_value(
        crate::signatures::extract_declarations_document(source, "a.cs").expect("facts"),
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
        let facts: serde_json::Value = serde_json::to_value(
            crate::signatures::extract_declarations_document(source, path).expect("facts"),
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
