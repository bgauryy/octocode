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
fn distinguishes_declaration_and_specifier_level_type_imports() {
    let src =
        "import type { Whole } from './whole';\nimport { type Shape, value } from './mixed';\n";
    let v = graph(src, "main.ts");
    let imports = v["imports"].as_array().unwrap();
    let kinds = imports
        .iter()
        .map(|item| {
            (
                item["localName"].as_str().unwrap(),
                item["importKind"].as_str().unwrap(),
            )
        })
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(kinds.get("Whole"), Some(&"type"));
    assert_eq!(kinds.get("Shape"), Some(&"type"));
    assert_eq!(kinds.get("value"), Some(&"value"));
}

#[test]
fn extracts_string_literal_dynamic_import_as_a_dynamic_import_call() {
    // A dynamic `import('./mod.js')` with a string-literal source must be
    // captured so the dead-code graph can treat the target as reachable —
    // previously invisible, causing a false-positive "dead" verdict on files
    // reached only through a dynamic import.
    let src =
        "export async function loadPlugin() {\n  const mod = await import('./plugin.js');\n  return mod;\n}\n";
    let v = graph(src, "loader.ts");
    let calls = v["calls"].as_array().unwrap();
    let dynamic_import = calls
        .iter()
        .find(|call| call["kind"] == "dynamic-import")
        .unwrap_or_else(|| panic!("expected a dynamic-import call, got: {calls:?}"));
    assert_eq!(dynamic_import["callee"], "./plugin.js");
    assert_eq!(dynamic_import["caller"], "loadPlugin");
}

#[test]
fn does_not_synthesize_a_dynamic_import_for_a_computed_specifier() {
    // A non-literal specifier can't be resolved to a file statically — it
    // must not be silently treated as either reachable or dead. Scope is
    // deliberately limited to string-literal specifiers only.
    let src = "export async function loadPlugin(name) {\n  return await import(name);\n}\n";
    let v = graph(src, "loader.ts");
    let calls = v["calls"].as_array().unwrap();
    assert!(
        !calls.iter().any(|call| call["kind"] == "dynamic-import"),
        "computed specifier must not produce a dynamic-import fact: {calls:?}"
    );
}

#[test]
fn captures_a_dynamic_import_inside_a_bare_callback_argument() {
    // The common test-framework shape: `it('...', async () => { ... })` passes
    // the arrow function as a bare call argument, not a named declaration or an
    // IIFE callee. A dynamic import made inside that callback's body must still
    // be captured — previously any function/arrow expression found as a plain
    // sub-expression (a callback argument, an array element, a conditional
    // branch) was skipped entirely, silently dropping every call made inside
    // it, including a `dynamic-import`.
    let src = "it('loads', async () => {\n  const { loadConfig } = await import('./loader.js');\n  loadConfig();\n});\n";
    let v = graph(src, "loader.test.ts");
    let calls = v["calls"].as_array().unwrap();
    let dynamic_import = calls
        .iter()
        .find(|call| call["kind"] == "dynamic-import")
        .unwrap_or_else(|| panic!("expected a dynamic-import call, got: {calls:?}"));
    assert_eq!(dynamic_import["callee"], "./loader.js");
}

#[test]
fn captures_calls_inside_a_destructured_dynamic_import_declarator() {
    // `const { x } = await import(...)` — the binding pattern is an
    // ObjectPattern, not a plain identifier, so the module-level variable
    // walker has no single owner name for it. It must still walk the init
    // expression rather than skip the whole declarator, or the dynamic-import
    // fact (and file-level reachability of its target) is lost entirely.
    let src =
        "export async function main() {\n  const { run } = await import('./plugin.js');\n  run();\n}\n";
    let v = graph(src, "entry.ts");
    let calls = v["calls"].as_array().unwrap();
    assert!(
        calls
            .iter()
            .any(|call| call["kind"] == "dynamic-import" && call["callee"] == "./plugin.js"),
        "expected a dynamic-import call for a destructured declarator, got: {calls:?}"
    );
}

#[test]
fn captures_a_call_inside_a_map_callback_argument() {
    // Non-import calls inside a bare callback argument (`array.map(x => ...)`)
    // must also survive — this is the same code path as the dynamic-import
    // callback case above, exercised for an ordinary function call.
    let src = "export function run(items) {\n  return items.map(x => helper(x));\n}\nfunction helper(x) {\n  return x;\n}\n";
    let v = graph(src, "run.ts");
    let calls = v["calls"].as_array().unwrap();
    let callees = calls
        .iter()
        .map(|call| call["callee"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(
        callees.contains(&"helper"),
        "callee list missing call made inside a map() callback: {callees:?}"
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

#[test]
fn deeply_nested_parens_do_not_crash_symbol_extraction() {
    // oxc's recursive-descent parser (and this module's own recursive AST
    // walkers) can blow the default native stack on pathologically nested
    // input — a fault `catch_unwind` cannot intercept, unlike a parser panic.
    // `run_on_deep_stack` moves the parse+walk to a thread with a much larger
    // stack; this must survive depth that would SIGSEGV a default-size one.
    let depth = 5_000;
    let src = format!(
        "function f() {{ return {}1{}; }}",
        "(".repeat(depth),
        ")".repeat(depth)
    );
    let _ = extract_js_symbols(&src, "deep.js");
    let _ = extract_graph_facts(&src, "deep.js");
    let _ = find_in_file_references(&src, "deep.js", 0, 9);
}

#[test]
fn export_forms_preserve_type_kind_sources_and_arrow_calls() {
    let value = graph(
        r#"
export interface Shape { size: number }
export type Label = string;
const local = 1;
export { local as renamed };
export type { Shape as PublicShape };
export { remote as forwarded } from './remote';
export type { Model } from './model';
export const expression = () => target();
export const block = () => { target(); };
"#,
        "exports.ts",
    );
    assert_eq!(value["diagnostics"], serde_json::json!([]));
    let exports = value["exports"].as_array().unwrap();
    for (name, kind, source) in [
        ("Shape", "type", None),
        ("Label", "type", None),
        ("renamed", "value", None),
        ("PublicShape", "type", None),
        ("forwarded", "value", Some("./remote")),
        ("Model", "type", Some("./model")),
    ] {
        let item = exports
            .iter()
            .find(|item| item["name"] == name)
            .expect(name);
        assert_eq!(item["exportKind"], kind, "{name}");
        assert_eq!(item["source"].as_str(), source, "{name}");
    }
    let calls = value["calls"].as_array().unwrap();
    for caller in ["expression", "block"] {
        assert!(
            calls
                .iter()
                .any(|call| call["caller"] == caller && call["callee"] == "target"),
            "{caller}: {calls:?}"
        );
    }
}

#[test]
fn namespace_external_module_and_global_augmentation_keep_children() {
    let value = symbols(
        r#"
namespace Outer.Inner { export function nested() {} }
declare module "external" { export function api(): void; }
declare global { interface Window { custom: string } }
"#,
        "namespaces.d.ts",
    );
    assert_eq!(names(&value), ["Outer", "external", "global"]);
    assert_eq!(names(&value[0]["children"]), ["Inner"]);
    assert_eq!(names(&value[0]["children"][0]["children"]), ["nested"]);
    assert_eq!(names(&value[1]["children"]), ["api"]);
    assert_eq!(names(&value[2]["children"]), ["Window"]);
}
