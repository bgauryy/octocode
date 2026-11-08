//! CommonJS: a `require` binding links calls only where the engine proved
//! it holds the loaded module, and an import-bound name that resolves to no
//! export never falls back to a graph-wide name match.
use super::*;

const IS_OBJECT: &str =
    "function isObject(value) { return value != null; }\nmodule.exports = isObject;\n";

#[test]
fn a_whole_module_binding_links_to_the_module_exports_value() {
    let temp = fixture(&[
        ("lib/isObject.js", IS_OBJECT),
        (
            "a.js",
            "var isObject = require('./lib/isObject');\nfunction main(x) { return isObject(x); }\nmodule.exports = main;\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        caller_edges(root, "lib/isObject.js#isObject"),
        [edge("a.js#main", "import", "high")]
    );
    // The variable holding the module is never a call target of its own.
    assert_eq!(caller_edges(root, "a.js#isObject"), []);
    let deps = ask(root, "deps", Some("a.js"));
    assert_eq!(files(&deps), ["lib/isObject.js"], "{deps}");
}

#[test]
fn a_module_binding_reaches_exported_keys_only() {
    let temp = fixture(&[
        (
            "lib/util.js",
            "function pad(s) { return s; }\nfunction inner(s) { return s; }\nfunction trim(s) { return s; }\nmodule.exports = { pad, t: trim };\n",
        ),
        (
            "a.js",
            "const util = require('./lib/util');\nconst { t } = require('./lib/util');\nfunction main(x) { util.pad(x); util.inner(x); t(x); }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        caller_edges(root, "lib/util.js#pad"),
        [edge("a.js#main", "namespace", "high")]
    );
    assert_eq!(
        caller_edges(root, "lib/util.js#trim"),
        [edge("a.js#main", "import", "high")]
    );
    // Declared but not exported: no fallback to any nested declaration.
    assert_eq!(caller_edges(root, "lib/util.js#inner"), []);
}

/// Regression: an ESM default import whose target exports no `default`
/// linked to a same-named, same-directory variable of another file.
#[test]
fn an_import_bound_name_without_an_export_stays_unresolved() {
    let temp = fixture(&[
        ("lib/isObject.js", "function isObject(v) { return !!v; }\n"),
        (
            "a.js",
            "var isObject = require('./missing');\nfunction main(v) { return isObject(v); }\n",
        ),
        (
            "b.mjs",
            "import isObject from './lib/isObject.js';\nexport function run(x) { isObject(x); }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(caller_edges(root, "a.js#isObject"), []);
    assert_eq!(caller_edges(root, "lib/isObject.js#isObject"), []);
}

#[test]
fn an_esm_default_import_of_commonjs_reaches_module_exports() {
    let temp = fixture(&[
        ("lib/isObject.js", IS_OBJECT),
        (
            "b.mjs",
            "import isObject from './lib/isObject.js';\nexport function run(x) { isObject(x); }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        caller_edges(root, "lib/isObject.js#isObject"),
        [edge("b.mjs#run", "import", "high")]
    );
}

#[test]
fn calls_through_an_external_require_are_excluded_as_external() {
    let temp = fixture(&[(
        "a.js",
        "const path = require('path');\nfunction join() {}\nfunction main() { return path.join('a', 'b'); }\n",
    )]);
    let root = temp.0.as_path();
    let out = ingest(root, root);
    assert_eq!(out.value.get("error"), None, "{}", out.value);
    assert_eq!(
        out.value["calls"]["unresolvedByReason"]["external-package"], 1,
        "{}",
        out.value
    );
    assert_eq!(caller_edges(root, "a.js#join"), []);
}

/// A binding the engine could not prove keeps today's behavior: its file
/// edge, and no import link.
#[test]
fn an_unproven_binding_keeps_its_file_edge_only() {
    let temp = fixture(&[
        ("lib/isObject.js", IS_OBJECT),
        (
            "a.js",
            "var isObject = root.isObject || require('./lib/isObject');\nfunction main(x) { return isObject(x); }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(caller_edges(root, "lib/isObject.js#isObject"), []);
    let deps = ask(root, "deps", Some("a.js"));
    assert_eq!(files(&deps), ["lib/isObject.js"], "{deps}");
}

/// A barrel that imports a name and exports the binding (`import { a }
/// from './impl'; export { a }`) re-exports it: callers through the barrel
/// resolve to the declaration, not by a graph-wide name match.
#[test]
fn an_exported_import_binding_forwards_to_its_declaration() {
    let temp = fixture(&[
        (
            "src/impl.ts",
            "export function helper() {}\nexport default function main() {}\n",
        ),
        (
            "src/index.ts",
            "import { helper } from './impl';\nimport entry from './impl';\nexport { helper, entry as start };\n",
        ),
        (
            "app/use.ts",
            "import { helper, start } from '../src/index';\nexport function run() { helper(); start(); }\n",
        ),
        // A same-named decoy beside the caller.
        ("app/helper.ts", "export function helper() {}\n"),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        caller_edges(root, "src/impl.ts#helper"),
        [edge("app/use.ts#run", "import", "high")]
    );
    assert_eq!(
        caller_edges(root, "src/impl.ts#main"),
        [edge("app/use.ts#run", "import", "high")]
    );
    assert_eq!(caller_edges(root, "app/helper.ts#helper"), []);
}

/// A CommonJS barrel `module.exports = require('./impl')` forwards every
/// export of the implementation: callers through it, by destructured name or
/// by namespace, resolve to the declaration, never to a same-named decoy.
#[test]
fn a_whole_export_of_a_require_forwards_to_the_implementation() {
    let temp = fixture(&[
        (
            "lib/impl.js",
            "exports.work = function work() {};\nexports.other = function other() {};\n",
        ),
        ("lib/index.js", "module.exports = require('./impl');\n"),
        (
            "app/use.js",
            "const { work } = require('../lib');\nconst lib = require('../lib');\nfunction run() { work(); lib.other(); }\n",
        ),
        ("app/decoy.js", "exports.work = function work() {};\n"),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        caller_edges(root, "lib/impl.js#work"),
        [edge("app/use.js#run", "import", "high")]
    );
    assert_eq!(
        caller_edges(root, "lib/impl.js#other"),
        [edge("app/use.js#run", "namespace", "high")]
    );
    assert_eq!(caller_edges(root, "app/decoy.js#work"), []);
}

/// A variable holding a module is no call target, yet its name stays
/// occupied: it never leaves a same-named declaration elsewhere unique.
#[test]
fn a_module_holder_keeps_its_name_occupied() {
    let temp = fixture(&[
        (
            "lib/a.js",
            "var formatValue = require('./missing');\nmodule.exports = formatValue;\n",
        ),
        (
            "z/b.js",
            "function formatValue() {}\nmodule.exports = formatValue;\n",
        ),
        ("c/caller.js", "function run() { formatValue(); }\n"),
    ]);
    let root = ingested(&temp);
    assert_eq!(caller_edges(root, "z/b.js#formatValue"), []);
    assert_eq!(caller_edges(root, "lib/a.js#formatValue"), []);
}

/// `export default memo(Checkbox)` renders `Checkbox`: a default import of
/// it links JSX renders to the component, not by a graph-wide name match.
#[test]
fn a_react_wrapped_default_export_links_its_component() {
    let temp = fixture(&[
        (
            "src/Checkbox.tsx",
            "import React, { memo } from 'react';\nconst Checkbox = () => <input />;\nexport default memo(Checkbox);\n",
        ),
        (
            "src/Panel.tsx",
            "import React from 'react';\nimport Checkbox from './Checkbox';\nexport function Panel() { return <Checkbox />; }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        caller_edges(root, "src/Checkbox.tsx#Checkbox"),
        [edge("src/Panel.tsx#Panel", "renders:import", "high")]
    );
}

const UTIL: &str =
    "function k(s) { return s; }\nfunction j(s) { return s; }\nmodule.exports = { k, j };\n";

/// A `require` inside a function binds its name for that scope only: calls
/// inside it link at medium confidence (a lazy load runs only when the scope
/// does), and a same-named call elsewhere stays unlinked.
#[test]
fn a_function_scope_require_links_calls_in_its_scope_only() {
    let temp = fixture(&[
        ("lib/util.js", UTIL),
        (
            "a.js",
            "function f(x) { const m = require('./lib/util'); return m.k(x); }\nfunction g(x) { const { j } = require('./lib/util'); return j(x); }\nfunction h(x) { return m.k(x) + j(x); }\nmodule.exports = { f, g, h };\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        caller_edges(root, "lib/util.js#k"),
        [edge("a.js#f", "namespace", "medium")]
    );
    assert_eq!(
        caller_edges(root, "lib/util.js#j"),
        [edge("a.js#g", "import", "medium")]
    );
}

/// A scope binding reaches only calls inside the scope node: a module-level
/// callback's `require` does not bind the module's own calls, and a name
/// declared again in a nested scope proves nothing.
#[test]
fn a_scope_require_binds_nothing_outside_its_scope_or_under_a_shadow() {
    let temp = fixture(&[
        ("lib/util.js", UTIL),
        (
            "a.js",
            "run(() => { const m = require('./lib/util'); m.j(1); });\nm.k(2);\nfunction f(o) { const m = require('./lib/util'); function inner() { const m = o; return m.k(3); } return m.k(4) + inner(); }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        caller_edges(root, "lib/util.js#j"),
        [edge("a.js", "namespace", "medium")]
    );
    assert_eq!(caller_edges(root, "lib/util.js#k"), []);
}

/// An inline load binds its own receiver only: a call nested in its
/// arguments is governed by its own receiver, so a dynamic `require(p).k`
/// there links nothing and a literal one links to its own module.
#[test]
fn an_inline_require_binds_its_own_receiver_not_nested_calls() {
    let temp = fixture(&[
        ("lib/a.js", UTIL),
        (
            "lib/b.js",
            "function k(s) { return s; }\nmodule.exports = { k };\n",
        ),
        (
            "dyn.js",
            "function f(p) { return require('./lib/a').k(require(p).j(22)); }\nmodule.exports = { f };\n",
        ),
        (
            "lit.js",
            "function g() { return require('./lib/a').j(require('./lib/b').k()); }\nmodule.exports = { g };\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        caller_edges(root, "lib/a.js#k"),
        [edge("dyn.js#f", "namespace", "medium")]
    );
    assert_eq!(
        caller_edges(root, "lib/a.js#j"),
        [edge("lit.js#g", "namespace", "medium")]
    );
    assert_eq!(
        caller_edges(root, "lib/b.js#k"),
        [edge("lit.js#g", "namespace", "medium")]
    );
}

/// `require('./m').k()` calls the module's export `k` where it is written.
#[test]
fn an_inline_require_member_call_links_to_the_export() {
    let temp = fixture(&[
        ("lib/util.js", UTIL),
        (
            "a.js",
            "function f(x) { return require('./lib/util').k(x); }\nconst v = require('./lib/util').j(1);\nfunction g(x) { return require('./lib/util').inner(x); }\nmodule.exports = { f, g, v };\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        caller_edges(root, "lib/util.js#k"),
        [edge("a.js#f", "namespace", "medium")]
    );
    assert_eq!(
        caller_edges(root, "lib/util.js#j"),
        [edge("a.js#v", "namespace", "medium")]
    );
}

/// A specifier built at runtime is disclosed as `non-literal-specifier`,
/// whatever scope loads it, and binds no calls.
#[test]
fn a_dynamic_specifier_is_disclosed_and_binds_nothing() {
    let temp = fixture(&[
        ("lib/util.js", UTIL),
        (
            "a.js",
            "function f(p) {\n  const m = require(p);\n  return m.k(1) + require('./lib/' + p).j(2);\n}\nmodule.exports = f;\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(caller_edges(root, "lib/util.js#k"), []);
    assert_eq!(caller_edges(root, "lib/util.js#j"), []);
    let out = ask(root, "diagnostics", Some("unsupported-linking"));
    let lines = out["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| {
            row["message"]
                .as_str()
                .is_some_and(|message| message.contains("(non-literal-specifier)"))
        })
        .count();
    assert_eq!(lines, 2, "{out}");
}

/// Node gives an ESM importer each name a CommonJS module assigns as
/// `exports.k`: a named import of that key reaches its declaration.
#[test]
fn an_esm_named_import_of_a_commonjs_key_reaches_its_declaration() {
    let temp = fixture(&[
        (
            "lib/a.js",
            "exports.pad = function (s) { return s; };\nfunction trim(s) { return s; }\nmodule.exports.trim = trim;\n",
        ),
        (
            "b.mjs",
            "import { pad, trim } from './lib/a.js';\nexport function run(x) { pad(x); trim(x); }\n",
        ),
    ]);
    let root = ingested(&temp);
    for target in ["lib/a.js#pad", "lib/a.js#trim"] {
        assert_eq!(
            caller_edges(root, target),
            [edge("b.mjs#run", "import", "high")],
            "{target}"
        );
    }
}

/// CommonJS modules commonly define exports in place: a function, arrow or
/// class assigned to an export key, a `module.exports` object literal's
/// values and methods, or a named function as the whole export. Each is a
/// declaration named by what callers write, reached through its binding.
#[test]
fn exports_defined_in_place_are_declarations_their_bindings_reach() {
    let temp = fixture(&[
        (
            "lib/a.js",
            "exports.pad = function (s) { return s; };\nmodule.exports.trim = (s) => s;\nexports.named = function namedFn(s) { return s; };\n",
        ),
        (
            "lib/b.js",
            "module.exports = {\n  left: function (s) { return s; },\n  right(s) { return s; },\n  up: (s) => s,\n};\n",
        ),
        (
            "lib/c.js",
            "module.exports = function isThing(v) { return !!v; };\n",
        ),
        (
            "main.js",
            "const a = require('./lib/a');\nconst { trim } = require('./lib/a');\nconst b = require('./lib/b');\nconst isThing = require('./lib/c');\nfunction run(x) {\n  a.pad(x); trim(x); a.named(x);\n  b.left(x); b.right(x); b.up(x);\n  isThing(x);\n}\nmodule.exports = run;\n",
        ),
    ]);
    let root = ingested(&temp);
    for target in [
        "lib/a.js#pad",
        "lib/a.js#named",
        "lib/b.js#left",
        "lib/b.js#right",
        "lib/b.js#up",
    ] {
        assert_eq!(
            caller_edges(root, target),
            [edge("main.js#run", "namespace", "high")],
            "{target}"
        );
    }
    for target in ["lib/a.js#trim", "lib/c.js#isThing"] {
        assert_eq!(
            caller_edges(root, target),
            [edge("main.js#run", "import", "high")],
            "{target}"
        );
    }
}

/// Calls inside an in-place export belong to it, and only export writes
/// declare: a prototype member or an unprovable export stays undeclared, and
/// a key whose name the module already binds keeps that binding's identity.
#[test]
fn exports_defined_in_place_own_their_calls_and_nothing_else_declares() {
    let temp = fixture(&[
        (
            "lib/a.js",
            "function helper(s) { return s; }\nfunction pad(s) { return s; }\nexports.trim = function (s) { return helper(s); };\nexports.pad = function (s) { return pad(s); };\nWidget.prototype.render = function () { return helper(1); };\n",
        ),
        (
            "lib/b.js",
            "if (cond) { exports.cut = function (s) { return s; }; }\n",
        ),
        (
            "main.js",
            "const a = require('./lib/a');\nconst b = require('./lib/b');\nfunction run(x) { a.trim(x); a.pad(x); b.cut(x); render(); }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        caller_edges(root, "lib/a.js#helper"),
        [
            edge("lib/a.js", "local", "high"),
            edge("lib/a.js#trim", "local", "high")
        ]
    );
    assert_eq!(
        caller_edges(root, "lib/a.js#trim"),
        [edge("main.js#run", "namespace", "high")]
    );
    for undeclared in ["lib/a.js#render", "lib/b.js#cut"] {
        let out = ask(root, "node", Some(undeclared));
        assert_eq!(out["errorCode"], "notFound", "{undeclared}: {out}");
    }
    // `exports.pad` is not the declared `pad`, and `pad` is not exported.
    assert_eq!(
        caller_edges(root, "lib/a.js#pad"),
        [edge("lib/a.js", "local", "high")]
    );
}
