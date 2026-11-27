//! JS/TS local bindings bound their own reach: no other file can name a
//! parameter or a function nested in another, so importer and text checks of
//! other files cannot add a use of one.

use crate::lsp::resolver::local_binding_lines;

/// `shadow.js` of the tool-ratings fixture: a parameter named like an
/// exported function, and a function nested in another.
const SHADOW: &str = "\
export function shadow(core) { return core(7); }
export function unrelatedCaller() { function core(x) { return x * 2; } return core(3); }
// core(100) is a comment, not a call.
export const text = \"core(200)\";
";

#[test]
fn a_javascript_parameter_and_nested_function_are_local() {
    // The parameter, at its declaration and at its call.
    assert_eq!(
        local_binding_lines("shadow.js", SHADOW, "core", 0, 23),
        Some((0, 0))
    );
    assert_eq!(
        local_binding_lines("shadow.js", SHADOW, "core", 0, 38),
        Some((0, 0))
    );
    // The nested function, at its declaration and at its call.
    assert_eq!(
        local_binding_lines("shadow.js", SHADOW, "core", 1, 45),
        Some((1, 1))
    );
    assert_eq!(
        local_binding_lines("shadow.js", SHADOW, "core", 1, 78),
        Some((1, 1))
    );
    // The same reading for TypeScript, across lines.
    let typescript = "export function total(items: number[]): number {\n  let sum = 0;\n  for (const item of items) sum += item;\n  return sum;\n}\n";
    assert_eq!(
        local_binding_lines("total.ts", typescript, "sum", 3, 9),
        Some((0, 4))
    );
}

#[test]
fn module_scope_namespace_and_enum_names_are_not_local() {
    // An exported function and a module-level variable reach other files.
    assert_eq!(
        local_binding_lines("shadow.js", SHADOW, "shadow", 0, 16),
        None
    );
    let module = "const helper = 1;\nexport function f() { return helper; }\n";
    assert_eq!(local_binding_lines("m.ts", module, "helper", 1, 29), None);
    // A namespace member and an enum member are reached through their owner.
    let namespace = "namespace N {\n  export const inner = 1;\n}\n";
    assert_eq!(local_binding_lines("n.ts", namespace, "inner", 1, 15), None);
    let members = "enum E {\n  A,\n}\n";
    assert_eq!(local_binding_lines("e.ts", members, "A", 1, 2), None);
}

#[test]
fn a_shorthand_pattern_element_and_a_dynamic_scope_are_not_local() {
    // `{ core }` binds a name that is also the property it reads.
    let shorthand = "export function f({ core }) {\n  return core;\n}\n";
    assert_eq!(local_binding_lines("p.js", shorthand, "core", 1, 9), None);
    // A renamed element binds only its local name.
    let renamed = "export function f({ core: local }) {\n  return local;\n}\n";
    assert_eq!(
        local_binding_lines("p.js", renamed, "local", 1, 9),
        Some((0, 2))
    );
    // `eval` can bind names the scopes do not show.
    let dynamic = "function f(core) {\n  eval('var core = 2');\n  return core;\n}\n";
    assert_eq!(local_binding_lines("d.js", dynamic, "core", 2, 9), None);
}

#[test]
fn a_parse_with_errors_cannot_tell() {
    let broken = "export function f(core) {\n  return core(;\n}\n";
    assert_eq!(local_binding_lines("b.js", broken, "core", 0, 18), None);
}
