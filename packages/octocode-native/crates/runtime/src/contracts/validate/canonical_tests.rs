use super::{format_input_error, validate};
use crate::contracts::{prepare_rows, prepare_saved_rows};
use serde_json::{Value, json};

#[test]
fn every_core_fixture_has_the_same_admission_and_defaults_in_native() {
    let fixtures: Vec<Value> =
        serde_json::from_str(crate::contracts::generated::CONTRACT_FIXTURES_JSON).unwrap();
    let mut failures = Vec::new();
    for fixture in fixtures {
        let tool = fixture["tool"].as_str().unwrap();
        let result = validate(tool, fixture["input"].clone());
        if result.is_ok() != fixture["accepted"].as_bool().unwrap() {
            failures.push(format!("{}: {result:?}", fixture["id"]));
        } else if let (Err(error), Some(expected)) = (&result, fixture.get("issues")) {
            let actual: Vec<Value> = error
                .issues
                .iter()
                .map(|issue| {
                    json!({
                        "path": issue.path,
                        "message": issue.message,
                    })
                })
                .collect();
            let mut expected = expected.clone();
            for issue in expected.as_array_mut().unwrap() {
                for part in issue["path"].as_array_mut().unwrap() {
                    if !part.is_string() {
                        *part = Value::from(part.to_string());
                    }
                }
            }
            if Value::from(actual) != expected {
                failures.push(format!(
                    "{}: diagnostics differ: {error:?}; expected {expected}",
                    fixture["id"]
                ));
            }
        } else if let (Ok(actual), Some(expected)) = (result, fixture.get("normalized"))
            && &actual != expected
        {
            failures.push(format!("{}: defaults differ: {actual}", fixture["id"]));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn only_canonical_envelopes_reach_preparation() {
    let row = json!({"path":"/tmp/source.rs"});
    for rejected in [
        row.clone(),
        json!([row.clone()]),
        json!({}),
        json!({"queries":[{"queries":[row.clone()]}]}),
        json!({"queries":[row.clone()],"cursor":"issued"}),
        json!({"cursor":"issued","responseChars":1000}),
        json!({"responseSnapshot":"old"}),
    ] {
        assert!(prepare_rows("localFetch", rejected).is_err());
    }
    assert!(prepare_rows("localFetch", json!({"queries":[row]})).is_ok());
    assert!(
        prepare_rows("localFetch", json!({"cursor":"issued"}))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn private_progress_is_accepted_only_from_the_saved_request_path() {
    let input =
        json!({"queries":[{"path":"/tmp/source.rs","readProgress":{"unit":"lines","offset":3}}]});
    let error = prepare_rows("localFetch", input.clone()).unwrap_err();
    assert!(
        error
            .issues
            .iter()
            .any(|issue| issue.path == ["queries", "0", "readProgress"])
    );
    let saved = prepare_saved_rows("localFetch", input).unwrap();
    assert_eq!(saved[0].as_ref().unwrap()["readProgress"]["offset"], 3);
    assert!(
        prepare_saved_rows(
            "localFetch",
            json!({"queries":[{"path":"a","readProgress":{"unit":"lines","offset":-1}}]})
        )
        .is_err()
    );
}

#[test]
fn batch_errors_keep_valid_rows_and_original_indexes() {
    let rows = prepare_rows(
        "localFetch",
        json!({"queries":[
            {"path":"/tmp/first.rs"},{"path":"/tmp/bad.rs","mystery":true},{"path":"/tmp/last.rs"}
        ]}),
    )
    .unwrap();
    assert_eq!(rows[0].as_ref().unwrap()["path"], "/tmp/first.rs");
    assert_eq!(rows[2].as_ref().unwrap()["path"], "/tmp/last.rs");
    let error = rows[1].as_ref().unwrap_err();
    assert!(
        error
            .issues
            .iter()
            .all(|issue| issue.path.starts_with(&["queries".into(), "1".into()]))
    );
    let message = format_input_error("localFetch", error).to_string();
    assert!(message.contains("mystery") && message.contains("Allowed fields"));
}

#[test]
fn invalid_input_help_lists_only_fields_agents_can_author() {
    let error = prepare_rows(
        "localFetch",
        json!({"queries":[{"path":"/tmp/source.rs","mystery":true}]}),
    )
    .unwrap_err();
    let known = &error.issues[0].schema.as_ref().unwrap()["knownFields"];
    assert!(known.as_array().unwrap().contains(&json!("path")));
    for field in crate::contracts::state_fields(crate::id::ToolId::LocalFetch) {
        assert!(
            !known.as_array().unwrap().contains(&json!(field)),
            "private cursor field advertised as authored input: {field}"
        );
    }
}

#[test]
fn admission_preserves_content_and_does_not_rewrite_legacy_inputs() {
    let input = json!({"queries":[{"operation":"evaluate","content":{"text":"  exact evidence\n"},"context":"  context\n","questions":{"relevant":{"type":"noul","instructions":"Relevant?"}}}]});
    let parsed = validate("clasify", input.clone()).unwrap();
    for field in ["content", "context", "questions"] {
        assert_eq!(parsed["queries"][0][field], input["queries"][0][field]);
    }
    for (field, value) in [
        ("extras", json!([])),
        ("length", json!(5)),
        ("ranges", json!(["1-2"])),
        ("matchString", json!("x")),
        ("cursor", json!("old")),
    ] {
        let mut row = json!({"path":"/tmp/source.rs"});
        row[field] = value;
        assert!(
            validate("localFetch", json!({"queries":[row]})).is_err(),
            "{field}"
        );
    }
}

#[test]
fn every_tool_row_accepts_the_brief_and_bounds_it() {
    let row = json!({"path":"/tmp/source.rs","mainGoal":"Find the retry policy","reasoning":"The reader owns it"});
    let parsed = validate("localFetch", json!({"queries":[row.clone()]})).expect("brief accepted");
    assert_eq!(parsed["queries"][0]["mainGoal"], row["mainGoal"]);
    // A saved request keeps the brief beside its private progress.
    let saved = json!({"queries":[{"path":"/tmp/source.rs","mainGoal":"Find the retry policy","reasoning":"The reader owns it","readProgress":{"unit":"lines","offset":3}}]});
    let rows = prepare_saved_rows("localFetch", saved).expect("saved brief accepted");
    let row = rows[0].as_ref().expect("row");
    assert_eq!(row["mainGoal"], "Find the retry policy");
    assert_eq!(row["reasoning"], "The reader owns it");
    let long = json!({"path":"/tmp/source.rs","reasoning":"x".repeat(501)});
    assert!(validate("localFetch", json!({"queries":[long]})).is_err());
}

#[test]
fn invalid_rows_cannot_hide_an_oversized_batch() {
    let mut queries = vec![json!({"path":"source.rs"}); 5];
    queries.push(json!({"path":"source.rs","unknown":true}));
    let error = prepare_rows("localFetch", json!({"queries":queries})).unwrap_err();
    assert!(error.issues.iter().any(|issue| issue.path == ["queries"]));
}
