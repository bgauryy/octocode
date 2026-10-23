use super::*;

#[test]
fn a_fix_that_needs_the_caller_has_no_lead() {
    // An unlisted value no rule maps to one listed value.
    let input = json!({"queries":[{"view":"none"}]});
    let unlisted = issue(
        "schema.enum",
        &["queries", "0", "view"],
        Some(json!({"enum":["files","paths"]})),
        Some(json!("none")),
    );
    assert!(repair_input(&input, &[unlisted], always).is_none());
    let missing = issue("schema.required", &["queries", "0", "path"], None, None);
    let unknown = issue(
        "schema.unknown-field",
        &["queries", "0", "view"],
        None,
        None,
    );
    assert!(repair_input(&input, &[unknown, missing], always).is_none());
    assert!(repair_input(&input, &[], always).is_none());
}

/// A line-range item spelled another way becomes its span; one sent
/// for the whole list becomes the one-span list.
#[test]
fn a_line_range_typo_becomes_its_span() {
    let item =
        json!({"type":"string","pattern":crate::contracts::validate::shape::LINE_RANGE_PATTERN});
    let list = json!({"type":"array","items": item.clone()});
    let input = json!({"queries":[{"ranges":["3:6",[8,9],"5:2"]}]});
    let at = |index: &str, received: Value, rule: &str| {
        issue(
            rule,
            &["queries", "0", "ranges", index],
            Some(item.clone()),
            Some(received),
        )
    };
    let fixed = repair_input(
        &input,
        &[
            at("0", json!("3:6"), "schema.pattern"),
            at("1", json!([8, 9]), "schema.type"),
        ],
        always,
    )
    .expect("exact");
    assert_eq!(
        fixed.input["queries"][0]["ranges"],
        json!(["3-6", "8-9", "5:2"])
    );
    assert!(repair_input(&input, &[at("2", json!("5:2"), "schema.pattern")], always).is_none());
    let scalar = json!({"queries":[{"ranges":"3..6"}]});
    let whole = issue(
        "schema.type",
        &["queries", "0", "ranges"],
        Some(list),
        Some(json!("3..6")),
    );
    let fixed = repair_input(&scalar, &[whole], always).expect("exact");
    assert_eq!(fixed.input, json!({"queries":[{"ranges":["3-6"]}]}));
}

/// An issue with no fix of its own is settled by another edit of the
/// same pass that sets its path; a fix's approximate fields are named.
#[test]
fn an_issue_another_edit_sets_needs_no_fix_and_approximations_are_named() {
    let input = json!({"queries":[{"path":"https://github.com/o/r/blob/main/a.rs"}]});
    let url = issue(
        "schema.repository-url",
        &["queries", "0", "path"],
        Some(
            json!({"setFields":{"owner":"o","repo":"r","ref":"main","path":"a.rs"},
                "approximateFields":["ref","path"]}),
        ),
        None,
    );
    let missing = issue("schema.required", &["queries", "0", "repo"], None, None);
    let unrelated = issue("schema.required", &["queries", "0", "token"], None, None);
    let repaired = repair_input(&input, &[url.clone(), missing.clone()], always).expect("settled");
    assert_eq!(
        repaired.input,
        json!({"queries":[{"path":"a.rs","owner":"o","repo":"r","ref":"main"}]})
    );
    assert!(repaired.dropped.is_empty());
    assert_eq!(repaired.approximate, ["queries.0.path", "queries.0.ref"]);
    assert!(repair_input(&input, &[url, missing, unrelated], always).is_none());
}

#[test]
fn dropping_an_unmapped_caller_field_is_named_as_lossy() {
    let input = json!({"queries":[{"matchString":"x","glob":"*.ts"}]});
    let unknown = issue(
        "schema.unknown-field",
        &["queries", "0", "glob"],
        Some(json!({"knownFields":["matchString","include","path"]})),
        None,
    );
    let repaired = repair_input(&input, &[unknown], always).expect("lossy repair");
    assert_eq!(repaired.input, json!({"queries":[{"matchString":"x"}]}));
    assert_eq!(repaired.dropped, ["queries.0.glob"]);
}

/// A field inert without a sibling only the caller can fill is removed
/// exactly; one whose form the row's own selector excludes is dropped,
/// named.
#[test]
fn a_field_inert_without_its_sibling_is_removed_or_dropped() {
    let input = json!({"queries":[{"path":"a","contextLines":2}]});
    let inert = |extra: Value| {
        let mut schema = json!({"knownFields":["path"],"siblingRequires":["matchString"]});
        schema
            .as_object_mut()
            .expect("object")
            .extend(extra.as_object().cloned().unwrap_or_default());
        issue(
            "schema.unknown-field",
            &["queries", "0", "contextLines"],
            Some(schema),
            None,
        )
    };
    let removed = repair_input(&input, &[inert(json!({}))], always).expect("exact repair");
    assert_eq!(removed.input, json!({"queries":[{"path":"a"}]}));
    assert!(removed.dropped.is_empty());
    let conflict = inert(json!({"siblingConflict": true}));
    let dropped = repair_input(&input, &[conflict], always).expect("lossy repair");
    assert_eq!(dropped.input, json!({"queries":[{"path":"a"}]}));
    assert_eq!(dropped.dropped, ["queries.0.contextLines"]);
}

/// An unknown field named for a kind of value one row field takes
/// becomes that field, exactly; a row already setting it keeps today's
/// handling (the unknown field is dropped, named).
#[test]
fn a_field_named_for_a_kind_of_value_becomes_the_field_that_takes_it() {
    let unknown = |path: &[&str], kinds: Value| {
        issue(
            "schema.unknown-field",
            path,
            Some(json!({"knownFields":["ref","path"],"kindFields":kinds})),
            None,
        )
    };
    let input = json!({"queries":[{"branch":"main","path":"a"}]});
    let repaired = repair_input(
        &input,
        &[unknown(&["queries", "0", "branch"], json!(["ref"]))],
        always,
    )
    .expect("exact");
    assert_eq!(
        repaired.input,
        json!({"queries":[{"ref":"main","path":"a"}]})
    );
    assert!(repaired.dropped.is_empty() && repaired.approximate.is_empty());
    let both = json!({"queries":[{"branch":"main","ref":"v1"}]});
    let repaired = repair_input(
        &both,
        &[unknown(&["queries", "0", "branch"], json!(["ref"]))],
        always,
    )
    .expect("lossy");
    assert_eq!(repaired.dropped, ["queries.0.branch"]);
    assert_eq!(repaired.input, json!({"queries":[{"ref":"v1"}]}));
    // Several fields take that kind: only the caller knows which.
    let repaired = repair_input(
        &input,
        &[unknown(&["queries", "0", "branch"], json!(["ref", "path"]))],
        always,
    )
    .expect("lossy");
    assert_eq!(repaired.dropped, ["queries.0.branch"]);
}

/// A path spelled from the current directory names the same path
/// without `./`, when the field accepts it; `../` names another.
#[test]
fn a_dot_relative_path_becomes_the_bare_path() {
    let path = json!({"type":"string","pattern":"^[A-Za-z][^/]*(?:/[A-Za-z][^/]*)*$"});
    let lead = |text: &str| {
        let input = json!({"queries":[{"path": text}]});
        let problem = issue(
            "schema.pattern",
            &["queries", "0", "path"],
            Some(path.clone()),
            Some(json!(text)),
        );
        repair_input(&input, &[problem], always).map(|repaired| {
            assert!(repaired.dropped.is_empty() && repaired.approximate.is_empty());
            repaired.input["queries"][0]["path"].clone()
        })
    };
    assert_eq!(lead("./crates"), Some(json!("crates")));
    assert_eq!(lead("././crates/a"), Some(json!("crates/a")));
    assert_eq!(lead("../crates"), None);
    assert_eq!(lead("./"), None);
}

/// An unknown field a known one expresses (`x-covers`: `limit` for
/// `pageSize`) is never dropped: the caller meant that field.
#[test]
fn a_covered_unknown_field_has_no_lead() {
    let input = json!({"queries":[{"path":".","limit":2}]});
    let unknown = issue(
        "schema.unknown-field",
        &["queries", "0", "limit"],
        Some(json!({"knownFields":["path","pageSize"],"coverFields":["pageSize"]})),
        None,
    );
    assert!(repair_input(&input, &[unknown], always).is_none());
}

#[test]
fn a_repair_the_caller_refuses_is_not_offered() {
    let input = json!({"queries":[{"extra":1}]});
    let unknown = issue(
        "schema.unknown-field",
        &["queries", "0", "extra"],
        None,
        None,
    );
    assert!(repair_input(&input, &[unknown], only(|_| false)).is_none());
}
