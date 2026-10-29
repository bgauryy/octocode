//! Calls inside the arguments of allowlisted Rust expression macros.

use super::*;
use crate::graph::{GraphFactCall, GraphFactsDocument};

fn rust_facts(source: &str) -> GraphFactsDocument {
    extract_graph_facts_with_metadata(source, "src/lib.rs")
        .expect("graph facts")
        .facts
}

/// The calls of `facts` to `callee`.
fn calls_to<'a>(facts: &'a GraphFactsDocument, callee: &str) -> Vec<&'a GraphFactCall> {
    facts
        .calls
        .iter()
        .filter(|call| call.callee == callee)
        .collect()
}

/// The adapter id of the unit a call's source span is observed in.
fn unit_adapter<'a>(facts: &'a GraphFactsDocument, call: &GraphFactCall) -> &'a str {
    let span = call.source_range.expect("source range");
    &facts.source_templates.units[span.unit.0 as usize]
        .adapter
        .id
}

#[test]
fn ready_argument_is_a_call_in_an_expression_macro_unit() {
    let source = "use std::task::ready; fn f(cx: u8) { let c = ready!(crate::task::coop::poll_proceed(cx)); }";
    let facts = rust_facts(source);
    let calls = calls_to(&facts, "crate::task::coop::poll_proceed");
    assert_eq!(calls.len(), 1, "{:?}", facts.calls);
    let call = calls[0];
    let f = facts
        .declarations
        .iter()
        .find(|declaration| declaration.name == "f")
        .expect("f");
    assert_eq!(call.caller_id.as_deref(), Some(f.id.as_str()));
    assert_eq!(call.line, 1);
    assert_eq!(call.macros, ["ready"]);
    let span = call.source_range.expect("source range");
    assert_eq!(
        &source[span.range.start_byte as usize..span.range.end_byte as usize],
        "crate::task::coop::poll_proceed(cx)"
    );
    assert_eq!(unit_adapter(&facts, call), "rust-expression-macro-body");
    // The macro-name call stays, in the file's own unit.
    let ready = calls_to(&facts, "ready");
    assert_eq!(ready.len(), 1);
    assert!(ready[0].macros.is_empty());
    assert_eq!(ready[0].source_range.expect("span").unit.0, 0);
}

#[test]
fn qualified_std_macro_paths_are_admitted() {
    let facts = rust_facts("fn f(cx: u8) { std::task::ready!(poll_proceed(cx)); }");
    let calls = calls_to(&facts, "poll_proceed");
    assert_eq!(calls.len(), 1, "{:?}", facts.calls);
    assert_eq!(calls[0].macros, ["std::task::ready"]);
}

#[test]
fn std_forwarding_family_arguments_are_calls() {
    let facts = rust_facts(
        "fn g(x: u8) {\n    assert_eq!(helper(x), 1, \"msg {}\", helper(2));\n    println!(\"{} {v}\", helper(x), v = helper(3));\n    let _ = vec![helper(x); 3];\n}\n",
    );
    let helper = calls_to(&facts, "helper")
        .iter()
        .map(|call| (call.line, call.macros.clone()))
        .collect::<Vec<_>>();
    let one = |line: u32, name: &str| (line, vec![name.to_owned()]);
    assert_eq!(
        helper,
        [
            one(2, "assert_eq"),
            one(2, "assert_eq"),
            one(3, "println"),
            one(3, "println"),
            one(4, "vec"),
        ]
    );
    for name in ["assert_eq", "println", "vec"] {
        assert_eq!(calls_to(&facts, name).len(), 1, "{name}");
    }
}

#[test]
fn string_text_inside_a_macro_is_not_code() {
    let facts = rust_facts("fn s() { println!(\"helper(1) is text\"); }");
    assert!(calls_to(&facts, "helper").is_empty(), "{:?}", facts.calls);
}

#[test]
fn pattern_macros_read_only_their_scrutinee() {
    let facts = rust_facts("fn m(v: u8) -> bool { assert!(matches!(kind(v), Foo(_) | Bar)) }");
    let kind = calls_to(&facts, "kind");
    assert_eq!(kind.len(), 1, "{:?}", facts.calls);
    assert_eq!(kind[0].macros, ["assert", "matches"]);
    assert!(calls_to(&facts, "Foo").is_empty(), "{:?}", facts.calls);
    assert!(calls_to(&facts, "Bar").is_empty(), "{:?}", facts.calls);
    // `matches!` itself is a call the `assert!` body holds.
    let matches = calls_to(&facts, "matches");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].macros, ["assert"]);
}

#[test]
fn dsl_macros_are_not_admitted() {
    let facts =
        rust_facts("fn t() { async_assert_fn!(tokio::fs::read(&str): Send & Sync & !Unpin); }");
    let callees = facts
        .calls
        .iter()
        .map(|call| call.callee.as_str())
        .collect::<Vec<_>>();
    assert_eq!(callees, ["async_assert_fn"]);
}

#[test]
fn an_admitted_body_with_a_syntax_error_is_dropped_whole() {
    let facts = rust_facts("fn e() { assert!(x => y(z)); }");
    assert!(calls_to(&facts, "y").is_empty(), "{:?}", facts.calls);
    // A dropped expression body is no macro-expansion gap.
    assert!(
        !facts
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == crate::graph::GraphFactCode::RustMacroExpansion)
    );
}

#[test]
fn stringify_and_codegen_macros_are_not_read() {
    let facts = rust_facts("fn q() { stringify!(foo(x)); let _ = quote!{ foo(#x) }; }");
    assert!(calls_to(&facts, "foo").is_empty(), "{:?}", facts.calls);
}

#[test]
fn a_non_admitted_macro_nested_in_an_admitted_one_stays_opaque() {
    let facts = rust_facts("fn n(cx: u8) { ready!(trace_op!(\"n\", poll_proceed(cx))); }");
    assert!(
        calls_to(&facts, "poll_proceed").is_empty(),
        "{:?}",
        facts.calls
    );
    let trace = calls_to(&facts, "trace_op");
    assert_eq!(trace.len(), 1, "{:?}", facts.calls);
    assert_eq!(trace[0].macros, ["ready"]);
}

#[test]
fn expression_macros_inside_item_macro_bodies_are_read() {
    let facts = rust_facts("cfg_rt! { fn g() { ready!(h()); } }");
    let g = facts
        .declarations
        .iter()
        .find(|declaration| declaration.name == "g")
        .expect("g");
    let h = calls_to(&facts, "h");
    assert_eq!(h.len(), 1, "{:?}", facts.calls);
    assert_eq!(h[0].caller_id.as_deref(), Some(g.id.as_str()));
    assert_eq!(h[0].macros, ["ready"]);
    assert_eq!(unit_adapter(&facts, h[0]), "rust-expression-macro-body");
}

#[test]
fn a_same_file_macro_rules_shadows_the_std_name() {
    let facts = rust_facts(
        "macro_rules! ready { ($e:expr) => { stringify!($e) } }\nfn f() { ready!(h()); }",
    );
    assert!(calls_to(&facts, "h").is_empty(), "{:?}", facts.calls);
    // A qualified spelling names the std macro, not the local one.
    let facts = rust_facts(
        "macro_rules! ready { ($e:expr) => { stringify!($e) } }\nfn f() { std::task::ready!(h()); }",
    );
    assert_eq!(calls_to(&facts, "h").len(), 1, "{:?}", facts.calls);
    // A rule that invokes `ready!` declares no `ready` macro.
    let facts =
        rust_facts("macro_rules! wrap { ($e:expr) => { ready!($e) } }\nfn f() { ready!(h()); }");
    assert_eq!(calls_to(&facts, "h").len(), 1, "{:?}", facts.calls);
}

#[test]
fn a_use_binding_of_a_std_macro_name_shadows_it() {
    // A repo macro imported under the std name, a crate path or another
    // crate's macro may expand differently: its arguments are no calls.
    for source in [
        "macro_rules! my_ready { ($e:expr) => { stringify!($e) } }\npub(crate) use my_ready as ready;\nfn f() { ready!(h()); }",
        "use crate::m::ready;\nfn f() { ready!(h()); }",
        "use pretty_assertions::assert_eq;\nfn f() { assert_eq!(h(), 1); }",
    ] {
        let facts = rust_facts(source);
        assert!(
            calls_to(&facts, "h").is_empty(),
            "{source}: {:?}",
            facts.calls
        );
    }
    // A std or futures binding of the same name is the admitted macro.
    for source in [
        "use std::task::ready;\nfn f() { ready!(h()); }",
        "use std::task::{Context, ready};\nfn f() { ready!(h()); }",
        "use futures::ready;\nfn f() { ready!(h()); }",
    ] {
        let facts = rust_facts(source);
        assert_eq!(
            calls_to(&facts, "h").len(),
            1,
            "{source}: {:?}",
            facts.calls
        );
    }
}

#[test]
fn calls_in_a_macro_of_a_recovered_declaration_stay_recovered() {
    let facts = rust_facts(
        "fn broken(x: u8) -> u8 {\n    let y = match x { 1 => , };\n    plain();\n    assert!(f(y));\n    ready!(g(assert_eq!(h(), 1)));\n    x\n}\nfn clean() { assert!(k()); }\n",
    );
    let recovered = |call: &GraphFactCall| {
        matches!(
            call.parse_assessment,
            crate::graph::GraphParseAssessment::Recovered { .. }
        )
    };
    for callee in ["plain", "assert", "f", "g", "assert_eq", "h"] {
        let calls = calls_to(&facts, callee)
            .into_iter()
            .filter(|call| call.caller == "broken")
            .collect::<Vec<_>>();
        assert_eq!(calls.len(), 1, "{callee}: {:?}", facts.calls);
        assert!(recovered(calls[0]), "{callee}: {:?}", calls[0]);
    }
    let f = calls_to(&facts, "f")[0];
    assert!(
        facts
            .argument_sites
            .iter()
            .filter(|site| site.call_id.as_deref() == Some(f.id.as_str()))
            .all(|site| matches!(
                site.parse_assessment,
                crate::graph::GraphParseAssessment::Recovered { .. }
            )),
        "{:?}",
        facts.argument_sites
    );
    let k = calls_to(&facts, "k");
    assert_eq!(k.len(), 1, "{:?}", facts.calls);
    assert!(!recovered(k[0]), "{:?}", k[0]);
}

#[test]
fn a_macro_argument_call_is_neither_a_path_nor_a_reference() {
    let source = "fn b(x: u8) -> u8 { x }\nfn f(cx: u8) { ready!(crate::a::b(cx)); }\n";
    let facts = rust_facts(source);
    assert_eq!(
        calls_to(&facts, "crate::a::b").len(),
        1,
        "{:?}",
        facts.calls
    );
    assert!(
        !facts
            .paths
            .iter()
            .any(|path| path.path == "crate::a::b" && path.line == 2),
        "{:?}",
        facts.paths
    );
    assert_eq!(reference_count(source, "src/lib.rs", "b"), 0);
    // A path argument that is not called stays a path.
    let value = rust_facts("fn f() { assert!(ok(crate::a::LIMIT)); }");
    assert!(
        value
            .paths
            .iter()
            .any(|path| path.path == "crate::a::LIMIT"),
        "{:?}",
        value.paths
    );
}

#[test]
fn macro_chains_round_trip_and_bump_the_schema() {
    assert_eq!(crate::signatures::GRAPH_FACTS_SCHEMA_VERSION, 12);
    let facts = rust_facts("fn f(cx: u8) { ready!(poll(cx)); poll(cx); }");
    let json = serde_json::to_value(&facts).expect("json");
    let calls = json["calls"].as_array().expect("calls");
    let macro_call = calls
        .iter()
        .find(|call| call["callee"] == "poll" && call.get("macros").is_some())
        .expect("macro call");
    assert_eq!(macro_call["macros"], serde_json::json!(["ready"]));
    // A plain call carries no `macros` key.
    assert!(
        calls
            .iter()
            .any(|call| call["callee"] == "poll" && call.get("macros").is_none())
    );
    let back: GraphFactsDocument = serde_json::from_value(json).expect("round trip");
    assert_eq!(back, facts);
}
