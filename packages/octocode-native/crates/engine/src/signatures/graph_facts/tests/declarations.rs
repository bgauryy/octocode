//! Declarations: containers, keyword names, kinds and export visibility.

use super::*;

#[test]
fn rust_macro_arms_do_not_declare_attribute_or_body_identifiers() {
    for body in [
        "#[macro_export]\n$item",
        "#[allow(dead_code)]\n$item",
        "body_identifier()",
    ] {
        let source = format!(
            "macro_rules! wrapper {{ ($item:item) => {{ {body} }}; }}\n\
             macro_rules! value {{ () => {{ 1 }}; }}\n\
             fn use_macro() {{ value!(); }}\n"
        );
        let graph = extract_graph_facts_with_metadata(&source, "macros.rs")
            .expect("graph facts")
            .facts;
        let outline = crate::signatures::extract_declarations_document(&source, "macros.rs")
            .expect("declarations");
        for document in [&graph, &outline] {
            let names: Vec<_> = document
                .declarations
                .iter()
                .map(|declaration| declaration.name.as_str())
                .collect();
            assert_eq!(names, ["wrapper", "value", "use_macro"], "{body}");
        }
        assert!(
            graph
                .calls
                .iter()
                .any(|call| call.caller == "use_macro" && call.callee == "value"),
            "macro invocation: {:?}",
            graph.calls
        );
    }
}

#[test]
fn rust_nested_macro_definitions_and_invocations_keep_their_owner() {
    let source = "fn use_macro() {\n\
                  macro_rules! local_value { () => { 1 }; }\n\
                  local_value!();\n\
                  }\n";
    let graph = extract_graph_facts_with_metadata(source, "nested-macro.rs")
        .expect("graph facts")
        .facts;
    let caller = graph
        .declarations
        .iter()
        .find(|declaration| declaration.name == "use_macro")
        .expect("function");
    let nested = graph
        .declarations
        .iter()
        .find(|declaration| declaration.name == "local_value")
        .expect("nested macro");
    assert_eq!(nested.kind, "macro");
    assert_eq!(nested.parent.as_deref(), Some(caller.id.as_str()));
    assert_eq!(graph.declarations.len(), 2);
    assert!(graph.calls.iter().any(|call| {
        call.callee == "local_value" && call.caller_id.as_deref() == Some(caller.id.as_str())
    }));
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

/// Body-less declarations (a Rust trait's required method or an extern
/// fn, a Go interface method, a C++ member declaration) are declarations
/// of their container, as the bodied ones are.
#[test]
fn body_less_signatures_are_declarations_of_their_container() {
    let member = |source: &str, path: &str, container: &str, name: &str, kind: &str| {
        let value = facts(source, path);
        let declarations = value["declarations"].as_array().unwrap();
        let parent = declarations
            .iter()
            .find(|item| item["name"] == container)
            .map(|item| item["id"].clone());
        let found = declarations
            .iter()
            .find(|item| item["name"] == name)
            .unwrap_or_else(|| panic!("{path} {name}: {declarations:?}"));
        assert_eq!(found["kind"], kind, "{path} {name}");
        if let Some(parent) = parent {
            assert_eq!(found["parent"], parent, "{path} {name}");
        }
    };
    let rust = "pub trait T { fn req(&self) -> u8; fn def(&self) {} }\nextern \"C\" { fn c_fn(x: i32); }\n";
    member(rust, "a.rs", "T", "req", "method");
    member(rust, "a.rs", "T", "def", "method");
    member(rust, "a.rs", "", "c_fn", "function");
    let go =
        "package p\ntype Reader interface {\n\tRead(p []byte) (int, error)\n\tClose() error\n}\n";
    member(go, "a.go", "Reader", "Read", "method");
    member(go, "a.go", "Reader", "Close", "method");
}

#[cfg(feature = "tree-sitter-cpp")]
#[test]
fn cpp_class_owns_its_member_function_declarations() {
    let value = facts(
        "class A { public: virtual void run() = 0; void go(); int field; };",
        "a.cpp",
    );
    let declarations = value["declarations"].as_array().unwrap();
    let class = declarations
        .iter()
        .find(|item| item["name"] == "A")
        .expect("class declaration");
    for name in ["run", "go"] {
        let method = declarations
            .iter()
            .find(|item| item["name"] == name)
            .unwrap_or_else(|| panic!("{name}: {declarations:?}"));
        assert_eq!(method["kind"], "method", "{name}");
        assert_eq!(method["parent"], class["id"], "{name}");
    }
    assert!(
        !declarations.iter().any(|item| item["name"] == "field"),
        "plain fields stay out: {declarations:?}"
    );
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
