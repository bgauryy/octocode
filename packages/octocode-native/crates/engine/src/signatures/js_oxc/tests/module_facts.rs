//! Imports, exports, dynamic imports and the bindings exports name.

use super::*;

#[test]
fn extracts_graph_facts_for_imports_exports_and_calls() {
    let src = "import { dep } from './dep';\nexport function run() {\n  dep();\n  helper();\n}\nfunction helper() {}\n";
    let v = graph(src, "main.ts");
    assert_eq!(
        v["schemaVersion"],
        crate::signatures::GRAPH_FACTS_SCHEMA_VERSION
    );

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
fn declarations_only_matches_full_graph_facts_declarations() {
    let source = "import { a } from './a';\nexport class Panel { draw() { return a(); } }\nexport function helper(n) { return n; }\nconst local = () => helper(1);\nexport default function main() { local(); }\n";
    for path in ["mod.ts", "mod.js", "mod.tsx"] {
        let full = graph(source, path);
        let light: Value =
            serde_json::to_value(extract_declarations(source, path).expect("declarations"))
                .expect("json");
        assert_eq!(light["declarations"], full["declarations"], "{path}");
        assert_eq!(light["imports"], full["imports"], "{path}");
        assert_eq!(light["exports"], full["exports"], "{path}");
        assert_eq!(light["calls"], serde_json::json!([]), "{path}");
    }
}
