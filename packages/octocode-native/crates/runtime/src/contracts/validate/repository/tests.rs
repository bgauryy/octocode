use super::super::{repair_input, validate_input_schema};
use serde_json::{Value, json};

/// A row schema with a joined `owner`/`repo` pair (the core metadata),
/// a `ref` and a required file `path`.
fn schema(required: &[&str]) -> Value {
    let name = json!({"type":"string","pattern":"^[\\w.][\\w.-]*$"});
    let mut owner = name.clone();
    owner["x-joinedWith"] = json!("repo");
    json!({"type":"object","additionalProperties":false,"required":required,
            "properties":{"owner":owner,"repo":name,
                "ref":{"type":"string","pattern":"\\S"},
                "path":{"type":"string","pattern":"\\S"}}})
}

fn issues(schema: &Value, row: &Value) -> Vec<super::ValidationIssue> {
    validate_input_schema(schema, schema, &mut row.clone(), &mut Vec::new())
        .err()
        .map(|error| error.issues)
        .unwrap_or_default()
}

/// The repaired row, checked against the same schema; `None`: no lead.
fn repaired(schema: &Value, row: Value) -> Option<(Value, Vec<String>)> {
    let found = issues(schema, &row);
    assert!(!found.is_empty(), "{row} is rejected");
    let check = |candidate: &Value| match issues(schema, candidate) {
        found if found.is_empty() => Ok(()),
        found => Err(found),
    };
    repair_input(&row, &found, check).map(|repair| {
        assert!(repair.dropped.is_empty(), "coordinate repairs drop nothing");
        (repair.input, repair.approximate)
    })
}

/// A pull request, issue, commit or compare URL becomes the row's
/// coordinates, the operation its kind spells and the item's fields.
#[test]
fn an_item_url_becomes_the_operation_and_its_fields() {
    let check = |value: &Value| {
        crate::contracts::prepare_many_and_validate("ghGetHistoryItem", value.clone())
            .map(|_| ())
            .map_err(|error| error.issues)
    };
    let sha = "0123456789abcdef0123456789abcdef01234567";
    for (url, expected) in [
        (
            "https://github.com/o/r/pull/7".to_owned(),
            json!({"owner":"o","repo":"r","operation":"pullRequest","number":7}),
        ),
        (
            "https://github.com/o/r/issues/7".to_owned(),
            json!({"owner":"o","repo":"r","operation":"issue","number":7}),
        ),
        (
            format!("https://github.com/o/r/commit/{sha}"),
            json!({"owner":"o","repo":"r","operation":"commit","ref":sha}),
        ),
        (
            "https://github.com/o/r/compare/v1...v2".to_owned(),
            json!({"owner":"o","repo":"r","operation":"compare","base":"v1","head":"v2"}),
        ),
    ] {
        let input = json!({"queries":[{"owner": url}]});
        let found = check(&input).expect_err(&url);
        let repair = repair_input(&input, &found, check).expect(&url);
        assert_eq!(repair.input["queries"][0], expected, "{url}");
        assert!(repair.dropped.is_empty() && repair.approximate.is_empty());
    }
    // The row's own operation names another kind: no fix.
    let input = json!({"queries":[{"operation":"issue","owner":"https://github.com/o/r/pull/7"}]});
    let found = check(&input).expect_err("another kind");
    assert!(repair_input(&input, &found, check).is_none());
}

#[test]
fn a_joined_pair_in_one_half_splits_into_both() {
    let required = schema(&["owner", "repo"]);
    let optional = schema(&[]);
    for (schema, row, expected) in [
        (
            &required,
            json!({"owner":"a/b"}),
            Some(json!({"owner":"a","repo":"b"})),
        ),
        (
            &optional,
            json!({"repo":"a/b"}),
            Some(json!({"repo":"b","owner":"a"})),
        ),
        (
            &required,
            json!({"owner":"a/b","repo":"b"}),
            Some(json!({"owner":"a","repo":"b"})),
        ),
        (
            &required,
            json!({"owner":"a/b.git"}),
            Some(json!({"owner":"a","repo":"b"})),
        ),
        (&required, json!({"owner":"a/b","repo":"c"}), None),
        (&required, json!({"owner":"a/b/c"}), None),
    ] {
        let repair = repaired(schema, row.clone());
        assert_eq!(
            repair.as_ref().map(|(input, _)| input),
            expected.as_ref(),
            "{row}"
        );
        assert!(repair.is_none_or(|(_, approximate)| approximate.is_empty()));
    }
    // The error names the split once: the missing repo is settled by it.
    let found = issues(&required, &json!({"owner":"a/b"}));
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].message.ends_with("send owner:\"a\", repo:\"b\""),
        "{found:?}"
    );
}

#[test]
fn a_clone_suffix_on_the_second_half_is_dropped_exactly() {
    let schema = schema(&["owner", "repo"]);
    let (input, approximate) =
        repaired(&schema, json!({"owner":"o","repo":"is.git"})).expect("exact");
    assert_eq!(input, json!({"owner":"o","repo":"is"}));
    assert!(approximate.is_empty());
    assert!(issues(&schema, &json!({"owner":"o","repo":"gitlab"})).is_empty());
    assert!(issues(&schema, &json!({"owner":"o.git","repo":"r"})).is_empty());
}

#[test]
fn a_web_url_becomes_the_rows_coordinates() {
    let schema = schema(&["owner", "repo", "path"]);
    let sha = "0123456789abcdef0123456789abcdef01234567";
    for (row, expected, approximate) in [
        (
            json!({"path": format!("https://github.com/o/r/blob/{sha}/src/a.rs")}),
            json!({"path":"src/a.rs","owner":"o","repo":"r","ref":sha}),
            false,
        ),
        (
            json!({"path":"https://github.com/o/r/blob/main/src/a.rs#L3"}),
            json!({"path":"src/a.rs","owner":"o","repo":"r","ref":"main"}),
            true,
        ),
        (
            json!({"owner":"o","path":"https://raw.githubusercontent.com/o/r.git/v1/a.md"}),
            json!({"owner":"o","path":"a.md","repo":"r","ref":"v1"}),
            true,
        ),
        (
            json!({"owner":"github.com/o/r","path":"a.md"}),
            json!({"owner":"o","path":"a.md","repo":"r"}),
            false,
        ),
    ] {
        let (input, named) = repaired(&schema, row.clone()).expect("a lead");
        assert_eq!(input, expected, "{row}");
        assert_eq!(!named.is_empty(), approximate, "{row}: {named:?}");
    }
    // A line anchor becomes the span of the row's span field, exactly.
    let mut spans = schema.clone();
    spans["properties"]["ranges"] = json!({"type":"array","items":{"type":"string",
            "pattern":super::super::shape::LINE_RANGE_PATTERN}});
    let sha_url = format!("https://github.com/o/r/blob/{sha}/src/a.rs");
    for (anchor, expected) in [
        ("#L3-L9", Some("3-9")),
        ("#L4", Some("4-4")),
        ("#L9-L3", None),
    ] {
        let (input, named) =
            repaired(&spans, json!({"path": format!("{sha_url}{anchor}")})).expect("a lead");
        assert_eq!(
            input.get("ranges"),
            expected.map(|span| json!([span])).as_ref(),
            "{anchor}"
        );
        assert_eq!(named.is_empty(), expected.is_some(), "{anchor}: {named:?}");
    }
    // Another owner than the URL's, or no scheme on a file path: no fix.
    assert!(
        repaired(
            &schema,
            json!({"owner":"x","path":"https://github.com/o/r/blob/m/a"})
        )
        .is_none()
    );
    assert!(
        issues(
            &schema,
            &json!({"owner":"o","repo":"r","path":"github.com/o/r/blob/m/a"})
        )
        .is_empty()
    );
}
