//! Call facts in every expression position, JSX renders, decorators and heritage.

use super::*;

#[test]
fn preserves_super_receiver_identity() {
    let facts = graph(
        "class Child extends Base { run() { this.transaction(); super.transaction(); super['transaction'](); } }",
        "receivers.ts",
    );
    let callees = calls_of_kind(&facts, "calls")
        .into_iter()
        .map(|(_, callee, _)| callee)
        .collect::<Vec<_>>();
    assert_eq!(
        callees,
        vec!["this.transaction", "super.transaction", "super.transaction"]
    );
}

#[test]
fn extracts_calls_nested_in_return_binary_and_args() {
    let src = "export function run(x: number) {\n  return helper(x) + other(x);\n}\nfunction helper(n: number) { return n; }\nfunction other(n: number) { return n; }\n";
    let v = graph(src, "nested.ts");
    let callees = v["calls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|call| call["callee"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(
        callees.contains(&"helper"),
        "nested return binary should capture helper: {callees:?}"
    );
    assert!(
        callees.contains(&"other"),
        "nested return binary should capture other: {callees:?}"
    );
    assert!(
        v["calls"]
            .as_array()
            .unwrap()
            .iter()
            .all(|call| call["caller"] == "run"),
        "calls should belong to run: {:?}",
        v["calls"]
    );
}

#[test]
fn extracts_calls_in_logical_conditional_await_and_array() {
    let src = r#"
export async function run(flag: boolean) {
  const a = flag && helper(1);
  const b = flag ? other(2) : helper(3);
  await helper(4);
  return [other(5), ...[helper(6)]];
}
function helper(n: number) { return n; }
function other(n: number) { return n; }
"#;
    let v = graph(src, "nested-more.ts");
    let callees = v["calls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|call| call["callee"].as_str().unwrap())
        .collect::<Vec<_>>();
    for expected in ["helper", "other"] {
        assert!(
            callees.iter().filter(|c| **c == expected).count() >= 1,
            "expected {expected} in {callees:?}"
        );
    }
    assert!(
        callees.len() >= 6,
        "expected nested call sites, got {callees:?}"
    );
}

#[test]
fn extracts_calls_in_switch_try_and_for_of() {
    let src = r#"
export function run(items: number[]) {
  switch (helper(1)) {
    case other(2):
      helper(3);
      break;
  }
  try {
    other(4);
  } catch {
    helper(5);
  } finally {
    other(6);
  }
  for (const x of helper(7)) {
    other(x);
  }
}
function helper(n: number) { return n; }
function other(n: number) { return n; }
"#;
    let v = graph(src, "control.ts");
    let callees = v["calls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|call| call["callee"].as_str().unwrap())
        .collect::<Vec<_>>();
    for expected in ["helper", "other"] {
        assert!(
            callees.contains(&expected),
            "expected {expected} in {callees:?}"
        );
    }
    assert!(
        callees.len() >= 7,
        "expected switch/try/for-of call sites, got {callees:?}"
    );
}

#[test]
fn extracts_calls_in_iife_jsx_defaults_and_tagged_templates() {
    let src = r#"
export function run(x = helper(1)) {
  (function () { other(2); })();
  return helper`ok${other(3)}`;
}
function helper(n: any) { return n; }
function other(n: number) { return n; }
"#;
    let v = graph(src, "extra.ts");
    let callees = v["calls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|call| call["callee"].as_str().unwrap())
        .collect::<Vec<_>>();
    for expected in ["helper", "other"] {
        assert!(
            callees.contains(&expected),
            "expected {expected} in {callees:?}"
        );
    }
}

#[test]
fn jsx_component_usage_is_a_renders_call() {
    let src = "import * as ns from './ns';\nexport function App() {\n  return <>\n    <div><Foo /></div>\n    <Foo.Bar x={make()}>text</Foo.Bar>\n    <ns.Comp />\n    <svg:rect />\n    <my-element />\n  </>;\n}\nrender(<App />, root);\n";
    let facts = graph(src, "app.tsx");
    assert_eq!(
        calls_of_kind(&facts, "renders"),
        vec![
            ("App".to_string(), "Foo".to_string(), true),
            ("App".to_string(), "Foo.Bar".to_string(), true),
            ("App".to_string(), "ns.Comp".to_string(), true),
            ("module".to_string(), "App".to_string(), false),
        ]
    );
    let app_id = &declaration(&facts, "App")["id"];
    let renders = facts["calls"]
        .as_array()
        .unwrap()
        .iter()
        .find(|call| call["kind"] == "renders")
        .unwrap();
    assert_eq!(&renders["callerId"], app_id);
    assert!(
        calls_of_kind(&facts, "calls").contains(&("App".to_string(), "make".to_string(), true)),
        "calls inside JSX attributes stay calls"
    );
    assert!(
        facts["edges"]
            .as_array()
            .unwrap()
            .iter()
            .any(|edge| edge["relation"] == "renders" && &edge["from"] == app_id),
        "renders facts get the same call edge as calls"
    );
}

#[test]
fn bare_decorators_are_decorates_calls() {
    let src = "@Injectable\n@ns.Tag\n@Component({ selector: 'x' })\nexport class Svc {\n  @Input name = '';\n  @HostListener('click') onClick() {}\n}\n";
    let facts = graph(src, "svc.ts");
    let decorates: Vec<(String, String)> = calls_of_kind(&facts, "decorates")
        .into_iter()
        .map(|(caller, callee, _)| (caller, callee))
        .collect();
    assert_eq!(
        decorates,
        vec![
            ("Svc".to_string(), "Injectable".to_string()),
            ("Svc".to_string(), "ns.Tag".to_string()),
            ("Svc".to_string(), "Input".to_string()),
        ]
    );
    let calls: Vec<String> = calls_of_kind(&facts, "calls")
        .into_iter()
        .map(|(_, callee, _)| callee)
        .collect();
    assert_eq!(calls, vec!["Component", "HostListener"]);
}

#[test]
fn class_and_interface_heritage_are_edges() {
    let src = "export class A extends B<T> implements C, ns.D<U> {}\ninterface I extends J, K.L<V> {}\nclass M extends mixin(A) {}\nnamespace N { export class Inner extends outer.Base {} }\nconst Expr = class Named extends A {};\nfunction f() { class Local extends A {} }\n";
    let facts = graph(src, "h.ts");
    let s = |value: &str| value.to_string();
    assert_eq!(
        heritage_edges(&facts),
        vec![
            (s("extends"), s("A"), s("B"), 1),
            (s("implements"), s("A"), s("C"), 1),
            (s("implements"), s("A"), s("ns.D"), 1),
            (s("extends"), s("I"), s("J"), 2),
            (s("extends"), s("I"), s("K.L"), 2),
            (s("extends"), s("Inner"), s("outer.Base"), 4),
            // A class declared in a function body is a nested declaration.
            (s("extends"), s("Local"), s("A"), 6),
        ]
    );
    let js = graph("class A extends React.Component {}\n", "a.js");
    assert_eq!(
        heritage_edges(&js),
        vec![(s("extends"), s("A"), s("React.Component"), 1)]
    );
}

/// A member call keeps its receiver as written: a chained or computed
/// receiver is never dropped, so `this.app.scene.mutate()` cannot read as a
/// bare `mutate()` that the caller's own scope would bind.
#[test]
fn chained_receivers_stay_in_the_callee() {
    let src = "function f() {\n  this.app.scene.mutate();\n  a.b.c();\n  get().run();\n  list[0].pick();\n  new Store().save();\n  (x || y).merge();\n  a?.b.c?.();\n  [1].map(g);\n}\n";
    let facts = graph(src, "chain.ts");
    let callees: Vec<String> = calls_of_kind(&facts, "calls")
        .into_iter()
        .map(|(_, callee, _)| callee)
        .collect();
    assert_eq!(
        callees,
        vec![
            "this.app.scene.mutate",
            "a.b.c",
            "get().run",
            "get",
            "list[].pick",
            "new Store().save",
            "().merge",
            "a.b.c",
            "[].map",
        ]
    );
}
