//! A rule around a declaration pattern: its `kind` names one kind, it can
//! require an unsaid part absent, every place it holds a pattern reads it the
//! same way, and a name the grammar lacks names the nearest ones.

use super::*;

fn rule_lines(src: &str, ext: &str, rule: serde_json::Value) -> Vec<u32> {
    search(src, ext, None, Some(&rule))
        .unwrap_or_else(|error| panic!("{ext} {rule}: {error}"))
        .iter()
        .map(|found| found.start_line)
        .collect()
}

/// The grammar's own longer kind of a declaration (`abstract_class_declaration`
/// for `class_declaration`) is matched as well, and a rule's `kind` still
/// selects exactly one.
#[test]
fn a_rule_kind_still_names_one_kind() {
    let src = "class A {}\nabstract class B {}\n";
    assert_eq!(
        rule_lines(
            src,
            "ts",
            json!({"pattern":"class $C { $$$ }","kind":"class_declaration"})
        ),
        [1]
    );
    assert_eq!(
        rule_lines(src, "ts", json!({"kind":"abstract_class_declaration"})),
        [2]
    );
}

/// A rule says what the pattern leaves unsaid must be absent.
#[test]
fn a_rule_can_require_an_unsaid_part_absent() {
    let src = "function a() {}\nfunction b(): void {}\n";
    let rule = json!({"pattern":"function $N($$$) { $$$ }","not":{"has":{"field":"return_type","regex":"."}}});
    assert_eq!(rule_lines(src, "ts", rule), [1]);
    let rust = "fn a() {}\npub fn b() {}\n";
    let rule = json!({"pattern":"fn $N($$$) { $$$ }","not":{"has":{"kind":"visibility_modifier"}}});
    assert_eq!(rule_lines(rust, "rs", rule), [1]);
}

/// Every place a rule holds a pattern reads it the same way.
#[test]
fn nested_patterns_read_the_same() {
    let src = "class A { public m(): void { g(); } }\nclass B { n() { h(); } }\n";
    let method = "class $C { $M($$$) { $$$ } }";
    assert_eq!(
        rule_lines(src, "ts", json!({"all":[{"pattern":method}]})),
        [1, 2]
    );
    assert_eq!(
        rule_lines(src, "ts", json!({"any":[{"pattern":method}]})),
        [1, 2]
    );
    assert_eq!(
        rule_lines(
            src,
            "ts",
            json!({"kind":"class_body","has":{"pattern":{"context":"class W { m() { } }","selector":"method_definition"},"stopBy":"end"}})
        ),
        Vec::<u32>::new(),
        "a context pattern's literal name and empty body stay spelled"
    );
    assert_eq!(
        rule_lines(
            src,
            "ts",
            json!({"kind":"class_declaration","has":{"pattern":{"context":"class W { $M($$$) { $$$ } }","selector":"method_definition"},"stopBy":"end"}})
        ),
        [1, 2]
    );
    assert_eq!(
        rule_lines(
            src,
            "ts",
            json!({"pattern":"g()","inside":{"pattern":{"context":"class W { $M($$$) { $$$ } }","selector":"method_definition"},"stopBy":"end"}})
        ),
        [1]
    );
}

/// A kind or field this grammar lacks names the ones it has with those words.
#[test]
fn an_unknown_kind_or_field_names_the_nearest_ones() {
    let error = search("", "ts", None, Some(&json!({"kind":"function_definition"})))
        .err()
        .expect("unknown kind");
    assert!(error.contains("function_declaration"), "{error}");
    assert!(error.contains("method_definition"), "{error}");
    let error = search(
        "",
        "rs",
        None,
        Some(&json!({"kind":"function_item","has":{"field":"return_value","regex":"."}})),
    )
    .err()
    .expect("unknown field");
    assert!(error.contains("return_type"), "{error}");
    let error = search(
        "",
        "ts",
        None,
        Some(&json!({"pattern":{"context":"class A { m() {} }","selector":"method_defn"}})),
    )
    .err()
    .expect("unknown selector");
    assert!(error.contains("method_definition"), "{error}");
    // Nothing near: the error stays plain.
    let error = search("", "ts", None, Some(&json!({"kind":"zzz"})))
        .err()
        .expect("unknown kind");
    assert!(!error.contains("nearest"), "{error}");
}
