use super::*;
use serde_json::json;

fn derive_rule_prefilter(rule: &Value) -> Prefilter {
    StructuralQuery::new(None, Some(rule)).unwrap().prefilter()
}

/// An all-upper-case macro cannot select files to parse, yet it still
/// says where a match sits in a parsed file.
#[test]
fn an_upper_case_macro_anchors_recovery_but_not_the_prefilter() {
    let pattern = StructuralQuery::new(Some("JSON_ASSERT($$$A)"), None).unwrap();
    assert_eq!(pattern.prefilter(), Prefilter::None);
    assert_eq!(
        pattern.recovery_anchors(),
        Some(vec!["JSON_ASSERT".to_owned()])
    );
    let any = json!({"any": [{"pattern": "JSON_ASSERT($A)"}, {"pattern": "foo($A)"}]});
    let rule = StructuralQuery::new(None, Some(&any)).unwrap();
    assert_eq!(rule.prefilter(), Prefilter::None);
    assert_eq!(
        rule.recovery_anchors(),
        Some(vec!["JSON_ASSERT".to_owned(), "foo".to_owned()])
    );
    let anchorless = StructuralQuery::new(Some("$F($$$A)"), None).unwrap();
    assert_eq!(anchorless.recovery_anchors(), None);
}

#[test]
fn new_rejects_both_or_neither_query_sources() {
    assert!(StructuralQuery::new(Some("a"), Some(&json!({"pattern": "b"}))).is_err());
    assert!(StructuralQuery::new(None, None).is_err());
}

#[test]
fn new_rejects_empty_query_text() {
    assert!(StructuralQuery::new(Some("   "), None).is_err());
}

/// An empty rule object names no matcher: the shared decode refuses it.
#[test]
fn an_empty_rule_object_is_not_a_rule() {
    let empty = json!({});
    let query = StructuralQuery::new(None, Some(&empty)).unwrap();
    assert!(query.parsed_rule().is_err());
}

#[test]
fn decoded_json_literals_and_composition_keep_required_anchors() {
    let escaped: Value = serde_json::from_str(r#"{"pattern": "\u0066oo($X)"}"#).unwrap();
    for rule in [
        json!({"pattern": "foo($X)\n"}),
        escaped,
        json!({"all": [{"any": [{"pattern": "foo($X)"}, {"kind": "call_expression"}]}, {"pattern": "foo($Y)"}]}),
        json!({"has": {"pattern": "foo($X)", "stopBy": "end"}}),
    ] {
        assert_eq!(
            derive_rule_prefilter(&rule),
            Prefilter::Single("foo".to_owned()),
            "{rule}"
        );
    }
    for rule in [
        json!({"not": {"pattern": "foo($X)"}}),
        json!({"any": [{"pattern": "foo($X)"}, {"pattern": "$X"}]}),
        json!({"any": [{"pattern": "foo($X)"}, {"not": {"pattern": "bar($X)"}}]}),
        json!({"unknown": {"pattern": "foo($X)"}}),
    ] {
        assert_eq!(derive_rule_prefilter(&rule), Prefilter::None, "{rule}");
    }
}

/// The query prefilter keeps only literals every match must contain.
#[test]
fn rule_prefilter_keeps_only_required_anchors() {
    let single = |anchor: &str| Prefilter::Single(anchor.to_owned());
    let cases = [
        (
            "simple rule pattern",
            json!({"pattern": "await $C"}),
            single("await"),
        ),
        // `not:` alone is safe — the top-level positive `pattern:` implies the
        // file must contain the literal; `not:` only filters what's found.
        // Only the parsed positive pattern supplies the required anchor.
        (
            "positive anchor when not is present without any",
            json!({"pattern": "await $C", "not": {"pattern": "bar"}}),
            single("await"),
        ),
        (
            "any with a single anchor uses single",
            json!({"any": [{"pattern": "foo($X)"}]}),
            single("foo"),
        ),
        (
            "any with multiple anchors uses union",
            json!({"any": [{"pattern": "foo($X)"}, {"pattern": "bar($X)"}]}),
            Prefilter::Union(vec!["bar".to_owned(), "foo".to_owned()]),
        ),
        (
            "not does not hide the required any anchor",
            json!({"not": {"pattern": "bar"}, "any": [{"pattern": "foo($X)"}]}),
            single("foo"),
        ),
        // Patterns with only metavars produce no safe literal anchor.
        (
            "any without extractable anchors is none",
            json!({"any": [{"pattern": "$X"}, {"pattern": "$Y"}]}),
            Prefilter::None,
        ),
    ];
    for (case, rule, expected) in cases {
        let query = StructuralQuery::new(None, Some(&rule)).expect(case);
        assert_eq!(query.prefilter(), expected, "{case}");
    }
}

#[test]
fn literal_anchor_never_uses_operator_runs_whose_spacing_the_ast_ignores() {
    // `$A&&!$B` must still match `a && !b`; operator text is not a
    // necessary file literal, so only identifier-like tokens anchor.
    for pattern in ["$A && $A()", "$A&&!$B", "$A=-1", "$A>>$B", "$A::$B"] {
        assert_eq!(
            StructuralQuery::new(Some(pattern), None)
                .expect("valid query")
                .prefilter(),
            Prefilter::None,
            "{pattern}"
        );
    }
    assert_eq!(
        StructuralQuery::new(Some("$A&&!foo($B)"), None)
            .expect("valid query")
            .prefilter(),
        Prefilter::Single("foo".to_owned())
    );
}

#[test]
fn literal_anchor_skips_metavars_and_prefers_identifier_literals() {
    assert_eq!(
        StructuralQuery::new(Some("console.log($$$ARGS)"), None)
            .expect("valid query")
            .prefilter(),
        Prefilter::Single("console".to_owned())
    );
    assert_eq!(
        StructuralQuery::new(Some("foo($X)"), None)
            .expect("valid query")
            .prefilter(),
        Prefilter::Single("foo".to_owned())
    );
}
