use super::*;
use octocode_contract_types::ErrorCode;

fn parsed_ids(rows: &[Value]) -> Vec<String> {
    parse_rows(rows).into_iter().map(|row| row.id).collect()
}

#[test]
fn auto_ids_do_not_collide_with_earlier_explicit_ids() {
    assert_eq!(
        parsed_ids(&[
            json!({"operation":"models", "id":"q1"}),
            json!({"operation":"models"}),
        ]),
        ["q1", "q2"]
    );
}

#[test]
fn auto_ids_reserve_later_explicit_ids() {
    assert_eq!(
        parsed_ids(&[
            json!({"operation":"models"}),
            json!({"operation":"models", "id":"q0"}),
        ]),
        ["q1", "q0"]
    );
}

#[test]
fn auto_ids_skip_already_generated_ids() {
    assert_eq!(
        parsed_ids(&[
            json!({"operation":"models", "id":"q1"}),
            json!({"operation":"models"}),
            json!({"operation":"models"}),
        ]),
        ["q1", "q2", "q3"]
    );
}

#[test]
fn auto_ids_keep_free_positional_ids_and_explicit_names() {
    assert_eq!(
        parsed_ids(&[
            json!({"operation":"models"}),
            json!({"operation":"models", "id":"chosen"}),
            json!({"operation":"models"}),
        ]),
        ["q0", "chosen", "q2"]
    );
}

/// A sections row fails only when no resource was read: then as its
/// reads did, one code, else a failed context.
#[test]
fn a_sections_row_with_no_resource_read_fails_like_its_reads() {
    let resource =
        |status: &str, code: &str| json!({"status":status,"error":{"errorCode":code,"error":"x"}});
    let row = |resources: Value| json!({"operation":"sections","resources":resources});
    let missing = resource("error", "pathNotFound");
    assert_eq!(
        unread_resources_code(&row(json!({"a":missing,"b":missing}))),
        Some(ErrorCode::PathNotFound)
    );
    assert_eq!(
        unread_resources_code(&row(
            json!({"a":missing,"b":resource("error","outsideAllowedRoots")})
        )),
        Some(ErrorCode::ClassificationContextFailed)
    );
    for other in [
        json!({"status":"ready"}),
        resource("unsupported", "classificationContextTooLarge"),
    ] {
        assert_eq!(
            unread_resources_code(&row(json!({"a":missing,"b":other}))),
            None
        );
    }
    assert_eq!(unread_resources_code(&row(json!({}))), None);
}

#[test]
fn duplicate_query_ids_reject_with_the_core_wording() {
    let rows = [json!({"id":"a"}), json!({"id":"b"}), json!({"id":"a"})];
    let error = admit(rows.iter()).expect_err("duplicate");
    assert_eq!(error.issues[0].message, "Duplicate query id: a");
    assert_eq!(error.issues[0].path, ["queries", "2", "id"]);
    assert!(admit([json!({"id":"a"}), json!({})].iter()).is_ok());
}

#[test]
fn admission_matches_the_core_clasify_fixtures() {
    let fixtures: Value = serde_json::from_str(crate::contracts::generated::CONTRACT_FIXTURES_JSON)
        .expect("generated fixture JSON");
    let mut checked = 0;
    for fixture in fixtures.as_array().expect("fixture array") {
        if fixture["tool"] != "clasify" {
            continue;
        }
        let validated =
            crate::contracts::prepare_many_and_validate("clasify", fixture["input"].clone())
                .and_then(|rows| admit(rows.iter()));
        assert_eq!(
            validated.is_ok(),
            fixture["accepted"].as_bool().expect("accepted"),
            "{}: {validated:?}",
            fixture["id"],
        );
        if let Some(expected) = fixture.get("messages") {
            let messages = validated
                .expect_err("a worded fixture is rejected")
                .issues
                .into_iter()
                .map(|issue| Value::String(issue.message))
                .collect::<Vec<_>>();
            assert_eq!(&Value::Array(messages), expected, "{}", fixture["id"]);
        }
        checked += 1;
    }
    assert!(checked >= 10, "clasify fixtures: {checked}");
}
