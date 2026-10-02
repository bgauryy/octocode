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
    assert_eq!(v["schemaVersion"], 1);

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
fn destructured_exports_emit_each_binding_name() {
    // Regression: `export const {a,b} = …` / `export const [x] = …` previously
    // dropped every binding because only bare identifiers were collected.
    let src = "export const { a, b } = obj;\nexport const [x, [y]] = arr;\nexport const { c: { d }, ...rest } = obj;\n";
    let v = graph(src, "main.ts");
    let exports = names(&v["exports"]);
    for expected in ["a", "b", "x", "y", "d", "rest"] {
        assert!(
            exports.contains(&expected.to_string()),
            "expected {expected:?} among exports: {exports:?}"
        );
    }
}

#[test]
fn export_assignment_and_import_equals_are_captured() {
    // Regression: `export = x` and `import x = require(...)` were dropped by the
    // graph-facts statement walker.
    let src = "import util = require('./util');\nexport = util;\n";
    let v = graph(src, "main.ts");
    let exports = names(&v["exports"]);
    assert!(
        exports.contains(&"util".to_string()),
        "export= must emit its name: {exports:?}"
    );
    let imports = v["imports"].as_array().unwrap();
    let specifiers = imports
        .iter()
        .map(|i| i["specifier"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(
        specifiers.contains(&"./util"),
        "import= must emit its specifier: {specifiers:?}"
    );
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
    let src = "export async function loadPlugin() {\n  const mod = await import('./plugin.js');\n  return mod;\n}\n";
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
    let src = "export async function main() {\n  const { run } = await import('./plugin.js');\n  run();\n}\n";
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

#[test]
fn jsx_in_plain_js_files_parses() {
    // React components commonly live in `.js`; JSX must not be a parse error.
    let src = "export function App() {\n  return <div className=\"a\">hi</div>;\n}\n";
    let v = symbols(src, "App.js");
    assert!(names(&v).contains(&"App".to_string()), "{v}");
}

#[test]
fn source_type_follows_the_full_path() {
    let st = |p: &str| {
        source_type_for(
            &crate::text::file_extension::get_extension_internal(p, true, "ts"),
            p,
            "",
        )
    };
    assert!(st("a.cjs").is_commonjs(), ".cjs is CommonJS");
    assert!(
        st("a.cts").is_commonjs() && st("a.cts").is_typescript(),
        ".cts is CommonJS TS"
    );
    assert!(
        st("a.mts").is_module() && st("a.mts").is_typescript(),
        ".mts is an ES module"
    );
    assert!(st("a.mjs").is_module(), ".mjs is an ES module");
    assert!(st("a.js").is_jsx(), ".js allows JSX");
    assert!(!st("a.ts").is_jsx(), ".ts keeps `<T>x` assertions (no JSX)");
    assert!(st("a.tsx").is_jsx() && st("a.tsx").is_typescript());
    assert!(st("types/a.d.ts").is_typescript_definition());
    assert!(st("types/a.d.cts").is_typescript_definition() && st("types/a.d.cts").is_commonjs());
}

#[test]
fn non_js_files_are_rejected_before_any_oxc_work() {
    assert!(!is_oxc_path("main.py"));
    assert!(!is_oxc_path("lib.rs"));
    assert!(is_oxc_path("a.tsx") && is_oxc_path("a.cjs") && is_oxc_path("types/a.d.ts"));
    assert!(extract_graph_facts("def f():\n    pass\n", "main.py").is_none());
}

fn call_pairs(value: &Value) -> Vec<(String, String)> {
    value["calls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|call| {
            (
                call["caller"].as_str().unwrap().to_string(),
                call["callee"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

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
        ("run", "nestedDecl"),
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
        ("prop", "objArrow"),
        ("deep", "deepArrow"),
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
    let line_index = LineIndex::new(src);
    let mut calls = Vec::new();
    super::super::deep_stack::run_as_cancelled_job(|| {
        collect_program_calls(&parsed.program, &line_index, &mut calls, &mut Vec::new());
    });
    assert!(calls.is_empty(), "cancelled walk must not descend");
    collect_program_calls(&parsed.program, &line_index, &mut calls, &mut Vec::new());
    assert_eq!(calls.len(), 3);
}

fn declaration<'v>(facts: &'v Value, name: &str) -> &'v Value {
    facts["declarations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["name"] == name && d.get("parent").is_none())
        .unwrap_or_else(|| panic!("declaration {name}: {facts}"))
}

#[test]
fn renamed_export_flags_the_local_binding_and_keeps_the_public_alias() {
    // `export { foo as bar }` exports local `foo` under public `bar`;
    // the unrelated local `bar` is not exported.
    let src = "export function kept() { return 1 }\nfunction foo() { return 2 }\nfunction bar() { return 3 }\nexport { foo as bar }\n";
    let facts = graph(src, "mod.ts");
    assert_eq!(declaration(&facts, "kept")["exported"], true);
    assert!(declaration(&facts, "kept").get("exportedAs").is_none());
    assert_eq!(declaration(&facts, "foo")["exported"], true);
    assert_eq!(
        declaration(&facts, "foo")["exportedAs"],
        serde_json::json!(["bar"])
    );
    assert_eq!(declaration(&facts, "bar")["exported"], false);
    let export = facts["exports"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "bar")
        .unwrap();
    assert_eq!(export["localName"], "foo");
}

#[test]
fn reexports_do_not_flag_a_same_named_local_declaration() {
    let src = "function helper() {}\nexport { helper } from './other';\n";
    let facts = graph(src, "mod.ts");
    assert_eq!(declaration(&facts, "helper")["exported"], false);
}

#[test]
fn default_export_records_its_local_binding() {
    // `import foo from` binds the module's `default`; the declaration
    // `foo` must carry `default` as its public name.
    let facts = graph(
        "export default function foo() { return 1 }\nexport function other() { return 2 }\n",
        "def.ts",
    );
    let foo = declaration(&facts, "foo");
    assert_eq!(foo["exported"], true);
    assert_eq!(foo["exportedAs"], serde_json::json!(["default"]));
    assert!(declaration(&facts, "other").get("exportedAs").is_none());
    let export = facts["exports"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "default")
        .unwrap();
    assert_eq!(export["localName"], "foo");

    let facts = graph("function value() {}\nexport default value;\n", "def.ts");
    assert_eq!(declaration(&facts, "value")["exported"], true);
    assert_eq!(
        declaration(&facts, "value")["exportedAs"],
        serde_json::json!(["default"])
    );
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

#[test]
fn declarations_only_matches_full_graph_facts_declarations() {
    let source = "import { a } from './a';\nexport class Panel { draw() { return a(); } }\nexport function helper(n) { return n; }\nconst local = () => helper(1);\nexport default function main() { local(); }\n";
    for path in ["mod.ts", "mod.js", "mod.tsx"] {
        let mut full = graph(source, path);
        // Import uses come from the semantic pass the light path skips.
        for import in full["imports"].as_array_mut().expect("imports") {
            import.as_object_mut().expect("import").remove("usedIn");
        }
        let light: Value =
            serde_json::from_str(&extract_declarations(source, path).expect("declarations"))
                .expect("json");
        assert_eq!(light["declarations"], full["declarations"], "{path}");
        assert_eq!(light["imports"], full["imports"], "{path}");
        assert_eq!(light["exports"], full["exports"], "{path}");
        assert_eq!(light["calls"], serde_json::json!([]), "{path}");
    }
}

/// `(kind, caller, callee, has callerId)` for every call fact of `kind`.
fn calls_of_kind(facts: &Value, kind: &str) -> Vec<(String, String, bool)> {
    facts["calls"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|call| call["kind"] == kind)
        .map(|call| {
            (
                call["caller"].as_str().unwrap().to_string(),
                call["callee"].as_str().unwrap().to_string(),
                call.get("callerId").is_some(),
            )
        })
        .collect()
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

/// `(relation, from declaration name, to, line)` for heritage edges.
fn heritage_edges(facts: &Value) -> Vec<(String, String, String, u64)> {
    let declarations = facts["declarations"].as_array().unwrap();
    facts["edges"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|edge| matches!(edge["relation"].as_str(), Some("extends" | "implements")))
        .map(|edge| {
            assert_eq!(edge["source"], "oxc");
            assert_eq!(edge["resolution"], "syntax");
            let from = declarations
                .iter()
                .find(|declaration| declaration["id"] == edge["from"])
                .unwrap_or_else(|| panic!("from is a declaration id: {edge}"));
            (
                edge["relation"].as_str().unwrap().to_string(),
                from["name"].as_str().unwrap().to_string(),
                edge["to"].as_str().unwrap().to_string(),
                edge["line"].as_u64().unwrap(),
            )
        })
        .collect()
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

/// Named functions declared inside function and method bodies are
/// declarations too, parented to their enclosing declaration, as the
/// tree-sitter extractors report nested Python and Rust functions.
#[test]
fn nested_function_declarations_carry_their_parent() {
    let src = "export function outer() {\n  function innerA() { return 1; }\n  const innerB = () => 2;\n  const local = 3;\n  if (local) { function guarded() {} }\n  return innerA() + innerB();\n}\nexport class K {\n  method() { function deep() {} return deep; }\n}\nexport const run = () => [1].map(function callback() { function inCallback() {} });\n";
    let facts: Value =
        serde_json::from_str(&extract_declarations(src, "n.ts").expect("declarations"))
            .expect("json");
    let declarations = facts["declarations"].as_array().expect("declarations");
    let parent_of = |name: &str| {
        let row = declarations
            .iter()
            .find(|d| d["name"] == name)
            .unwrap_or_else(|| panic!("missing {name}: {facts}"));
        let parent = row["parent"].as_str().expect("nested row has a parent");
        declarations
            .iter()
            .find(|d| d["id"] == parent)
            .and_then(|d| d["name"].as_str())
            .expect("parent resolves")
            .to_owned()
    };
    assert_eq!(parent_of("innerA"), "outer");
    assert_eq!(parent_of("innerB"), "outer");
    assert_eq!(parent_of("guarded"), "outer");
    assert_eq!(parent_of("deep"), "method");
    // Locals that are not functions, and expressions that are not
    // declarations, stay out of the outline.
    for absent in ["local", "callback"] {
        assert!(
            declarations.iter().all(|d| d["name"] != absent),
            "{absent}: {facts}"
        );
    }
    // A declaration inside an anonymous callback belongs to the enclosing
    // named declaration.
    assert_eq!(parent_of("inCallback"), "run");
}

/// CommonJS and prototype-style modules define their API by assigning
/// functions to members (`res.redirect = function () {}`,
/// `exports.x = () => {}`): those assignments are declarations spanning the
/// function, so a block read or a declaration outline covers the body.
#[test]
fn member_assigned_functions_are_declarations() {
    let src = "'use strict';\nvar res = module.exports = {};\n\nres.redirect = function redirect(url) {\n  var status = 302;\n  return status;\n};\n\nexports.handler = (req) => {\n  return req;\n};\n\nWidget.prototype.render = Other.render = function () {\n  return 1;\n};\n\nres.count = 1;\nres.once = function () { return 0; };\n";
    let facts: Value =
        serde_json::from_str(&extract_declarations(src, "lib/response.js").expect("declarations"))
            .expect("json");
    let declarations = facts["declarations"].as_array().expect("declarations");
    let span = |name: &str| {
        let row = declarations
            .iter()
            .find(|d| d["name"] == name)
            .unwrap_or_else(|| panic!("missing {name}: {facts}"));
        (
            row["range"]["start"]["line"].as_u64().unwrap() + 1,
            row["range"]["end"]["line"].as_u64().unwrap() + 1,
        )
    };
    assert_eq!(span("res.redirect"), (4, 7));
    assert_eq!(span("exports.handler"), (9, 11));
    assert_eq!(span("Widget.prototype.render"), (13, 15));
    assert_eq!(span("res.once"), (18, 18));
    // A member assigned a plain value is not a declaration.
    assert!(
        declarations.iter().all(|d| d["name"] != "res.count"),
        "{facts}"
    );
}
