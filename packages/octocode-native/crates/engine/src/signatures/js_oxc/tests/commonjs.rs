//! CommonJS binding and export-shape facts: only what oxc semantics prove.

use super::*;

/// `(local, imported)` pairs of every load's bindings, in load order. A
/// module binding (no `scope`) is checked against the top-level declaration
/// the outline emitted for `local`; a scoped one holds only over its scope.
fn bindings(facts: &Value) -> Vec<Vec<(String, Option<String>)>> {
    facts["commonJs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|load| {
            load.get("bindings")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|binding| {
                    let local = binding["local"].as_str().unwrap().to_owned();
                    if binding.get("scope").is_none() {
                        assert_eq!(
                            binding["declarationId"],
                            declaration(facts, &local)["id"],
                            "{binding}"
                        );
                    }
                    (local, binding["imported"].as_str().map(str::to_owned))
                })
                .collect()
        })
        .collect()
}

fn exported_as(facts: &Value, name: &str) -> Option<Vec<String>> {
    let declaration = declaration(facts, name);
    (declaration["exported"] == true).then(|| {
        match declaration.get("exportedAs").and_then(Value::as_array) {
            Some(names) => names
                .iter()
                .map(|name| name.as_str().unwrap().to_owned())
                .collect(),
            None => vec![name.to_owned()],
        }
    })
}

fn has_export_diagnostic(facts: &Value) -> bool {
    facts["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|diagnostic| diagnostic["code"] == "unsupportedCommonJsExports")
}

#[test]
fn commonjs_bindings_name_whole_module_and_property_bindings() {
    let facts = graph(
        "var a = require('./a'); const {b, c: d} = require('./m'); const k = require('./n').k;",
        "entry.js",
    );
    let some = |name: &str| Some(name.to_owned());
    assert_eq!(
        bindings(&facts),
        vec![
            vec![("a".to_owned(), None)],
            vec![("b".to_owned(), some("b")), ("d".to_owned(), some("c"))],
            vec![("k".to_owned(), some("k"))],
        ]
    );
}

#[test]
fn commonjs_bindings_admit_module_require_and_parenthesized_initializers() {
    let facts = graph(
        "const a = (require('./a')); const b = module.require('./b');",
        "entry.js",
    );
    assert_eq!(
        bindings(&facts),
        vec![vec![("a".to_owned(), None)], vec![("b".to_owned(), None)]]
    );
}

#[test]
fn commonjs_bindings_reject_unprovable_identities_but_keep_the_load() {
    for source in [
        "var x = require('./x'); x = y;",
        "var _ = root._ || require('./l'); _ = other._;",
        "const {a = 1} = require('./m');",
        "const {...r} = require('./m');",
        "const {a, ...r} = require('./m');",
        "const {['a']: e} = require('./m');",
        "const {a: {b}} = require('./m');",
        "const p = require('./p'); function g(p) { p.f(); }",
        "const p = require('./p'); function g() { const p = 1; return p; }",
        "var q = require('./q'); var q = require('./r');",
        "const w = require('./w')();",
        "const v = cond ? require('./a') : require('./b');",
        "const t = require('./t').a.b;",
        "if (cond) { var h = require('./h'); }",
        "const s = require(name);",
    ] {
        let facts = graph(source, "entry.js");
        let loads = facts["commonJs"].as_array().unwrap();
        assert!(!loads.is_empty(), "{source}");
        assert!(
            bindings(&facts).iter().all(Vec::is_empty),
            "{source}: {facts}"
        );
    }
}

/// A guarded load (`E || require(x)`, `E ?? require(x)`) binds the module on
/// the branch where the guard fails (B122): even at module scope it carries
/// the extent of its scope, so its links stay at most medium.
#[test]
fn commonjs_guarded_loads_bind_over_their_scope() {
    for source in [
        "var _ = root._ || require('./l');",
        "var _ = root._ ?? require('./l');",
        // A write of the guard's own text assigns one of the two values
        // the declaration admits: the binding stays, at medium.
        "var _ = (root._) || require('./l'); if (c) { _ = root._; }",
    ] {
        let facts = graph(source, "entry.js");
        assert_eq!(
            bindings(&facts),
            vec![vec![("_".to_owned(), None)]],
            "{source}"
        );
        let scope = &facts["commonJs"][0]["bindings"][0]["scope"]["range"];
        assert_eq!(scope["start_byte"], 0, "{source}: {facts}");
        assert_eq!(scope["end_byte"], source.len(), "{source}: {facts}");
    }
}

/// A `require` declared directly in a function binds there and carries the
/// function's extent, so the graph links it only inside that scope.
#[test]
fn commonjs_bindings_in_a_function_hold_over_that_scope() {
    let source = "function f() { const z = require('./z'); }";
    let facts = graph(source, "entry.js");
    assert_eq!(bindings(&facts), vec![vec![("z".to_owned(), None)]]);
    let binding = &facts["commonJs"][0]["bindings"][0];
    assert_eq!(
        binding["scope"]["range"],
        serde_json::json!({"start_byte": 0, "end_byte": source.len()}),
        "{binding}"
    );
}

#[test]
fn commonjs_exports_name_whole_and_keyed_shapes() {
    let facts = graph(
        "function isObject() {}\nmodule.exports = isObject;",
        "lib.js",
    );
    assert_eq!(
        exported_as(&facts, "isObject"),
        Some(vec!["module.exports".to_owned()])
    );

    let facts = graph(
        "function copy() {}\nfunction write() {}\nfunction hidden() {}\nmodule.exports = {copy, w: write};",
        "lib.js",
    );
    assert_eq!(exported_as(&facts, "copy"), Some(vec!["copy".to_owned()]));
    assert_eq!(exported_as(&facts, "write"), Some(vec!["w".to_owned()]));
    assert_eq!(exported_as(&facts, "hidden"), None);

    let facts = graph(
        "function f() {}\nfunction g() {}\nexports.k = f;\nmodule.exports.j = g;",
        "lib.js",
    );
    assert_eq!(exported_as(&facts, "f"), Some(vec!["k".to_owned()]));
    assert_eq!(exported_as(&facts, "g"), Some(vec!["j".to_owned()]));
    assert!(!has_export_diagnostic(&facts));

    let facts = graph("function A() {}\nexports = module.exports = A;", "lib.js");
    assert_eq!(
        exported_as(&facts, "A"),
        Some(vec!["module.exports".to_owned()])
    );
}

#[test]
fn commonjs_exports_cancel_on_unsound_shapes() {
    for source in [
        "function a() {}\nfunction b() {}\nmodule.exports = a;\nmodule.exports = b;",
        "function a() {}\nfunction b() {}\nif (cond) { module.exports = a; }\nexports.b = b;",
        "function a() {}\nfunction b() {}\nfunction later() { module.exports = b; }\nmodule.exports = a;",
        "function a() {}\nmodule = {};\nmodule.exports = a;",
        "function a() {}\nexports = {};\nexports.a = a;",
    ] {
        let facts = graph(source, "lib.js");
        for name in ["a", "b"] {
            if facts["declarations"]
                .as_array()
                .unwrap()
                .iter()
                .any(|d| d["name"] == name)
            {
                assert_eq!(exported_as(&facts, name), None, "{source}: {name}");
            }
        }
        assert!(has_export_diagnostic(&facts), "{source}");
    }
}

#[test]
fn commonjs_exports_skip_detached_unbound_or_ambiguous_keys() {
    // A whole assignment detaches `exports`: `exports.k` no longer exports.
    let facts = graph(
        "function A() {}\nfunction f() {}\nmodule.exports = A;\nexports.k = f;",
        "lib.js",
    );
    assert_eq!(
        exported_as(&facts, "A"),
        Some(vec!["module.exports".to_owned()])
    );
    assert_eq!(exported_as(&facts, "f"), None);

    // `module.exports.k` written after the whole assignment still counts.
    let facts = graph(
        "function A() {}\nfunction f() {}\nmodule.exports = A;\nmodule.exports.k = f;",
        "lib.js",
    );
    assert_eq!(exported_as(&facts, "f"), Some(vec!["k".to_owned()]));

    // `module.exports = exports = X` re-attaches `exports`: a later
    // `exports.k` overwrites the literal's key.
    for source in [
        "function foo() {}\nfunction bar() {}\nmodule.exports = exports = { foo };\nexports.foo = bar;",
        "function foo() {}\nfunction bar() {}\nexports = module.exports = { foo };\nexports.foo = bar;",
    ] {
        let facts = graph(source, "lib.js");
        assert_eq!(exported_as(&facts, "foo"), None, "{source}");
        assert_eq!(
            exported_as(&facts, "bar"),
            Some(vec!["foo".to_owned()]),
            "{source}"
        );
    }

    // Program-level writes run in source order: the last one is the value.
    for source in [
        "function f() {}\nfunction g() {}\nexports.k = f;\nexports.k = g;",
        "function f() {}\nfunction g() {}\nmodule.exports = {k: f, k: g};",
        "function f() {}\nfunction g() {}\nexports.k = void 0, exports.k = g;",
    ] {
        let facts = graph(source, "lib.js");
        assert_eq!(exported_as(&facts, "f"), None, "{source}");
        assert_eq!(
            exported_as(&facts, "g"),
            Some(vec!["k".to_owned()]),
            "{source}"
        );
    }

    // The shape TypeScript emits for `export function a`.
    let facts = graph(
        "\"use strict\";\nObject.defineProperty(exports, \"__esModule\", { value: true });\nexports.b = exports.a = void 0;\nfunction a() {}\nexports.a = a;",
        "lib.js",
    );
    assert_eq!(exported_as(&facts, "a"), Some(vec!["a".to_owned()]));

    for source in [
        // A deferred write may run after any other.
        "function f() {}\nfunction g() {}\nexports.k = f;\nfunction later() { exports.k = g; }",
        // A spread or computed key may overwrite any key.
        "function f() {}\nfunction g() {}\nmodule.exports = {k: f, ...other};",
        "function f() {}\nfunction g() {}\nmodule.exports = {k: f, [name]: g};",
        "function f() {}\nfunction g() {}\nexports.k = f;\nexports[name] = g;",
        // The exports object escapes: any key may change.
        "function f() {}\nfunction g() {}\nexports.k = f;\nmixin(exports, g);",
        "function f() {}\nfunction g() {}\nmodule.exports.k = f;\nconst alias = module.exports;",
        "function f() {}\nfunction g() {}\nexports.k = f;\ndelete exports.k;",
        // A reassigned local is not the value the module exported.
        "var f = function () {};\nfunction g() {}\nexports.k = f;\nf = g;",
        // A shadowed `module` or `exports` is not the module object.
        "function f() {}\nfunction g() {}\nconst module = {};\nmodule.exports = f;",
        "function f() {}\nfunction g() {}\nvar exports = {};\nexports.k = f;",
    ] {
        let facts = graph(source, "lib.js");
        assert_eq!(exported_as(&facts, "f"), None, "{source}");
        assert_eq!(exported_as(&facts, "g"), None, "{source}");
    }
}

/// A function, arrow or class written straight into an export is a
/// declaration named as callers reach it, spanning its definition and owning
/// the calls inside it.
#[test]
fn commonjs_exports_defined_in_place_are_declarations() {
    let facts = graph(
        "exports.pad = function (s) { return trim(s); };\nmodule.exports.Store = class Box { get() {} };\n",
        "lib.js",
    );
    assert_eq!(exported_as(&facts, "pad"), Some(vec!["pad".to_owned()]));
    assert_eq!(exported_as(&facts, "Store"), Some(vec!["Store".to_owned()]));
    assert_eq!(declaration(&facts, "pad")["range"]["end"]["character"], 46);
    let get = facts["declarations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["name"] == "get")
        .unwrap();
    assert_eq!(get["parent"], declaration(&facts, "Store")["id"]);
    let call = &facts["calls"][0];
    assert_eq!(call["callee"], "trim");
    assert_eq!(
        call["callerId"],
        declaration(&facts, "pad")["id"],
        "{facts}"
    );

    let facts = graph(
        "module.exports = { left: function () {}, right() {}, up: () => 1, get size() { return 1; } };",
        "lib.js",
    );
    for name in ["left", "right", "up"] {
        assert_eq!(
            exported_as(&facts, name),
            Some(vec![name.to_owned()]),
            "{name}"
        );
    }
    assert!(names(&facts).iter().all(|name| name != "size"), "{facts}");

    let facts = graph("module.exports = function isThing() {};", "lib.js");
    assert_eq!(
        exported_as(&facts, "isThing"),
        Some(vec!["module.exports".to_owned()])
    );

    // Unnamed, unproven, detached, shadowing or repeated definitions
    // declare nothing; a shadowed name keeps its own, unexported, declaration.
    for source in [
        "module.exports = function () {};",
        "if (cond) { exports.f = function () {}; }",
        "exports.f = function () {};\nmodule.exports = {};",
        "module.exports = function f() {};\nmodule.exports.f = () => {};",
        "function f() {}\nexports.f = function () {};",
    ] {
        let facts = graph(source, "lib.js");
        let declared = names(&facts);
        if source.starts_with("function f") {
            assert_eq!(declared, ["f"], "{source}");
            assert_eq!(exported_as(&facts, "f"), None, "{source}");
        } else {
            assert!(declared.is_empty(), "{source}: {facts}");
        }
    }
}

fn names(facts: &Value) -> Vec<String> {
    facts["declarations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["name"].as_str().unwrap().to_owned())
        .collect()
}

/// `module.exports = require('./impl')` hands the whole module on: the file
/// forwards every export of the target, as `export * from './impl'` does.
/// A require that cannot be proven (reassigned, a non-literal specifier) or
/// a value that is not a bare load forwards nothing.
#[test]
fn a_whole_export_of_a_require_forwards_the_target_module() {
    let forwards = |src: &str| -> Vec<String> {
        graph(src, "lib/index.js")["exports"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|export| export["name"] == "*")
            .map(|export| export["source"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(
        forwards("module.exports = require('./impl');\n"),
        ["./impl"]
    );
    assert_eq!(
        forwards("exports = module.exports = require('./impl');\n"),
        ["./impl"]
    );
    assert!(forwards("module.exports = require(name);\n").is_empty());
    assert!(forwards("module.exports = require('./impl').work;\n").is_empty());
    assert!(forwards("function require() {}\nmodule.exports = require('./impl');\n").is_empty());
    assert!(forwards("const x = require('./impl');\nmodule.exports = x;\n").is_empty());
    // Only the one program-level write of a proven load forwards: a choice,
    // a later write, a call of the module or a write inside a function names
    // no module the file is.
    assert!(forwards("module.exports = ok ? require('./a') : require('./b');\n").is_empty());
    assert!(
        forwards("module.exports = require('./impl');\nmodule.exports = require('./other');\n")
            .is_empty()
    );
    assert!(forwards("module.exports = require('./impl')();\n").is_empty());
    assert!(forwards("function load() {\n  module.exports = require('./impl');\n}\n").is_empty());
    // A package forwards by its specifier, as `export * from 'pkg'` does.
    assert_eq!(forwards("module.exports = require('pkg');\n"), ["pkg"]);
}
