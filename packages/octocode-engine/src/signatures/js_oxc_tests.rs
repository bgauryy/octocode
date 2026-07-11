use super::*;
use serde_json::Value;

fn symbols(content: &str, path: &str) -> Value {
    let json = extract_js_symbols(content, path).expect("symbols expected");
    serde_json::from_str(&json).expect("valid json")
}

fn graph(content: &str, path: &str) -> Value {
    let json = extract_graph_facts(content, path).expect("graph facts expected");
    serde_json::from_str(&json).expect("valid json")
}

fn names(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn extracts_functions_classes_and_members() {
    let src = "export function add(a: number, b: number): number {\n  return a + b;\n}\n\nexport class Calc {\n  value = 0;\n  multiply(x: number) {\n    return this.value * x;\n  }\n  constructor() {}\n}\n";
    let v = symbols(src, "calc.ts");
    let top = names(&v);
    assert!(top.contains(&"add".to_string()), "function: {top:?}");
    assert!(top.contains(&"Calc".to_string()), "class: {top:?}");

    let calc = v
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "Calc")
        .unwrap();
    assert_eq!(calc["kind"], 5, "class kind");
    let members = names(&calc["children"]);
    assert!(members.contains(&"value".to_string()), "field: {members:?}");
    assert!(
        members.contains(&"multiply".to_string()),
        "method: {members:?}"
    );
    assert!(
        members.contains(&"constructor".to_string()),
        "ctor: {members:?}"
    );
}

#[test]
fn extracts_graph_facts_for_imports_exports_and_calls() {
    let src = "import { dep } from './dep';\nexport function run() {\n  dep();\n  helper();\n}\nfunction helper() {}\n";
    let v = graph(src, "main.ts");

    let declarations = v["declarations"].as_array().unwrap();
    let run = declarations.iter().find(|d| d["name"] == "run").unwrap();
    assert_eq!(run["kind"], "function");
    assert_eq!(run["exported"], true);

    let imports = v["imports"].as_array().unwrap();
    assert_eq!(imports[0]["specifier"], "./dep");
    assert_eq!(imports[0]["localName"], "dep");

    let calls = v["calls"].as_array().unwrap();
    let callees = calls
        .iter()
        .map(|call| call["callee"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(callees.contains(&"dep"), "callee list: {callees:?}");
    assert!(callees.contains(&"helper"), "callee list: {callees:?}");
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
fn extracts_interface_enum_typealias_namespace() {
    let src = "export interface User {\n  id: string;\n  greet(): void;\n}\n\nexport enum Color { Red, Green }\n\nexport type Id = string;\n\nexport namespace NS {\n  export function inner() {}\n}\n";
    let v = symbols(src, "types.ts");
    let top = names(&v);
    for expected in ["User", "Color", "Id", "NS"] {
        assert!(top.contains(&expected.to_string()), "{expected} in {top:?}");
    }
    let user = v
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "User")
        .unwrap();
    assert_eq!(user["kind"], 11, "interface kind");
    let members = names(&user["children"]);
    assert!(members.contains(&"id".to_string()));
    assert!(members.contains(&"greet".to_string()));

    let ns = v
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "NS")
        .unwrap();
    assert!(names(&ns["children"]).contains(&"inner".to_string()));
}

#[test]
fn arrow_const_is_a_function_const_value_is_constant() {
    let src = "export const handler = (req) => req;\nexport const MAX = 10;\nlet counter = 0;\n";
    let v = symbols(src, "h.js");
    let arr = v.as_array().unwrap();
    let handler = arr.iter().find(|s| s["name"] == "handler").unwrap();
    assert_eq!(handler["kind"], 12, "arrow → function");
    let max = arr.iter().find(|s| s["name"] == "MAX").unwrap();
    assert_eq!(max["kind"], 14, "const → constant");
    let counter = arr.iter().find(|s| s["name"] == "counter").unwrap();
    assert_eq!(counter["kind"], 13, "let → variable");
}

#[test]
fn ranges_are_zero_based() {
    let src = "function first() {}\nfunction second() {}\n";
    let v = symbols(src, "a.ts");
    let first = &v.as_array().unwrap()[0];
    assert_eq!(first["range"]["start"]["line"], 0, "0-based first line");
    let second = v
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "second")
        .unwrap();
    assert_eq!(second["range"]["start"]["line"], 1);
}

#[test]
fn tsx_and_jsx_parse() {
    let src = "export function App() {\n  return <div>hi</div>;\n}\n";
    let v = symbols(src, "App.tsx");
    assert!(names(&v).contains(&"App".to_string()));
}

#[test]
fn empty_or_dataless_returns_none() {
    assert!(extract_js_symbols("", "empty.ts").is_none());
    // A hard parse failure must not abort; it returns None or a best-effort
    // outline — either is acceptable, just never a panic.
    let _ = extract_js_symbols("const x = 1 +;", "broken.ts");
}

fn refs(content: &str, path: &str, line: u32, character: u32) -> Value {
    let json =
        find_in_file_references(content, path, line, character).expect("references expected");
    serde_json::from_str(&json).expect("valid json")
}

#[test]
fn finds_in_file_references_from_declaration() {
    // `count` declared on line 0; used on lines 1 and 2.
    let src = "const count = 1;\nconst a = count + 1;\nconsole.log(count);\n";
    // Cursor on the declaration identifier `count` (line 0, char 6).
    let v = refs(src, "m.ts", 0, 6);
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 3, "declaration + 2 uses: {arr:?}");
    // First range is the declaration (line 0).
    assert_eq!(arr[0]["start"]["line"], 0);
    let lines: Vec<i64> = arr
        .iter()
        .map(|r| r["start"]["line"].as_i64().unwrap())
        .collect();
    assert!(lines.contains(&1) && lines.contains(&2), "uses: {lines:?}");
}

#[test]
fn finds_references_from_a_use_site() {
    let src = "function greet(name) {\n  return name + name;\n}\n";
    // Cursor on a `name` use inside the body (line 1).
    let v = refs(src, "m.js", 1, 9);
    let arr = v.as_array().unwrap();
    assert!(arr.len() >= 2, "param + uses: {arr:?}");
}

#[test]
fn references_none_off_symbol() {
    let src = "const x = 1;\n";
    // Cursor in whitespace / on a keyword, not a binding.
    assert!(find_in_file_references(src, "m.ts", 0, 0).is_none());
}

#[test]
fn never_aborts_on_adversarial_input() {
    for src in [
        "function broken( { [ unterminated",
        "class { { { {",
        "\u{0}\u{0}\u{0}",
        "import type type from from",
    ] {
        let _ = extract_js_symbols(src, "x.ts");
    }
}
