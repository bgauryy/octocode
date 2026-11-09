use super::*;
use serde_json::json;

fn derive_rule_prefilter(rule: &Value) -> Prefilter {
    StructuralQuery::new(None, Some(rule)).unwrap().prefilter()
}

/// A `regex` that spells only a literal (bare, or between `^` and `$`) is
/// text every node it tests must hold, so it selects files to parse; a
/// regex with any syntax (flags, classes, alternation, escapes) does not.
#[test]
fn a_literal_regex_anchors_the_prefilter() {
    let named =
        json!({"kind": "function_item", "has": {"kind": "identifier", "regex": "^run_task$"}});
    assert_eq!(
        derive_rule_prefilter(&named),
        Prefilter::Single("run_task".to_owned())
    );
    let spelled = json!({"kind": "function_item", "regex": "Config"});
    assert_eq!(
        derive_rule_prefilter(&spelled),
        Prefilter::Single("Config".to_owned())
    );
    for regex in [
        "(?i)config",
        "Conf.g",
        "^a|b$",
        "Config\\b",
        "[C]onfig",
        "^$",
        "",
    ] {
        let rule = json!({"kind": "function_item", "regex": regex});
        assert_eq!(derive_rule_prefilter(&rule), Prefilter::None, "{regex}");
    }
}

/// The anchor a literal regex gives loses no match: the anchored rule finds
/// what the same regex wrapped in a group (which gives no anchor) finds, and
/// every source with a match holds the anchor's bytes. Case variants,
/// Unicode, CRLF and a literal that spans lines each keep their count.
#[test]
fn a_literal_regex_anchor_keeps_every_match() {
    let named = |regex: &str| json!({"kind": "function_item", "has": {"kind": "identifier", "regex": regex}});
    let spelled = |regex: &str| json!({"kind": "function_item", "regex": regex});
    let crlf = "a: u8,\r\n       b";
    let lf = "a: u8,\n       b";
    let cases: [(Value, &str, usize); 14] = [
        (named("^run_task$"), "fn run_task() {}\n", 1),
        (named("^run_task$"), "fn RUN_TASK() {}\n", 0),
        (named("^run_task$"), "fn Run_Task() {}\n", 0),
        (
            named("^run_task$"),
            "fn run_tasks() {}\nfn x() { run_task(); }\n",
            0,
        ),
        (named("^café$"), "fn café(x: u8) {}\nfn cafe() {}\n", 1),
        (named("^café$"), "fn cafe() {}\n", 0),
        (named("^café$"), "fn CAFÉ() {}\r\nfn café() {}\r\n", 1),
        (spelled("naïve"), "fn f() { let s = \"naïve\"; }\n", 1),
        (spelled("naïve"), "fn f() { let s = \"naive\"; }\n", 0),
        (spelled(crlf), "fn run(a: u8,\r\n       b: u8) {}\r\n", 1),
        (spelled(crlf), "fn run(a: u8,\n       b: u8) {}\n", 0),
        (
            spelled(lf),
            "fn run(a: u8,\n       b: u8) {}\nfn z() {}\n",
            1,
        ),
        (spelled(lf), "fn run(a: u8,\r\n       b: u8) {}\r\n", 0),
        (
            spelled(lf),
            "fn run(a: u8,\n       b: u8) {}\nfn s(a: u8,\n       b: u16) {}\n",
            2,
        ),
    ];
    for (rule, source, expected) in cases {
        let Prefilter::Single(anchor) = derive_rule_prefilter(&rule) else {
            panic!("a literal regex anchors: {rule}");
        };
        let found = crate::structural::search(source, "rs", None, Some(&rule)).expect("search");
        assert_eq!(found.len(), expected, "{rule} over {source:?}");
        assert!(
            found.is_empty() || source.contains(anchor.as_str()),
            "{rule} over {source:?}: a match in a file the anchor skips"
        );
        let mut grouped = rule.clone();
        let target = if grouped.get("has").is_some() {
            &mut grouped["has"]["regex"]
        } else {
            &mut grouped["regex"]
        };
        *target = json!(format!("(?:{})", target.as_str().expect("regex")));
        assert_eq!(
            derive_rule_prefilter(&grouped),
            Prefilter::None,
            "{grouped}"
        );
        let unanchored =
            crate::structural::search(source, "rs", None, Some(&grouped)).expect("search");
        assert_eq!(
            unanchored.len(),
            found.len(),
            "{grouped} over {source:?}: the anchored rule differs"
        );
    }
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
        Prefilter::All(vec![vec!["console".to_owned()], vec!["log".to_owned()]])
    );
    assert_eq!(
        StructuralQuery::new(Some("foo($X)"), None)
            .expect("valid query")
            .prefilter(),
        Prefilter::Single("foo".to_owned())
    );
}

/// A pattern needs each of its safe literals, so the prefilter requires all
/// of them (an AND), and the explanation names the kind.
#[test]
fn a_pattern_requires_every_safe_literal() {
    let query = StructuralQuery::new(Some("$X.map_err(|$E| format!($$$))"), None).unwrap();
    let explanation = query.explanation();
    assert_eq!(explanation.pre_filter, "all-anchors");
    assert_eq!(
        explanation.literal_anchor.as_deref(),
        Some("format & map_err")
    );
}

/// A rule keeps every conjunct: its own pattern, `all`, `has` and `inside`
/// each add their groups, and an `any` adds one union group.
#[test]
fn a_rule_requires_every_conjunct_and_any_stays_a_union() {
    let rule = json!({
        "all": [
            {"any": [{"pattern": "foo_a($X)"}, {"pattern": "bar_b($X)"}]},
            {"inside": {"pattern": "fn outer_c() { $$$ }", "stopBy": "end"}}
        ],
        "has": {"pattern": "qux_d", "stopBy": "end"},
        "not": {"pattern": "never_e($X)"},
        "precedes": {"pattern": "after_f($X)"}
    });
    let explanation = StructuralQuery::new(None, Some(&rule))
        .unwrap()
        .explanation();
    assert_eq!(explanation.pre_filter, "all-anchors");
    assert_eq!(
        explanation.literal_anchor.as_deref(),
        Some("outer_c & qux_d & bar_b|foo_a")
    );
}

/// Recovery still reads by the one narrowest group: a pattern's longest
/// literal, a rule's group with the fewest literals.
#[test]
fn recovery_reads_by_the_narrowest_group() {
    let pattern = StructuralQuery::new(Some("$X.map_err(|$E| format!($$$))"), None).unwrap();
    assert_eq!(pattern.recovery_anchors(), Some(vec!["map_err".to_owned()]));
    let rule = json!({
        "all": [{"any": [{"pattern": "foo_a($X)"}, {"pattern": "bar_b($X)"}]}],
        "has": {"pattern": "qux_d", "stopBy": "end"}
    });
    assert_eq!(
        StructuralQuery::new(None, Some(&rule))
            .unwrap()
            .recovery_anchors(),
        Some(vec!["qux_d".to_owned()])
    );
}

/// A macro body is re-parsed when it spells a literal of the match's own
/// node or of any `has` target, wherever that `has` sits in the rule; a
/// `has` target that needs no literal re-parses every body.
#[test]
fn macro_anchors_hold_every_has_target_literal() {
    let anchors = |rule: Value| {
        StructuralQuery::new(None, Some(&rule))
            .unwrap()
            .macro_anchors()
    };
    assert_eq!(
        anchors(json!({
            "kind": "function_item",
            "regex": "^fn k",
            "not": {"has": {"pattern": "check($A)", "stopBy": "end"}}
        })),
        Some(vec!["check".to_owned(), "fn k".to_owned()])
    );
    assert_eq!(
        anchors(json!({
            "pattern": "start_job()",
            "precedes": {"kind": "expression_statement", "has": {"pattern": "check($A)", "stopBy": "end"}}
        })),
        Some(vec!["check".to_owned(), "start_job".to_owned()])
    );
    assert_eq!(
        anchors(
            json!({"pattern": "outer_fn($$$)", "has": {"kind": "call_expression", "stopBy": "end"}})
        ),
        None
    );
    // An `inside` ancestor's literal never gates a body.
    assert_eq!(
        anchors(
            json!({"pattern": "check($A)", "inside": {"pattern": "assert!($$$)", "stopBy": "end"}})
        ),
        Some(vec!["check".to_owned()])
    );
}
