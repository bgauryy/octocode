//! Document symbols and declarations: kinds, ranges, nesting and parse limits.

use super::*;

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

    // A type alias is a `type`, never an `interface`.
    let facts: Value =
        serde_json::from_str(&extract_graph_facts(src, "types.ts").unwrap()).unwrap();
    let kind_of = |name: &str| {
        facts["declarations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["name"] == name)
            .map(|d| d["kind"].clone())
            .unwrap()
    };
    assert_eq!(kind_of("Id"), "type");
    assert_eq!(kind_of("User"), "interface");
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
fn wrapped_arrow_consts_and_arrow_class_fields_are_callables() {
    let src = "type F = (x: number) => number;\nconst a = (() => 1);\nconst b = (async (x: number) => x) satisfies unknown;\nconst c = ((x) => x) as F;\nconst d = wrap(() => 1);\nclass K { m = () => 1; n = function () {}; v = 1; }\n";
    let v = symbols(src, "w.ts");
    let arr = v.as_array().unwrap();
    let kind = |name: &str| {
        arr.iter()
            .find(|s| s["name"] == name)
            .map(|s| s["kind"].clone())
    };
    for name in ["a", "b", "c"] {
        assert_eq!(kind(name), Some(12.into()), "{name}: {v}");
    }
    assert_eq!(
        kind("d"),
        Some(14.into()),
        "a call result stays a constant: {v}"
    );
    let class = arr.iter().find(|s| s["name"] == "K").unwrap();
    let member = |name: &str| {
        class["children"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == name)
            .map(|s| s["kind"].clone())
    };
    assert_eq!(member("m"), Some(6.into()), "arrow field → method: {v}");
    assert_eq!(member("n"), Some(6.into()), "function field → method: {v}");
    assert_eq!(member("v"), Some(7.into()), "value field → property: {v}");
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
    // Surviving is not enough: the deep parse still names the function.
    assert_eq!(names(&symbols(&src, "deep.js")), ["f"]);
    assert!(graph(&src, "deep.js").is_object());
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
    let st = |p: &str| source_type_for(&crate::text::extension_of(p, true, "ts"), p, "");
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

/// Named functions declared inside function and method bodies are
/// declarations too, parented to their enclosing declaration, as the
/// tree-sitter extractors report nested Python and Rust functions.
#[test]
fn nested_function_declarations_carry_their_parent() {
    let src = "export function outer() {\n  function innerA() { return 1; }\n  const innerB = () => 2;\n  const local = 3;\n  if (local) { function guarded() {} }\n  return innerA() + innerB();\n}\nexport class K {\n  method() { function deep() {} return deep; }\n}\nexport const run = () => [1].map(function callback() { function inCallback() {} });\n";
    let facts: Value =
        serde_json::to_value(extract_declarations(src, "n.ts").expect("declarations"))
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
/// function, named by the property, so a block read or a declaration outline
/// covers the body.
#[test]
fn member_assigned_functions_are_declarations() {
    let src = "'use strict';\nvar res = module.exports = {};\n\nres.redirect = function redirect(url) {\n  var status = 302;\n  return status;\n};\n\nexports.handler = (req) => {\n  return req;\n};\n\nWidget.prototype.render = Other.render = function () {\n  return 1;\n};\n\nres.count = 1;\nres.once = function () { return 0; };\n";
    let facts: Value =
        serde_json::to_value(extract_declarations(src, "lib/response.js").expect("declarations"))
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
    assert_eq!(span("redirect"), (4, 7));
    assert_eq!(span("handler"), (9, 11));
    assert_eq!(span("render"), (13, 15));
    assert_eq!(span("once"), (18, 18));
    // Named like a method: the property is the anchor a language server
    // resolves.
    let redirect = declarations
        .iter()
        .find(|d| d["name"] == "redirect")
        .unwrap();
    assert_eq!(redirect["selectionRange"]["start"]["character"], 4);
    // A member assigned a plain value is not a declaration.
    assert!(declarations.iter().all(|d| d["name"] != "count"), "{facts}");
}

#[test]
fn every_oxc_symbol_kind_is_a_declared_declaration_kind() {
    for kind in 0..=u8::MAX {
        let name = symbol_kind_name(kind);
        assert!(
            crate::signatures::DECLARATION_KINDS.contains(&name),
            "{name} is missing from DECLARATION_KINDS"
        );
    }
}

/// A decorator list written before `export` belongs to the class: its range
/// starts there, and the selection stays on the name.
#[test]
fn a_class_range_starts_at_decorators_before_export() {
    let src = "@Component({})\nexport class C {}\n\n@Injectable()\nexport default class D {}\n";
    let facts = extract_declarations(src, "a.ts").expect("declarations");
    let spans: Vec<(&str, u32, u32)> = facts
        .declarations
        .iter()
        .map(|d| {
            (
                d.name.as_str(),
                d.range.start.line,
                d.selection_range.start.line,
            )
        })
        .collect();
    assert_eq!(spans, vec![("C", 0, 1), ("D", 3, 4)], "{facts:?}");
}

/// An object literal bound to a name declares its function-valued members,
/// as tsserver's navigation tree does: `api.fetch()` has a target.
#[test]
fn object_literal_methods_are_members_of_their_binding() {
    let src = "export const api = {\n  fetch() { return load(); },\n  post: () => 1,\n  put: function () {},\n  value: 1,\n  get size() { return 1; },\n  [key]: () => 1,\n  ...rest,\n} as const;\n";
    let v = symbols(src, "api.ts");
    let api = v
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "api")
        .unwrap();
    assert_eq!(api["kind"], 14, "the binding stays a constant: {v}");
    assert_eq!(names(&api["children"]), ["fetch", "post", "put"]);
    for member in api["children"].as_array().unwrap() {
        assert_eq!(member["kind"], 6, "{member}");
    }
}
