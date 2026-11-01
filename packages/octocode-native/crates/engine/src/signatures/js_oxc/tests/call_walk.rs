//! The call visitor: coverage, order, caller identity and cancellation.

use super::*;

#[test]
fn visitor_finds_calls_the_hand_rolled_walker_dropped() {
    let src = r#"
export function run(flag: boolean, items: any[]) {
  switch (flag) {
    case true: {
      function inSwitch() { nestedDecl(); }
    }
  }
  lbl: { labeledCall(); }
  class Local {
    static { staticBlock(); }
    field = fieldInit();
    handler = () => fieldArrow();
  }
  const t = `${tpl(a?.b?.(optionalArg()))}`;
  const s = (seq1(), flag ? cond1() : cond2());
  const o = { prop: () => objArrow(), nested: { deep: () => deepArrow() } };
  try { tryCall(); } catch ({ message = catchDefault() }) {}
  const { d = destructDefault() } = items[0];
}
export default makeDefault();
enum E { A = enumInit() }
namespace N { export function inner() { nsCall(); } }
@decorate() class Decorated { m(x = paramDefault()) { method(); } }
top: { topLabeled(); }
"#;
    let pairs = call_pairs(&graph(src, "dropped.ts"));
    for (caller, callee) in [
        // A nested function declaration owns its calls.
        ("inSwitch", "nestedDecl"),
        ("run", "labeledCall"),
        ("run", "staticBlock"),
        ("run", "fieldInit"),
        ("handler", "fieldArrow"),
        ("run", "tpl"),
        ("run", "a.b"),
        ("run", "optionalArg"),
        ("run", "seq1"),
        ("run", "cond1"),
        ("run", "cond2"),
        // Members of a nested object literal are no declarations: their
        // calls belong to the innermost declaration holding them.
        ("run", "objArrow"),
        ("run", "deepArrow"),
        ("run", "tryCall"),
        ("run", "catchDefault"),
        ("run", "destructDefault"),
        ("default", "makeDefault"),
        ("module", "enumInit"),
        ("inner", "nsCall"),
        ("Decorated", "decorate"),
        ("m", "paramDefault"),
        ("m", "method"),
        ("module", "topLabeled"),
    ] {
        assert!(
            pairs.contains(&(caller.to_string(), callee.to_string())),
            "expected {caller} -> {callee} in {pairs:?}"
        );
    }
}

#[test]
fn call_order_and_owners_match_the_legacy_walker() {
    // Locks the exact output order for shapes the hand-rolled walker
    // supported: call before callee/arguments, parameter default before
    // body, source order otherwise.
    let src = r#"
import { a, b } from './x';
const top = wrap(a(1), () => b(2));
export function run(x = def()) {
  if (check()) { new Thing(x); }
  return tag`t${inner()}` + outer(nested());
}
describe('suite', () => { it('case', () => spec()); });
const { first } = await import('./mod.js');
"#;
    let pairs = call_pairs(&graph(src, "order.ts"));
    let expected = [
        ("top", "wrap"),
        ("top", "a"),
        ("top", "b"),
        ("run", "def"),
        ("run", "check"),
        ("run", "Thing"),
        ("run", "tag"),
        ("run", "inner"),
        ("run", "outer"),
        ("run", "nested"),
        ("module", "describe"),
        ("module", "it"),
        ("module", "spec"),
        ("first", "./mod.js"),
    ];
    let expected: Vec<(String, String)> = expected
        .iter()
        .map(|(caller, callee)| (caller.to_string(), callee.to_string()))
        .collect();
    assert_eq!(pairs, expected);
}

#[test]
fn call_walk_stops_when_the_job_is_cancelled() {
    let src = "export function run() { a(); b(); c(); }\n";
    let allocator = oxc_allocator::Allocator::default();
    let parsed = oxc_parser::Parser::new(&allocator, src, SourceType::ts()).parse();
    let line_index = SpanPositions::new(src);
    let mut calls = Vec::new();
    crate::deep_stack::run_as_cancelled_job(|| {
        collect_program_calls(&parsed.program, &line_index, &mut calls, &mut Vec::new());
    });
    assert!(calls.is_empty(), "cancelled walk must not descend");
    collect_program_calls(&parsed.program, &line_index, &mut calls, &mut Vec::new());
    assert_eq!(calls.len(), 3);
}

#[test]
fn calls_carry_the_caller_declaration_identity() {
    // A function `run` and a method `run` are distinct callers.
    let src = "export function run() { return publicA() }\nclass Calls { run() { return secret() } }\nsecret();\n";
    let facts = graph(src, "calls.ts");
    let calls = facts["calls"].as_array().unwrap();
    let caller_of = |callee: &str| {
        calls
            .iter()
            .find(|c| c["callee"] == callee && c["caller"] != "module")
            .map(|c| c["callerId"].as_str().unwrap().to_owned())
            .unwrap()
    };
    let run_id = declaration(&facts, "run")["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(caller_of("publicA"), run_id);
    assert_ne!(caller_of("secret"), run_id);
    assert!(caller_of("secret").contains("#run@"));
    let module_call = calls
        .iter()
        .find(|c| c["caller"] == "module")
        .expect("module-level call");
    assert!(module_call.get("callerId").is_none());
}

#[test]
fn top_level_iife_bodies_are_outlined_as_module_scope() {
    for (wrapper, path) in [
        ("(function(){\n%\n})();", "umd.js"),
        ("(() => {\n%\n})();", "arrow.js"),
        ("!function(){\n%\n}();", "bang.js"),
        ("(function(){\n%\n}).call(this);", "call.js"),
    ] {
        let source = wrapper.replace(
            '%',
            "var FBL = {};\nfunction helper(n) { return n; }\nclass Panel {}",
        );
        let got = names(&symbols(&source, path));
        for expected in ["FBL", "helper", "Panel"] {
            assert!(got.iter().any(|name| name == expected), "{path}: {got:?}");
        }
        let decls = graph(&source, path)["declarations"]
            .as_array()
            .map(Vec::len)
            .unwrap_or(0);
        assert!(decls >= 3, "{path}: graph declarations {decls}");
    }
    // A plain call statement is not a scope.
    let plain = extract_js_symbols("run(function(){ var hidden = 1; });", "call.js");
    assert!(
        plain.as_deref().is_none_or(|json| !json.contains("hidden")),
        "{plain:?}"
    );
}
