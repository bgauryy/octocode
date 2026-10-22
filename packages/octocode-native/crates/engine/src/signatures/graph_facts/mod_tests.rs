use super::*;
use crate::text::file_extension::extension_of;
use serde_json::Value;

pub(super) fn extract_graph_facts(content: &str, file_path: &str) -> Option<String> {
    extract_graph_facts_with_metadata(content, file_path)
        .and_then(|extraction| serde_json::to_string(&extraction.facts).ok())
}

fn extract_graph_facts_with_metadata(
    content: &str,
    file_path: &str,
) -> Option<crate::signatures::GraphFactsExtraction> {
    let extension = extension_of(file_path, true, "txt");
    extract_graph_facts_with_metadata_with_extension(content, file_path, &extension)
}

fn extract_graph_facts_before(
    content: &str,
    file_path: &str,
    deadline: std::time::Instant,
) -> Option<String> {
    let extension = extension_of(file_path, true, "txt");
    extract_graph_facts_with_metadata_before(content, file_path, &extension, deadline)
        .and_then(|extraction| serde_json::to_string(&extraction.facts).ok())
}

/// Reference count of the first declaration named `name`.
fn reference_count(source: &str, path: &str, name: &str) -> u32 {
    let extraction = extract_graph_facts_with_metadata(source, path).expect("graph facts");
    let id = &extraction
        .facts
        .declarations
        .iter()
        .find(|declaration| declaration.name == name)
        .unwrap_or_else(|| panic!("declaration {name}"))
        .id;
    extraction
        .reference_counts
        .iter()
        .find(|count| &count.declaration_id == id)
        .expect("counted")
        .count
}

#[test]
fn rust_paths_outside_calls_and_uses_are_facts() {
    let source = "use crate::util::run;\nconst A: u32 = crate::limits::MAX;\nfn f() -> crate::types::Out {\n    let g = crate::rules::globs;\n    crate::calls::direct();\n    json!({\"x\": crate::macros::recovery(a), \"y\": $crate::m::Z});\n    let _ = Self::new;\n    mod inner { fn h() { super::super::up::X; } }\n}\n";
    let facts = extract_graph_facts_with_metadata(source, "src/lib.rs")
        .expect("graph facts")
        .facts;
    let paths = facts
        .paths
        .iter()
        .map(|p| {
            (
                p.path.as_str(),
                p.line,
                p.caller_id.is_some(),
                p.inline_depth,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        [
            ("crate::limits::MAX", 2, true, 0),
            ("crate::types::Out", 3, true, 0),
            ("crate::rules::globs", 4, true, 0),
            ("crate::m::Z", 6, true, 0),
            ("crate::macros::recovery", 6, true, 0),
            ("super::super::up::X", 8, true, 1),
        ]
    );
}

#[test]
fn rust_comments_and_strings_are_not_references() {
    let source = "/// `helper` is documented; see helper.\npub fn helper() {}\n// helper\n/* helper */\npub fn caller() -> &'static str { \"helper\" }\n";
    assert_eq!(reference_count(source, "lib.rs", "helper"), 0);
}

#[test]
fn rust_value_uses_and_format_captures_count_but_calls_do_not() {
    let source = "pub fn callback() {}\npub fn called() {}\npub const LIMIT: u32 = 1;\npub struct Svc;\nimpl Svc { pub fn run(&self) {} pub fn go(&self) { self.run(); called(); register(callback); println!(\"{LIMIT} {{LIMIT}}\"); } }\n";
    assert_eq!(reference_count(source, "lib.rs", "callback"), 1);
    assert_eq!(reference_count(source, "lib.rs", "called"), 0);
    assert_eq!(reference_count(source, "lib.rs", "run"), 0);
    assert_eq!(reference_count(source, "lib.rs", "LIMIT"), 1);
    assert_eq!(
        reference_count(source, "lib.rs", "Svc"),
        0,
        "the impl names the struct as a declaration, not a use"
    );
}

#[test]
fn inline_format_captures_skip_escapes_and_positional_arguments() {
    assert_eq!(
        inline_format_captures("\"{a} {b:?} {{c}} {0} {} {_d:>4}\""),
        ["a", "b", "_d"]
    );
}

#[test]
fn rust_import_ranges_do_not_invent_synthetic_name_tokens() {
    let value = facts(
        "use crate::origin::{self as module_alias, *};\nuse crate::origin::plain;\n",
        "imports.rs",
    );
    let imports = value["imports"].as_array().unwrap();
    assert!(imports[0].get("importedRange").is_none());
    assert!(imports[0].get("localRange").is_some());
    assert!(imports[1].get("importedRange").is_none());
    assert!(imports[1].get("localRange").is_none());
    assert_eq!(imports[2]["localRange"], imports[2]["importedRange"]);
    assert!(imports[2].get("importedRange").is_some());
}

#[test]
fn rust_import_binding_ranges_are_exact_utf16() {
    let value = facts(
        "/*😀*/ use crate::origin::{target as first, target as second};\nuse crate::origin::{\n target as third,\n};\nuse crate::origin::target as fourth;\n",
        "aliases.rs",
    );
    let imports = value["imports"].as_array().unwrap();
    assert_eq!(imports.len(), 4);
    for (index, (line, imported, local, length)) in [
        (0, 27, 37, 5),
        (0, 44, 54, 6),
        (2, 1, 11, 5),
        (4, 19, 29, 6),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            imports[index]["importedRange"],
            serde_json::json!({"start":{"line":line,"character":imported},"end":{"line":line,"character":imported+6}})
        );
        assert_eq!(
            imports[index]["localRange"],
            serde_json::json!({"start":{"line":line,"character":local},"end":{"line":line,"character":local+length}})
        );
    }
}

#[test]
fn python_import_ranges_do_not_invent_synthetic_name_tokens() {
    let value = facts(
        "import package.sub as alias\nfrom origin import plain\nfrom origin import *\n",
        "imports.py",
    );
    let imports = value["imports"].as_array().unwrap();
    assert!(imports[0].get("importedRange").is_none());
    assert_eq!(
        imports[0]["localRange"],
        serde_json::json!({"start":{"line":0,"character":22},"end":{"line":0,"character":27}})
    );
    assert_eq!(imports[1]["localRange"], imports[1]["importedRange"]);
    assert!(imports[2].get("importedRange").is_none());
    assert!(imports[2].get("localRange").is_none());
}

#[test]
fn python_import_binding_ranges_are_exact_utf16() {
    let value = facts(
        "marker = \"😀\"; from origin import target as first, target as second\nfrom origin import (\n    target as third,\n)\n",
        "aliases.py",
    );
    let imports = value["imports"].as_array().unwrap();
    assert_eq!(imports.len(), 3);
    for (index, (line, imported, local, length)) in [(0, 34, 44, 5), (0, 51, 61, 6), (2, 4, 14, 5)]
        .into_iter()
        .enumerate()
    {
        assert_eq!(
            imports[index]["importedRange"],
            serde_json::json!({"start":{"line":line,"character":imported},"end":{"line":line,"character":imported+6}})
        );
        assert_eq!(
            imports[index]["localRange"],
            serde_json::json!({"start":{"line":line,"character":local},"end":{"line":line,"character":local+length}})
        );
    }
}

#[test]
fn rust_cfg_and_path_attributes_survive_comments_and_inner_attributes() {
    let value = facts(
        "#[cfg(feature = \"x\")]\n// note\nmod child;\n#[path = \"actual.rs\"]\n/// documented\nmod alias;\nmod gated { #![cfg(feature = \"x\")] use super::Thing; }",
        "src/lib.rs",
    );
    assert!(
        value["modules"]
            .as_array()
            .unwrap()
            .iter()
            .any(|module| module["name"] == "child" && module["unsupported"] != true)
    );
    assert!(
        value["modules"]
            .as_array()
            .unwrap()
            .iter()
            .any(|module| module["name"] == "alias" && module["path"] == "actual.rs")
    );
    // `cfg` gates compilation, not the module's file: the edge stays.
    assert!(
        value["modules"]
            .as_array()
            .unwrap()
            .iter()
            .any(|module| module["name"] == "gated" && module["unsupported"] != true)
    );
    let root = facts("#![cfg(feature = \"x\")]\nmod child;", "src/lib.rs");
    assert_ne!(root["rustRootUnsupported"], true);
    // A conditional attribute that rewrites the path is still unknown.
    let rewritten = facts(
        "#[cfg_attr(unix, path = \"u.rs\")] mod child;",
        "src/lib.rs",
    );
    assert!(
        rewritten["modules"]
            .as_array()
            .unwrap()
            .iter()
            .all(|module| module["unsupported"] == true)
    );
    let local = facts(
        "fn f() { mod hidden { #[path = \"child.rs\"] mod child; } }",
        "src/lib.rs",
    );
    assert!(
        local["modules"]
            .as_array()
            .unwrap()
            .iter()
            .all(|module| module["unsupported"] == true)
    );
}

#[test]
fn rust_modules_preserve_literal_paths_inline_scopes_and_unknown_cfg() {
    let value = facts(
        "#[path = \"actual.rs\"] mod alias;\nmod inside { use super::Thing; #[path = \"nested.rs\"] mod child; }\n#[cfg(feature = \"x\")] mod conditional;",
        "src/lib.rs",
    );
    let modules = value["modules"].as_array().unwrap();
    assert!(modules.iter().any(|module| module["name"] == "alias"
        && module["path"] == "actual.rs"
        && module["unsupported"] != true));
    assert!(modules.iter().any(|module| module["name"] == "child"
        && module["scope"] == serde_json::json!(["inside"])
        && module["path"] == "nested.rs"));
    assert!(
        modules
            .iter()
            .any(|module| module["name"] == "conditional" && module["unsupported"] != true)
    );
    assert!(
        value["imports"]
            .as_array()
            .unwrap()
            .iter()
            .any(|import| import["specifier"] == "super::Thing"
                && import["moduleScope"] == serde_json::json!(["inside"])
                && import["resolutionHint"].is_null())
    );
}

#[test]
fn declaration_occurrences_do_not_alias_equal_names_or_impl_blocks() {
    for (source, path) in [
        (
            "struct A; impl A { fn run() { work(); } } struct B; impl B { fn run() { work(); } }",
            "names.rs",
        ),
        (
            "class A:\n def run(self): work()\nclass B:\n def run(self): work()\n",
            "names.py",
        ),
    ] {
        let value = facts(source, path);
        let declarations = value["declarations"].as_array().unwrap();
        let ids: std::collections::HashSet<_> = declarations
            .iter()
            .map(|d| d["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids.len(), declarations.len());
        for edge in value["edges"].as_array().unwrap() {
            if edge["relation"] == "calls" {
                assert!(ids.contains(edge["from"].as_str().unwrap()));
                assert!(edge["to"].as_str().unwrap().starts_with("reference:"));
                assert_eq!(edge["resolution"], "unresolved");
            }
        }
    }
}

#[test]
fn rust_use_trees_expand_multiline_groups_aliases_and_modules() {
    let value = facts(
        "mod child;\nuse super::{\n types::{Thing as Alias, Other},\n language::AgLanguage,\n};\n",
        "src/structural/files.rs",
    );
    let imports = value["imports"].as_array().unwrap();
    for expected in [
        "self::child",
        "super::types::Thing",
        "super::types::Other",
        "super::language::AgLanguage",
    ] {
        assert!(
            imports.iter().any(|i| i["specifier"] == expected),
            "missing {expected}: {imports:?}"
        );
    }
    assert!(
        imports
            .iter()
            .any(|i| i["localName"] == "Alias" && i["importedName"] == "Thing")
    );
}

#[test]
fn use_heavy_rust_file_keeps_its_imports_inside_the_deadline() {
    // Per-`use` `parent()`/sibling walks made this O(n²) and the
    // walk hit the deadline, discarding every import.
    let mut source = String::from("mod item0 { pub struct Name; }\n");
    for index in 0..6_000 {
        source.push_str(&format!("use crate::item{index}::Name;\n"));
    }
    let (value, elapsed) =
        crate::test_cpu::cpu_time_of(|| facts_with_load_proof_deadline(&source, "src/lib.rs"));
    let diagnostics = value["diagnostics"].as_array().unwrap();
    assert!(
        !codes(&value).iter().any(|code| code.ends_with("Deadline")),
        "{diagnostics:?}"
    );
    let imports = value["imports"].as_array().unwrap();
    assert_eq!(imports.len(), 6_000);
    assert_eq!(imports[5_999]["specifier"], "crate::item5999::Name");
    assert_eq!(imports[0]["moduleScope"], serde_json::json!([]));
    assert!(imports.iter().all(|i| i.get("resolutionHint").is_none()));
    assert!(
        elapsed < super::super::extractor::AST_EXECUTION_TIMEOUT / 2,
        "took {elapsed:?}"
    );
}

#[test]
fn rust_item_macro_bodies_are_read_as_items_and_expression_macros_are_not_gaps() {
    // tokio-style `cfg_rt! { ... }` wrappers hold real mod/use items.
    let value = facts(
        "cfg_rt! {\n    pub mod runtime;\n    pub use crate::runtime::Handle;\n}\nmod outer { cfg_net! { mod tcp; } }\nfn f() { println!(\"{}\", 1); let v = vec![1]; }\n",
        "src/lib.rs",
    );
    let modules = value["modules"].as_array().unwrap();
    assert!(
        modules
            .iter()
            .any(|m| m["name"] == "runtime" && m["line"] == 2),
        "{modules:?}"
    );
    assert!(
        modules
            .iter()
            .any(|m| m["name"] == "tcp" && m["scope"] == serde_json::json!(["outer"])),
        "{modules:?}"
    );
    assert!(
        value["imports"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["specifier"] == "crate::runtime::Handle")
    );
    assert!(
        !codes(&value).contains(&"rustMacroExpansion".to_owned()),
        "{}",
        value["diagnostics"]
    );
}

#[test]
fn rust_nonconventional_modules_and_macros_remain_explicitly_unsupported() {
    let value = facts(
        "#[cfg_attr(unix, path = \"other.rs\")] mod child;\nmod inline { use super::Thing; }\nmake_imports!();",
        "src/lib.rs",
    );
    let imports = value["imports"].as_array().unwrap();
    assert!(
        imports
            .iter()
            .any(|item| item["specifier"] == "self::child"
                && item["resolutionHint"] == "unsupported")
    );
    assert!(
        imports
            .iter()
            .any(|item| item["specifier"] == "super::Thing"
                && item["moduleScope"] == serde_json::json!(["inline"]))
    );
    assert!(codes(&value).contains(&"rustMacroExpansion".to_owned()));
}

#[cfg(feature = "tree-sitter-cpp")]
#[test]
fn cpp_class_owns_its_method_declarations() {
    let value = facts(
        "class Fixture { public: int target(int value) { return value; } };",
        "fixture.cpp",
    );
    let declarations = value["declarations"].as_array().unwrap();
    let class = declarations
        .iter()
        .find(|item| item["name"] == "Fixture")
        .expect("class declaration");
    let method = declarations
        .iter()
        .find(|item| item["name"] == "target")
        .expect("method declaration");
    assert_eq!(class["kind"], "class");
    assert_eq!(method["parent"], class["id"]);
    assert!(
        value["edges"]
            .as_array()
            .unwrap()
            .iter()
            .any(|edge| edge["relation"] == "contains"
                && edge["from"] == class["id"]
                && edge["to"] == method["id"])
    );
}

fn facts(src: &str, path: &str) -> Value {
    let raw = extract_graph_facts(src, path).expect("graph facts expected");
    serde_json::from_str(&raw).expect("valid graph json")
}

/// Facts under a wall-clock deadline that a loaded machine cannot reach, for
/// the complexity tests: they bound the work by thread CPU time instead
/// (`crate::test_cpu`), so scheduler delay neither cuts the walk short nor
/// fails the bound.
fn facts_with_load_proof_deadline(src: &str, path: &str) -> Value {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    let raw = extract_graph_facts_before(src, path, deadline).expect("graph facts expected");
    serde_json::from_str(&raw).expect("valid graph json")
}

#[test]
fn expired_graph_budget_reports_incomplete_rust_root() {
    let raw = extract_graph_facts_before(
        "mod child; fn main() { work(); }",
        "lib.rs",
        std::time::Instant::now(),
    )
    .unwrap();
    let graph: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(graph["rustRootUnsupported"], true);
    assert_eq!(graph["imports"], serde_json::json!([]));
    assert!(codes(&graph).contains(&"parseDeadline".to_owned()));
}

#[test]
fn expired_graph_walk_does_not_emit_complete_facts() {
    let source = "fn main() { work(); }";
    let language = tree_sitter_rust::LANGUAGE.into();
    let tree = super::super::extractor::parse_before(
        source,
        &language,
        std::time::Instant::now() + super::super::extractor::AST_EXECUTION_TIMEOUT,
    )
    .unwrap();
    let index = NodePositions::new(source);
    let mut acc = GraphAccumulator::new("main.rs", "rs");
    assert!(!visit_node(
        tree.root_node(),
        source,
        &index,
        &mut acc,
        std::time::Instant::now(),
        &[],
        None
    ));
    assert!(acc.declarations.is_empty());
    assert!(acc.calls.is_empty());
}

#[test]
fn deeply_nested_rust_use_groups_do_not_recurse_on_the_native_stack() {
    let source = format!("use {}std{};", "{".repeat(10_000), "}".repeat(10_000));
    let graph = facts(&source, "imports.rs");
    assert_eq!(graph["imports"].as_array().unwrap().len(), 1);
    assert_eq!(graph["imports"][0]["specifier"], "std");
    assert!(!codes(&graph).contains(&"recoveredParse".to_owned()));
}

/// `depth` nested blocks, each calling `s.save()` on a typed local.
fn deeply_nested_rust_receivers(depth: usize) -> String {
    let mut source = String::from("fn main() { let s: Store = Store::new(); ");
    for _ in 0..depth {
        source.push_str("{ s.save(); ");
    }
    source.push_str(&"} ".repeat(depth));
    source.push('}');
    source
}

#[test]
fn deeply_nested_receiver_lookups_stay_inside_the_deadline() {
    // A receiver lookup climbed `Node::parent()` (a root-down search per
    // hop) from every member call to its function: O(depth²) per call.
    let source = deeply_nested_rust_receivers(1_500);
    let (graph, elapsed) = crate::test_cpu::cpu_time_of(|| {
        facts_with_load_proof_deadline(&source, "deep_receivers.rs")
    });
    let diagnostics = graph["diagnostics"].as_array().unwrap();
    assert!(
        !codes(&graph).iter().any(|code| code.ends_with("Deadline")),
        "{diagnostics:?}"
    );
    let saves = graph["calls"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|call| call["callee"] == "s.save")
        .collect::<Vec<_>>();
    assert_eq!(saves.len(), 1_500);
    assert!(saves.iter().all(|call| call["receiverType"] == "Store"));
    assert!(
        elapsed < super::super::extractor::AST_EXECUTION_TIMEOUT / 2,
        "took {elapsed:?}"
    );
}

#[test]
fn receiver_facts_are_unchanged_on_an_ordinary_file() {
    // Pinned output of the receiver lane on a normal file: locals,
    // shadowing, nested fns, `self.field`, and a constructor binding.
    let source = "struct Store { cache: Cache }\nimpl Store {\n    fn run(&self, db: Db) {\n        self.cache.get();\n        db.query();\n        let w = Writer::new();\n        w.flush();\n        {\n            let db = make();\n            db.query();\n        }\n        db.close();\n        fn inner() { w.flush(); }\n    }\n}\n";
    let graph = facts(source, "store.rs");
    let receivers = graph["calls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|call| {
            format!(
                "{}@{}={}",
                call["callee"].as_str().unwrap(),
                call["line"],
                call.get("receiverType")
                    .and_then(Value::as_str)
                    .unwrap_or("-")
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        receivers,
        vec![
            "self.cache.get@4=Cache",
            "db.query@5=Db",
            "Writer::new@6=-",
            "w.flush@7=Writer",
            "make@9=-",
            "db.query@10=-",
            "db.close@12=Db",
            "w.flush@13=-",
        ]
    );
}

#[test]
fn deeply_nested_graph_traversal_preserves_calls() {
    let source = format!(
        "fn main() {{ let x = {}probe(){}; after(); }}",
        "[".repeat(10_000),
        "]".repeat(10_000)
    );
    let graph = facts(&source, "deep.rs");
    assert_eq!(
        graph["diagnostics"],
        serde_json::json!([{
            "code": "syntaxOnly",
            "message": "tree-sitter graph facts are syntax-only; use LSP references/callHierarchy for semantic proof"
        }])
    );
    let calls = graph["calls"].as_array().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0]["caller"], "main");
    assert_eq!(calls[0]["callee"], "probe");
    assert_eq!(calls[1]["caller"], "main");
    assert_eq!(calls[1]["callee"], "after");
}

#[test]
fn graph_traversal_restores_declaration_context_after_nested_scopes() {
    let graph = facts(
        "fn outer() { fn inner() { inside(); } after(); } fn sibling() { next(); }",
        "scopes.rs",
    );
    let calls: Vec<_> = graph["calls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|call| {
            (
                call["caller"].as_str().unwrap(),
                call["callee"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        calls,
        [("inner", "inside"), ("outer", "after"), ("sibling", "next")]
    );
    let declarations = graph["declarations"].as_array().unwrap();
    assert_eq!(declarations[0]["name"], "outer");
    assert_eq!(declarations[1]["name"], "inner");
    assert_eq!(declarations[1]["parent"], declarations[0]["id"]);
    assert_eq!(declarations[2]["name"], "sibling");
    assert!(declarations[2]["parent"].is_null());
}

#[test]
fn rust_graph_facts_include_pub_declarations_and_calls() {
    let src = r#"
use crate::other::helper;

pub struct Point {
    x: f64,
}

pub fn distance(point: Point) -> f64 {
    helper(point.x)
}
"#;
    let graph = facts(src, "geo.rs");
    assert_eq!(
        graph["schemaVersion"],
        crate::signatures::GRAPH_FACTS_SCHEMA_VERSION
    );
    assert!(
        graph
            .get("language")
            .is_some_and(|language| language == "rust")
    );
    assert!(
        graph
            .get("declarations")
            .and_then(Value::as_array)
            .is_some_and(|decls| decls
                .iter()
                .any(|decl| decl.get("name").is_some_and(|name| name == "Point")))
    );
    assert!(
        graph
            .get("declarations")
            .and_then(Value::as_array)
            .is_some_and(|decls| decls.iter().any(|decl| decl
                .get("name")
                .is_some_and(|name| name == "distance")
                && decl
                    .get("exported")
                    .is_some_and(|exported| exported == true)))
    );
    assert!(
        graph
            .get("imports")
            .and_then(Value::as_array)
            .is_some_and(|imports| imports.iter().any(|import| import
                .get("specifier")
                .and_then(Value::as_str)
                .is_some_and(|specifier| specifier.contains("crate::other"))))
    );
    assert!(
        graph
            .get("calls")
            .and_then(Value::as_array)
            .is_some_and(|calls| calls
                .iter()
                .any(|call| call.get("callee").is_some_and(|callee| callee == "helper")))
    );
}

#[test]
fn python_import_facts_preserve_module_paths_and_alias_bindings() {
    let graph = facts(
        "import os, pkg.worker as worker\nfrom .target import run as execute\nfrom . import sibling\n",
        "pkg/service.py",
    );
    let imports = graph["imports"].as_array().unwrap();
    assert_eq!(imports.len(), 4);
    assert_eq!(imports[0]["specifier"], "os");
    assert_eq!(imports[1]["specifier"], "pkg.worker");
    assert_eq!(imports[1]["localName"], "worker");
    assert_eq!(imports[1]["resolutionHint"], "python-absolute");
    assert_eq!(imports[2]["specifier"], ".target");
    assert_eq!(imports[2]["importedName"], "run");
    assert_eq!(imports[2]["localName"], "execute");
    assert_eq!(imports[2]["resolutionHint"], "python-relative");
    assert_eq!(imports[3]["specifier"], ".");
    assert_eq!(imports[3]["importedName"], "sibling");
}

#[test]
fn c_import_facts_distinguish_quoted_system_and_computed_headers() {
    let graph = facts(
        "#include \"local.h\"\n#include <system.h>\n#include HEADER\n",
        "entry.c",
    );
    let imports = graph["imports"].as_array().unwrap();
    assert_eq!(imports.len(), 3);
    assert_eq!(imports[0]["specifier"], "local.h");
    assert_eq!(imports[0]["resolutionHint"], "c-relative");
    assert_eq!(imports[1]["resolutionHint"], "c-system");
    assert_eq!(imports[2]["resolutionHint"], "unsupported");
}

/// A file-inclusion macro with a literal path is an `include` import of
/// that path; a computed path or another macro is none.
#[test]
fn rust_include_macros_name_their_literal_file() {
    let graph = facts(
        "include!(\"gen/types.rs\");\nconst T: &str = include_str!(\"../data.txt\");\n\
         include!(concat!(env!(\"OUT_DIR\"), \"/x.rs\"));\nfn f() { println!(\"a.rs\"); }\n",
        "lib.rs",
    );
    let includes: Vec<_> = graph["imports"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|import| {
            matches!(
                import["importKind"].as_str(),
                Some("include" | "include-data")
            )
        })
        .map(|import| {
            (
                import["importKind"].clone(),
                import["specifier"].clone(),
                import["line"].clone(),
            )
        })
        .collect();
    assert_eq!(
        includes,
        [
            (
                serde_json::json!("include"),
                serde_json::json!("gen/types.rs"),
                serde_json::json!(1)
            ),
            (
                serde_json::json!("include-data"),
                serde_json::json!("../data.txt"),
                serde_json::json!(2)
            ),
        ]
    );
}

#[test]
fn python_graph_facts_include_module_public_defs() {
    let src = r#"
import os

class Service:
    def run(self):
        helper()

def helper():
    return os.getcwd()
"#;
    let graph = facts(src, "service.py");
    assert!(
        graph
            .get("language")
            .is_some_and(|language| language == "python")
    );
    assert!(
        graph
            .get("declarations")
            .and_then(Value::as_array)
            .is_some_and(|decls| decls
                .iter()
                .any(|decl| decl.get("name").is_some_and(|name| name == "Service")))
    );
    assert!(
        graph
            .get("declarations")
            .and_then(Value::as_array)
            .is_some_and(|decls| decls.iter().any(|decl| decl
                .get("name")
                .is_some_and(|name| name == "helper")
                && decl
                    .get("exported")
                    .is_some_and(|exported| exported == true)))
    );
    assert!(
        graph
            .get("imports")
            .and_then(Value::as_array)
            .is_some_and(|imports| imports.iter().any(|import| import
                .get("specifier")
                .and_then(Value::as_str)
                .is_some_and(|specifier| specifier.contains("os"))))
    );
    assert!(
        graph
            .get("calls")
            .and_then(Value::as_array)
            .is_some_and(|calls| calls
                .iter()
                .any(|call| call.get("callee").is_some_and(|callee| callee == "helper")))
    );
}

fn facts_json(source: &str, path: &str) -> Value {
    serde_json::from_str(&extract_graph_facts(source, path).expect("graph facts"))
        .expect("facts JSON")
}

fn declared_names(source: &str, path: &str) -> Vec<String> {
    facts_json(source, path)["declarations"]
        .as_array()
        .expect("declarations")
        .iter()
        .filter_map(|declaration| declaration["name"].as_str().map(str::to_owned))
        .collect()
}

#[test]
fn keyword_named_declarations_survive_outside_the_js_family() {
    let rust = declared_names(
        "struct Foo;\nimpl Foo {\n    pub fn new() -> Self { Foo }\n    pub fn build() -> Self { Foo }\n}\n",
        "a.rs",
    );
    for name in ["new", "build"] {
        assert!(
            rust.iter().any(|found| found == name),
            "rust {name}: {rust:?}"
        );
    }
    let python = declared_names(
        "def new():\n    pass\n\nclass K:\n    def case(self):\n        pass\n",
        "a.py",
    );
    for name in ["new", "case"] {
        assert!(
            python.iter().any(|found| found == name),
            "python {name}: {python:?}"
        );
    }
}

/// The tree-sitter lane (what TypeScript falls back to when oxc cannot
/// parse it) names TypeScript aliases, abstract classes and namespaces with
/// the kinds the oxc lane uses.
#[test]
fn tree_sitter_typescript_declarations_keep_their_kinds() {
    let source = "export type Id = string;\nexport abstract class Shape {}\nnamespace Geo { export const x = 1; }\n";
    let facts = extract_graph_facts_with_metadata_with_extension(source, "a.ts", "ts")
        .expect("tree-sitter graph facts")
        .facts;
    let kind_of = |name: &str| {
        facts
            .declarations
            .iter()
            .find(|declaration| declaration.name == name)
            .map(|declaration| declaration.kind.clone())
            .unwrap_or_else(|| panic!("{name}: {:?}", facts.declarations))
    };
    assert_eq!(kind_of("Id"), "type");
    assert_eq!(kind_of("Shape"), "class");
    assert_eq!(kind_of("Geo"), "namespace");
}

#[test]
fn keyword_named_class_members_stay_legal_in_clean_js() {
    let js = declared_names("class A {\n  if() {}\n  build() {}\n}\n", "a.js");
    assert!(js.iter().any(|found| found == "build"), "{js:?}");
    assert!(js.iter().any(|found| found == "if"), "{js:?}");
}

/// `(relation, from declaration name, to, line)` for every heritage edge.
fn heritage(source: &str, path: &str) -> Vec<(String, String, String, u64)> {
    let facts = facts_json(source, path);
    let declarations = facts["declarations"].as_array().expect("declarations");
    facts["edges"]
        .as_array()
        .expect("edges")
        .iter()
        .filter(|edge| matches!(edge["relation"].as_str(), Some("extends" | "implements")))
        .map(|edge| {
            assert_eq!(edge["source"], "tree-sitter");
            assert_eq!(edge["resolution"], "syntax");
            let from = declarations
                .iter()
                .find(|declaration| declaration["id"] == edge["from"])
                .unwrap_or_else(|| panic!("edge from is a declaration id: {edge}"));
            (
                edge["relation"].as_str().unwrap_or_default().to_owned(),
                from["name"].as_str().unwrap_or_default().to_owned(),
                edge["to"].as_str().unwrap_or_default().to_owned(),
                edge["line"].as_u64().unwrap_or_default(),
            )
        })
        .collect()
}

fn edge(relation: &str, from: &str, to: &str, line: u64) -> (String, String, String, u64) {
    (relation.to_owned(), from.to_owned(), to.to_owned(), line)
}

#[test]
fn rust_heritage_links_impl_trait_and_supertraits() {
    let source = "trait Shape: Clone + std::fmt::Debug + 'static {}\nstruct Square;\nimpl std::fmt::Display for Square {}\nimpl<T> From<T> for Square {}\nimpl Square {}\n";
    assert_eq!(
        heritage(source, "lib.rs"),
        vec![
            edge("extends", "Shape", "Clone", 1),
            edge("extends", "Shape", "std::fmt::Debug", 1),
            edge("implements", "Square", "std::fmt::Display", 3),
            edge("implements", "Square", "From", 4),
        ]
    );
    let facts = facts_json(source, "lib.rs");
    let impl_ids: Vec<&Value> = facts["declarations"]
        .as_array()
        .expect("declarations")
        .iter()
        .filter(|declaration| declaration["kind"] == "impl")
        .map(|declaration| &declaration["id"])
        .collect();
    let froms: Vec<&Value> = facts["edges"]
        .as_array()
        .expect("edges")
        .iter()
        .filter(|edge| edge["relation"] == "implements")
        .map(|edge| &edge["from"])
        .collect();
    assert_eq!(
        froms,
        impl_ids[..2].to_vec(),
        "from is the impl declaration id"
    );
}

#[test]
fn python_heritage_skips_keywords_splats_and_object() {
    let source = "class A(Base, pkg.Mixin, Generic[T], metaclass=Meta):\n    pass\nclass B(object):\n    pass\nclass C(*bases, **kw):\n    pass\n";
    assert_eq!(
        heritage(source, "m.py"),
        vec![
            edge("extends", "A", "Base", 1),
            edge("extends", "A", "pkg.Mixin", 1),
            edge("extends", "A", "Generic", 1),
        ]
    );
}

#[test]
fn java_heritage_separates_extends_and_implements() {
    let source = "class A extends Base<String> implements Runnable, java.io.Serializable {}\ninterface I extends J, K<T> {}\nenum E implements I {}\nrecord R(int x) implements I {}\n";
    assert_eq!(
        heritage(source, "A.java"),
        vec![
            edge("extends", "A", "Base", 1),
            edge("implements", "A", "Runnable", 1),
            edge("implements", "A", "java.io.Serializable", 1),
            edge("extends", "I", "J", 2),
            edge("extends", "I", "K", 2),
            edge("implements", "E", "I", 3),
            edge("implements", "R", "I", 4),
        ]
    );
}

#[test]
fn cpp_heritage_reads_the_base_class_clause() {
    let source = "class A : public Base, private ns::Mixin<int> {};\nstruct S : virtual Base {};\nclass Plain {};\n";
    assert_eq!(
        heritage(source, "a.cpp"),
        vec![
            edge("extends", "A", "Base", 1),
            edge("extends", "A", "ns::Mixin", 1),
            edge("extends", "S", "Base", 2),
        ]
    );
}

#[test]
fn csharp_heritage_treats_the_first_class_base_as_extends() {
    let source = "class A : Base, IDisposable, IList<int> {}\ninterface I : J, K {}\nstruct S : IEquatable<S> {}\nrecord R(int X) : Base(X);\n";
    assert_eq!(
        heritage(source, "A.cs"),
        vec![
            edge("extends", "A", "Base", 1),
            edge("implements", "A", "IDisposable", 1),
            edge("implements", "A", "IList", 1),
            edge("extends", "I", "J", 2),
            edge("extends", "I", "K", 2),
            edge("implements", "S", "IEquatable", 3),
            edge("extends", "R", "Base", 4),
        ]
    );
}

#[test]
fn heritage_edges_name_their_unresolved_base() {
    let extraction =
        extract_graph_facts_with_metadata("class A(Base):\n    pass\n", "m.py").expect("facts");
    let extends = extraction
        .facts
        .edges
        .iter()
        .find(|edge| edge.relation == "extends")
        .expect("extends edge");
    let class = &extraction.facts.declarations[0];
    assert_eq!(class.name, "A");
    assert_eq!(extends.from, class.id);
    assert_eq!(extends.to, "Base");
}

#[test]
fn module_level_calls_have_no_caller_id() {
    let source = "import app\n\n@app.route('/')\ndef index():\n    helper()\n\nsetup()\nif __name__ == '__main__':\n    index()\n";
    let facts = facts_json(source, "main.py");
    let calls: Vec<(String, String, bool)> = facts["calls"]
        .as_array()
        .expect("calls")
        .iter()
        .map(|call| {
            (
                call["caller"].as_str().unwrap_or_default().to_owned(),
                call["callee"].as_str().unwrap_or_default().to_owned(),
                call.get("callerId").is_some(),
            )
        })
        .collect();
    assert_eq!(
        calls,
        vec![
            ("module".to_owned(), "app.route".to_owned(), false),
            ("index".to_owned(), "helper".to_owned(), true),
            ("module".to_owned(), "setup".to_owned(), false),
            ("module".to_owned(), "index".to_owned(), false),
        ]
    );
    let module_edge = facts["edges"]
        .as_array()
        .expect("edges")
        .iter()
        .find(|edge| edge["relation"] == "calls" && edge["line"] == 7)
        .expect("module call edge");
    assert_eq!(module_edge["from"], "file:main.py");
    // A module-level call target is an edge, not a value reference.
    assert_eq!(reference_count(source, "main.py", "index"), 0);
}

#[test]
fn module_level_calls_cover_go_rust_and_c() {
    let callers = |source: &str, path: &str| -> Vec<(String, String)> {
        facts_json(source, path)["calls"]
            .as_array()
            .expect("calls")
            .iter()
            .map(|call| {
                (
                    call["caller"].as_str().unwrap_or_default().to_owned(),
                    call["callee"].as_str().unwrap_or_default().to_owned(),
                )
            })
            .collect()
    };
    assert!(
        callers("package p\n\nvar x = build()\n", "p.go")
            .contains(&("module".to_owned(), "build".to_owned()))
    );
    assert!(
        callers(
            "lazy_static! { static ref X: u8 = 1; }\nfn f() { g(); }\n",
            "lib.rs"
        )
        .contains(&("module".to_owned(), "lazy_static".to_owned()))
    );
    assert!(
        callers("int f(void);\nint x = f();\n", "a.cpp")
            .contains(&("module".to_owned(), "f".to_owned()))
    );
}

#[test]
fn csharp_using_directives_are_imports() {
    let source = "using System.Text;\nusing static System.Math;\nglobal using System;\nusing Json = Newtonsoft.Json.JsonConvert;\nnamespace App { using Inner.Pkg; class A { void M() { using (var x = Open()) {} } } }\n";
    let facts = facts_json(source, "A.cs");
    let imports: Vec<(String, Option<String>, u64)> = facts["imports"]
        .as_array()
        .expect("imports")
        .iter()
        .map(|import| {
            (
                import["specifier"].as_str().unwrap_or_default().to_owned(),
                import["localName"].as_str().map(str::to_owned),
                import["line"].as_u64().unwrap_or_default(),
            )
        })
        .collect();
    assert_eq!(
        imports,
        vec![
            ("System.Text".to_owned(), None, 1),
            ("System.Math".to_owned(), None, 2),
            ("System".to_owned(), None, 3),
            (
                "Newtonsoft.Json.JsonConvert".to_owned(),
                Some("Json".to_owned()),
                4
            ),
            ("Inner.Pkg".to_owned(), None, 5),
        ]
    );
}

/// A recovered parse names the 1-based line spans its syntax errors cover,
/// so a consumer can tell whether a missing declaration could hide there.
#[test]
fn recovered_parse_reports_error_line_spans() {
    let source = "int ok(void) { return 1; }\n\nint broken( {\n  return 2;\n}\nint tail(void) { return 3; }\n";
    let facts = extract_graph_facts_with_metadata(source, "a.c")
        .expect("graph facts")
        .facts;
    assert!(
        facts
            .diagnostics
            .iter()
            .any(|note| note.code == crate::graph::GraphFactCode::RecoveredParse),
        "{facts:?}"
    );
    assert!(!facts.error_lines.is_empty(), "{facts:?}");
    assert!(
        facts
            .error_lines
            .iter()
            .all(|[start, end]| *start >= 3 && start <= end),
        "{:?}",
        facts.error_lines
    );
    assert!(
        !facts
            .error_lines
            .iter()
            .any(|[start, end]| *start <= 1 && 1 <= *end),
        "line 1 parses: {:?}",
        facts.error_lines
    );
    let clean = extract_graph_facts_with_metadata("int ok(void) { return 1; }\n", "b.c")
        .expect("graph facts")
        .facts;
    assert!(clean.error_lines.is_empty());
    let json = serde_json::to_value(&clean).expect("json");
    assert!(json.get("errorLines").is_none(), "{json}");
}

/// An error that runs to the end of a file with a trailing newline ends on
/// its last line, never the line after it (an end at column 0 is the
/// previous line's newline).
#[test]
fn an_error_reaching_the_file_end_ends_on_the_last_line() {
    for (path, source) in [
        (
            "a.c",
            "int ok(void) { return 1; }\nint broken( {\n  return 2;\n  more(;\n",
        ),
        (
            "a.js",
            "// @flow\ntype T = {| a: number |};\nconst x: Array<T> = [\n  {a: 1},\n",
        ),
    ] {
        let lines = u32::try_from(source.lines().count()).expect("lines");
        let facts = extract_graph_facts_with_metadata(source, path)
            .expect("graph facts")
            .facts;
        assert!(!facts.error_lines.is_empty(), "{path}: {facts:?}");
        assert!(
            facts.error_lines.iter().all(|[_, end]| *end <= lines),
            "{path}: {:?} past line {lines}",
            facts.error_lines
        );
        assert!(
            facts.error_lines.iter().any(|[_, end]| *end == lines),
            "{path}: {:?} reaches line {lines}",
            facts.error_lines
        );
    }
}

#[test]
fn rust_macro_body_members_nest_under_their_container_in_source_order() {
    // tokio's `impl NamedPipeServer { … cfg_io_util! { pub fn try_read_buf … } … }`:
    // a member generated inside an item-level macro of an impl body belongs to
    // that impl, as a method, at its source position.
    let value = facts(
        "struct S;\nimpl S {\n    fn a() {}\n    cfg_x! {\n        pub fn b() {}\n    }\n    fn c() {}\n}\nimpl T for S {\n    cfg_y! { fn d() {} }\n}\nfn z() {}\ncfg_z! { fn top() {} }\n",
        "src/lib.rs",
    );
    let declarations = value["declarations"].as_array().unwrap();
    let names: Vec<&str> = declarations
        .iter()
        .map(|d| d["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["S", "S", "a", "b", "c", "S", "d", "z", "top"]);
    let by_name = |name: &str| declarations.iter().find(|d| d["name"] == name).unwrap();
    assert_eq!(by_name("b")["parent"], declarations[1]["id"]);
    assert_eq!(by_name("b")["kind"], "method");
    assert_eq!(by_name("d")["parent"], declarations[5]["id"]);
    assert_eq!(by_name("d")["kind"], "method");
    assert!(by_name("top")["parent"].is_null());
    assert_eq!(by_name("top")["kind"], "function");
}

/// Every diagnostic carries its code; a module's attribute note names its
/// line in a field, not in its text.
#[test]
fn graph_fact_diagnostics_carry_typed_codes() {
    let broken = facts("fn f( {\n", "lib.rs");
    assert_eq!(codes(&broken), ["syntaxOnly", "recoveredParse"], "{broken}");
    let graph = facts("#![custom]\n#[custom]\nmod t;\n", "lib.rs");
    let module = graph["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|note| note["code"] == "rustModuleAttributes")
        .unwrap_or_else(|| panic!("{graph}"));
    assert_eq!(module["line"], 3, "{graph}");
    assert!(
        !module["message"].as_str().unwrap().contains("line"),
        "{graph}"
    );
    assert_eq!(
        codes(&graph),
        ["syntaxOnly", "rustCrateAttributes", "rustModuleAttributes"],
        "{graph}"
    );
    use crate::graph::GraphFactCode;
    assert!(GraphFactCode::RustModuleAttributes.is_linking_only());
    assert!(!GraphFactCode::RecoveredParse.is_linking_only());
    assert!(!GraphFactCode::TraversalDeadline.is_linking_only());
}

/// The code of each diagnostic of a facts document, in order.
fn codes(facts: &Value) -> Vec<String> {
    facts["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|diagnostic| diagnostic["code"].as_str().unwrap().to_owned())
        .collect()
}

/// Only plain `pub` exports a Rust item; a restricted visibility keeps it
/// inside the crate.
#[test]
fn rust_restricted_visibility_is_not_exported() {
    let src = "pub fn open() {}\npub(crate) fn krate() {}\npub(super) fn parent() {}\npub(self) fn own() {}\npub(in crate::a) fn scoped() {}\nfn private() {}\npub\nfn split() {}\n";
    let graph = facts(src, "vis.rs");
    let exported = |name: &str| {
        graph["declarations"]
            .as_array()
            .expect("declarations")
            .iter()
            .find(|decl| decl["name"] == name)
            .unwrap_or_else(|| panic!("{name}: {graph}"))
            .get("exported")
            == Some(&Value::Bool(true))
    };
    assert!(exported("open"));
    assert!(exported("split"));
    for name in ["krate", "parent", "own", "scoped", "private"] {
        assert!(!exported(name), "{name}");
    }
}
