//! Graph-facts occurrences: export and import ranges, CommonJS loads and
//! call targets.

use super::*;

#[test]
fn export_ranges_include_wrappers_without_moving_symbol_anchors() {
    let source = "export function first(){}\nexport default class Second {}\nexport const left = 1, right = 2;";
    let value: serde_json::Value =
        serde_json::from_str(&extract_graph_facts(source, "exports.ts").unwrap()).unwrap();
    let declarations = value["declarations"].as_array().unwrap();
    for (name, line, column) in [("first", 0, 16), ("Second", 1, 21), ("left", 2, 13)] {
        let d = declarations.iter().find(|d| d["name"] == name).unwrap();
        assert_eq!(
            d["range"]["start"],
            serde_json::json!({"line":line,"character":0})
        );
        assert_eq!(
            d["selectionRange"]["start"],
            serde_json::json!({"line":line,"character":column})
        );
    }
    let right = declarations.iter().find(|d| d["name"] == "right").unwrap();
    assert_eq!(right["range"]["end"]["character"], 33);
}

#[test]
fn exported_declaration_outline_scales_without_losing_aliases() {
    let mut source = (0..20_000)
        .map(|i| format!("export function fn{i}() {{ return {i}; }}\n"))
        .collect::<String>();
    source.push_str("export { fn42 as alias, fn42 as another };\n");
    let facts = extract_declarations(&source, "many-exports.ts").unwrap();
    let value = serde_json::to_value(facts).unwrap();
    let declarations = value["declarations"].as_array().unwrap();
    assert_eq!(declarations.len(), 20_000);
    assert!(declarations.iter().all(|row| row["exported"] == true));
    assert_eq!(
        declarations[42]["exportedAs"],
        serde_json::json!(["alias", "another", "fn42"])
    );
    assert_eq!(declarations.last().unwrap()["name"], "fn19999");
}

#[test]
fn import_ranges_do_not_invent_synthetic_name_tokens() {
    let value: serde_json::Value = serde_json::from_str(&extract_graph_facts("import value from './a'; import * as namespace from './b'; import { plain } from './c'; import './side';", "imports.ts").unwrap()).unwrap();
    let imports = value["imports"].as_array().unwrap();
    for import in &imports[..2] {
        assert!(import.get("importedRange").is_none());
        assert!(import.get("localRange").is_some());
    }
    assert_eq!(imports[2]["localRange"], imports[2]["importedRange"]);
    assert!(imports[3].get("importedRange").is_none());
    assert!(imports[3].get("localRange").is_none());
}

#[test]
fn named_import_binding_ranges_are_exact_utf16() {
    let value: serde_json::Value = serde_json::from_str(&extract_graph_facts("const marker = \"😀\"; import { target as first, target as second } from './origin';\nimport {\n target as third\n} from './origin';\n", "aliases.ts").unwrap()).unwrap();
    let imports = value["imports"].as_array().unwrap();
    assert_eq!(imports.len(), 3);
    for (index, (line, imported, local, length)) in [(0, 30, 40, 5), (0, 47, 57, 6), (2, 1, 11, 5)]
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

fn common_js(source: &str) -> serde_json::Value {
    let facts: serde_json::Value =
        serde_json::from_str(&extract_graph_facts(source, "entry.ts").unwrap()).unwrap();
    facts["commonJs"].clone()
}

#[test]
fn commonjs_links_literal_loads_with_scope_provenance() {
    let loads = common_js("const value = require('./value.cjs'); module.require('./other.cjs');");
    assert_eq!(loads[0]["specifier"], "./value.cjs");
    assert_eq!(loads[0]["binding"], "unshadowed-global");
    assert_eq!(loads[0]["kind"], "commonjs-require");
    assert_eq!(loads[1]["specifier"], "./other.cjs");
}

#[test]
fn commonjs_recall_holds_on_generated_loader_corpora() {
    let plain = (0..180)
        .map(|i| format!("const dep{i} = require('./dep{i}.cjs');\n"))
        .collect::<String>();
    assert_eq!(common_js(&plain).as_array().unwrap().len(), 180);
    let escaped = (0..120)
        .map(|i| {
            format!(
                r#"function task{i}() {{ const name = './dep{i}.cjs'; return requ\u0069re(name); }}"#
            )
        })
        .collect::<String>();
    assert_eq!(common_js(&escaped).as_array().unwrap().len(), 120);
}

#[test]
fn commonjs_does_not_link_shadowed_loader_names() {
    let loads = common_js(
        "function run(require, module) { require('./fake'); module.require('./fake'); } const text = \"require('./fake')\";",
    );
    assert!(loads.as_array().unwrap().is_empty());
}

#[test]
fn commonjs_covers_nested_escaped_and_computed_loader_syntax() {
    let loads = common_js(
        r#"function nested() { requ\u0069re('./nested.cjs'); } module['require']('./computed.cjs');"#,
    );
    assert_eq!(loads.as_array().unwrap().len(), 2);
    assert_eq!(loads[0]["specifier"], "./nested.cjs");
    assert_eq!(loads[1]["specifier"], "./computed.cjs");
}

#[test]
fn commonjs_precheck_preserves_template_and_escaped_string_loaders() {
    for source in [
        "const text = `result: ${require /* comment */ ('./value.cjs')}`;",
        "module['require']('./value.cjs');",
        r#"module['requ\ire']('./value.cjs');"#,
        r#"import { 'creat\u0065Require' as factory } from 'node:module'; const load = factory(import.meta.url); load('./value.cjs');"#,
    ] {
        let loads = common_js(source);
        assert_eq!(loads.as_array().unwrap().len(), 1, "{source}");
        assert_eq!(loads[0]["specifier"], "./value.cjs", "{source}");
    }
}

#[test]
fn commonjs_rejects_nonlocal_create_require_and_mutated_constants() {
    let loads = common_js(
        "import { createRequire } from 'node:module'; const load = createRequire('/elsewhere/index.js'); load('./value.cjs'); const target = './old.cjs'; target = changed; require(target);",
    );
    assert_eq!(loads[0]["reason"], "non-local-loader-base");
    assert!(loads[0].get("specifier").is_none());
    assert!(loads[1].get("specifier").is_none());
}

#[test]
fn commonjs_tracks_create_require_and_constant_specifiers_by_binding() {
    let loads = common_js(
        "import { createRequire as makeLoader } from 'node:module'; const load = makeLoader(import.meta.url); const target = './value.cjs'; load(target); function nested(target) { load(target); }",
    );
    assert_eq!(loads[0]["specifier"], "./value.cjs");
    assert_eq!(loads[0]["binding"], "create-require");
    assert!(loads[1].get("specifier").is_none());
    assert_eq!(loads[1]["reason"], "non-literal-specifier");
}

#[test]
fn commonjs_keeps_dynamic_and_reassigned_loader_coverage_explicit() {
    let loads = common_js("require(name); require = replacement; require('./value.cjs');");
    assert_eq!(loads.as_array().unwrap().len(), 2);
    assert!(loads[0].get("specifier").is_none());
    assert!(loads[1].get("specifier").is_none());
    assert_eq!(loads[1]["reason"], "loader-reassigned");
}

#[test]
fn commonjs_member_writes_preserve_prior_calls_and_reject_later_calls() {
    for target in [
        "module.require",
        "module['require']",
        r#"module['requ\u0069re']"#,
    ] {
        let loads = common_js(&format!(
            "module.require('./before.cjs'); {target} = module.require('./rhs.cjs'); module.require('./after.cjs'); module['require']('./computed-after.cjs');"
        ));
        assert_eq!(loads.as_array().unwrap().len(), 4, "{target}");
        assert_eq!(loads[0]["specifier"], "./before.cjs", "{target}");
        assert_eq!(loads[1]["specifier"], "./rhs.cjs", "{target}");
        for load in &loads.as_array().unwrap()[2..] {
            assert!(load.get("specifier").is_none(), "{target}: {load}");
            assert_eq!(load["reason"], "loader-reassigned", "{target}");
        }
    }
}

#[test]
fn commonjs_shadowed_member_writes_do_not_poison_global_loader() {
    let loads = common_js(
        "function replace(module) { module.require = replacement; module['require'] = replacement; } module.require('./real.cjs');",
    );
    assert_eq!(loads.as_array().unwrap().len(), 1);
    assert_eq!(loads[0]["specifier"], "./real.cjs");
}

#[test]
fn commonjs_member_writes_do_not_assume_deferred_call_order() {
    for source in [
        "function later() { module.require('./fake.cjs'); } module.require = replacement; later();",
        "module.require = () => module.require('./fake.cjs'); module.require();",
        "function replace() { module.require = replacement; } replace(); module.require('./fake.cjs');",
    ] {
        let loads = common_js(source);
        for load in loads.as_array().unwrap() {
            assert!(load.get("specifier").is_none(), "{source}: {load}");
            assert_eq!(load["reason"], "loader-reassigned", "{source}");
        }
    }
}

#[test]
fn ambient_declaration_files_do_not_report_missing_initializers() {
    for path in ["index.d.ts", "index.d.mts", "index.d.cts"] {
        let value: serde_json::Value = serde_json::from_str(
            &extract_graph_facts(
                "export const value: string; export function read(): string;",
                path,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(value["diagnostics"], serde_json::json!([]), "{path}");
        assert!(
            value["declarations"]
                .as_array()
                .unwrap()
                .iter()
                .any(|declaration| declaration["name"] == "value")
        );
    }
    let value: serde_json::Value = serde_json::from_str(
        &extract_graph_facts("export const value: string;", "index.ts").unwrap(),
    )
    .unwrap();
    assert!(!value["diagnostics"].as_array().unwrap().is_empty());
}

#[test]
fn equal_method_names_have_distinct_occurrences_and_unresolved_call_targets() {
    let value: serde_json::Value = serde_json::from_str(&extract_graph_facts(
        "export function run() {} class A { run() { work(); work(); } } class B { run() { work(); } }",
        "names.ts",
    ).unwrap()).unwrap();
    let declarations = value["declarations"].as_array().unwrap();
    let ids: std::collections::HashSet<_> = declarations
        .iter()
        .map(|d| d["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids.len(), declarations.len());
    assert_eq!(
        declarations
            .iter()
            .filter(|d| d["name"] == "run" && d["exported"] == true)
            .count(),
        1
    );
    let calls = value["calls"].as_array().unwrap();
    let call_ids: std::collections::HashSet<_> =
        calls.iter().map(|c| c["id"].as_str().unwrap()).collect();
    assert_eq!(call_ids.len(), calls.len());
    let mut callers = std::collections::HashSet::new();
    for edge in value["edges"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["relation"] == "calls")
    {
        assert!(ids.contains(edge["from"].as_str().unwrap()));
        callers.insert(edge["from"].as_str().unwrap());
        assert!(edge["to"].as_str().unwrap().starts_with("reference:"));
        assert_eq!(edge["resolution"], "unresolved");
    }
    assert_eq!(callers.len(), 2);
}
