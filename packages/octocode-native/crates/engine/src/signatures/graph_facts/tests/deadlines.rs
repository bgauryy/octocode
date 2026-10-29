//! Parse and walk deadlines, and deep nesting kept off the native stack.

use super::*;

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
    let tree = crate::signatures::extractor::parse_before(
        source,
        &language,
        std::time::Instant::now() + crate::signatures::extractor::AST_EXECUTION_TIMEOUT,
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
    let graph = serde_json::to_value(graph).expect("graph facts value");
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
        elapsed < crate::signatures::extractor::AST_EXECUTION_TIMEOUT / 2,
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
