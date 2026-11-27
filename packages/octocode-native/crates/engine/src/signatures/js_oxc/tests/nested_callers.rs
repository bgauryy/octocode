//! A call's caller is its innermost enclosing callable declaration.

use super::*;

/// `(caller, callerId, callee)` of every `calls` fact.
fn callers(facts: &Value) -> Vec<(String, String, String)> {
    facts["calls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|call| {
            (
                call["caller"].as_str().unwrap().to_owned(),
                call["callerId"].as_str().unwrap_or("").to_owned(),
                call["callee"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn id_of(facts: &Value, name: &str) -> String {
    facts["declarations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["name"] == name)
        .and_then(|d| d["id"].as_str())
        .unwrap_or_else(|| panic!("declaration {name}: {facts}"))
        .to_owned()
}

#[test]
fn nested_function_declarations_and_bound_arrows_own_their_calls() {
    let src = "export function outer(src) {\n  const blank = (l) => !l.trim();\n  function helper(x) {\n    return x + 1;\n  }\n  function blocks(ls) {\n    if (blank(ls[0])) return helper(1);\n    return 0;\n  }\n  return blocks(src);\n}\n";
    let facts = graph(src, "m.mjs");
    let got = callers(&facts);
    let expect =
        |caller: &str, callee: &str| (caller.to_owned(), id_of(&facts, caller), callee.to_owned());
    assert_eq!(
        got,
        vec![
            expect("blank", "l.trim"),
            expect("blocks", "blank"),
            expect("blocks", "helper"),
            expect("outer", "blocks"),
        ]
    );
    // Argument sites share the call's caller.
    for site in facts["argumentSites"].as_array().unwrap() {
        let call = facts["calls"]
            .as_array()
            .unwrap()
            .iter()
            .find(|call| call["range"]["start"] == site["range"]["start"])
            .unwrap_or_else(|| panic!("call of {site}"));
        assert_eq!(site["caller"], call["caller"], "{site}");
        assert_eq!(site["callerId"], call["callerId"], "{site}");
    }
}

#[test]
fn nested_callables_inside_members_and_classes_own_their_calls() {
    let src = "export class Shell {\n  run() {\n    function step() { return tick(); }\n    const later = function () { return tock(); };\n    return step() + later();\n  }\n}\nexport const make = () => {\n  class Local {\n    go() { return deep(); }\n  }\n  const inner = () => shallow();\n  return [new Local(), inner(), top()];\n};\n";
    let facts = graph(src, "shell.ts");
    let pairs = callers(&facts)
        .into_iter()
        .map(|(caller, _, callee)| (caller, callee))
        .collect::<Vec<_>>();
    for (caller, callee) in [
        ("step", "tick"),
        ("later", "tock"),
        ("run", "step"),
        ("run", "later"),
        ("go", "deep"),
        ("inner", "shallow"),
        ("make", "inner"),
        ("make", "top"),
    ] {
        assert!(
            pairs.contains(&(caller.to_owned(), callee.to_owned())),
            "expected {caller} -> {callee} in {pairs:?}"
        );
    }
}

/// A callback that is no declaration falls back to the nearest enclosing one.
#[test]
fn anonymous_callbacks_fall_back_to_the_enclosing_declaration() {
    let src = "export function each(items) {\n  items.forEach((item) => visit(item));\n  return items.map(function (x) { return wrap(x); });\n}\ndescribe('suite', () => { it('case', () => spec()); });\n";
    let facts = graph(src, "each.js");
    let pairs = callers(&facts)
        .into_iter()
        .map(|(caller, _, callee)| (caller, callee))
        .collect::<Vec<_>>();
    for (caller, callee) in [
        ("each", "visit"),
        ("each", "wrap"),
        ("module", "describe"),
        ("module", "spec"),
    ] {
        assert!(
            pairs.contains(&(caller.to_owned(), callee.to_owned())),
            "expected {caller} -> {callee} in {pairs:?}"
        );
    }
}
