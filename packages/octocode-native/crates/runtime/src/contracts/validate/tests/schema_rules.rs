use super::*;

#[test]
fn pure_clasify_preserves_provider_entries_and_rejects_removed_fields() {
    let query = json!({"id":"decision","reasoning":"Decide the next evidence read.","mainGoal":"Files that decide the next read.",
            "state": {"observation": true},
            "questions":{"answer":{"type": "noul", "instructions": {"prompt":"Assess supplied state"}, "criteria":{"true":null,"false":null}}}});
    let prepared = prepare_and_validate("clasify", query.clone())
        .expect("a pure provider query needs no workflow fields");
    assert_eq!(prepared, query);
    let validated =
        validate("clasify", json!({"queries": [query.clone()]})).expect("pure provider envelope");
    assert_eq!(validated["queries"][0], query);
    let mut padded = query.clone();
    padded["reasoning"] = json!("  Decide the next evidence read.  ");
    assert_eq!(prepare_and_validate("clasify", padded).unwrap(), query);
    for field in ["debug", "route", "sources", "resources", "carry"] {
        let mut invalid = query.clone();
        invalid[field] = json!("not part of the provider protocol");
        assert!(prepare_and_validate("clasify", invalid).is_err(), "{field}");
    }
    for field in ["state", "questions"] {
        let mut invalid = query.clone();
        invalid.as_object_mut().expect("query object").remove(field);
        assert!(
            prepare_and_validate("clasify", invalid).is_err(),
            "missing {field}"
        );
    }
    let mut blank_reasoning = query.clone();
    blank_reasoning["reasoning"] = json!("");
    let dropped =
        prepare_and_validate("clasify", blank_reasoning).expect("blank reasoning is dropped");
    assert!(dropped.get("reasoning").is_none());
}

#[test]
fn validates_local_fetch_and_applies_schema_defaults() {
    let output = validate(
            "localFetch",
            json!({"queries":[{"path":"/tmp/a","mainGoal":"Read the fixture.","reasoning":"The next step needs these lines."}]}),
        )
        .expect("valid query");
    assert_eq!(output["queries"][0]["mainGoal"], "Read the fixture.");
    assert!(
        validate(
            "localFetch",
            json!({"queries":[{"path":"/tmp/a","reasoning":"The next step needs these lines."}]}),
        )
        .is_ok(),
        "mainGoal is optional"
    );
    assert!(
        validate(
            "localFetch",
            json!({"queries":[{"path":"/tmp/a","mainGoal":"Read the fixture."}]}),
        )
        .is_ok(),
        "reasoning is optional"
    );
}

#[test]
fn rejects_local_fetch_relations_and_unknown_fields() {
    let relation = validate(
            "localFetch",
            json!({"queries":[{"path":"/tmp/a","fullContent":true,"length":2,"mainGoal": "test", "reasoning":"Read the complete fixture."}]}),
        )
        .expect_err("invalid relation");
    // `length` is a valid window control, mutually exclusive with
    // fullContent, so the relation is rejected as a field conflict rather
    // than an unknown field.
    assert_eq!(
        format_input_error("localFetch", &relation),
        json!({
            "error":"queries.0.length: length windows a read; fullContent reads the whole file. Remove length."
        })
    );
    assert!(
            validate(
                "localFetch",
                json!({"queries":[{"path":"/tmp/a","wat":true,"mainGoal": "test", "reasoning":"Exercise unknown-field validation."}]})
            )
            .is_err()
        );
}

#[test]
fn aggregates_schema_violations_across_fields_and_queries() {
    let error = validate(
        "localFetch",
        json!({
            "queries": [
                {"wat": true, "alsoWat": 1},
                {"path": 7, "offset": "bad", "fullContent": "bad"}
            ]
        }),
    )
    .expect_err("invalid fields");
    let keys = error
        .issues
        .iter()
        .map(|issue| (issue.rule_id.as_str(), issue.path.join(".")))
        .collect::<Vec<_>>();
    assert!(keys.contains(&("schema.unknown-field", "queries.0.wat".into())));
    assert!(keys.contains(&("schema.unknown-field", "queries.0.alsoWat".into())));
    assert!(keys.contains(&("schema.required", "queries.0.path".into())));
    assert!(keys.contains(&("schema.type", "queries.1.path".into())));
    assert!(keys.contains(&("schema.type", "queries.1.offset".into())));
    assert!(keys.contains(&("schema.type", "queries.1.fullContent".into())));
}

#[test]
fn blank_optional_source_selectors_survive_normalization_and_fail_validation() {
    let github = json!({"queries":[{
        "owner":"octocat","repo":"Hello-World","path":"README.md",
        "mainGoal":"Read the requested source version.",
        "reasoning":"Keep the caller's source selector exact."
    }]});
    let package = json!({"queries":[{
        "ecosystem":"npm","packageName":"zod",
        "mainGoal":"Read the requested package version.",
        "reasoning":"Keep the caller's source selector exact."
    }]});
    for (tool, mut input, field, value) in [
        ("ghGetFileContent", github.clone(), "ref", json!("  ")),
        ("ghGetFileContent", github, "ranges", json!([])),
        ("artifactSearch", package, "version", json!(" ")),
    ] {
        input["queries"][0][field] = value.clone();
        let normalized = normalize_input(tool, input);
        assert_eq!(
            normalized["queries"][0][field], value,
            "{tool}.{field}: {normalized}"
        );
        let error =
            validate(tool, normalized).expect_err("blank selector must fail before dispatch");
        assert!(
            error
                .issues
                .iter()
                .any(|issue| issue.path.join(".").ends_with(field)),
            "{tool}.{field}: {error:?}"
        );
    }
}

/// Admission accepts exactly the canonical input language: a value the
/// published schema rejects is rejected with a repair hint, never
/// repaired (no string-to-boolean/number, enum casing or plurals,
/// encoded or bare-scalar lists, or line-range spellings).
#[test]
fn admission_rejects_what_the_canonical_schema_rejects() {
    let admit = |tool: &str, row: Value| {
        crate::contracts::prepare_many_and_validate(
            tool,
            normalize_input(tool, json!({"queries":[row]})),
        )
    };
    let rejected = [
        (
            "localFetch",
            json!({"path":"a.rs","fullContent":"true"}),
            "send true",
        ),
        (
            "localFetch",
            json!({"path":"a.rs","minify":"none"}),
            "allowed:",
        ),
        (
            "localFetch",
            json!({"path":"a.rs","contextLines":"3"}),
            "send 3",
        ),
        (
            "localFetch",
            json!({"path":"a.rs","ranges":[" 140-150"]}),
            "ranges",
        ),
        (
            "localFetch",
            json!({"path":"a.rs","ranges":["190"]}),
            "ranges",
        ),
        (
            "localFetch",
            json!({"path":"a.rs","ranges":[["248","325"]]}),
            "ranges",
        ),
        (
            "localFetch",
            json!({"path":"a.rs","ranges":"[\"1-5\"]"}),
            "JSON-encoded",
        ),
        (
            "localSearch",
            json!({"path":".","matchString":"x","include":"[\"*.go\"]"}),
            "JSON-encoded",
        ),
        (
            "localSearch",
            json!({"path":".","matchString":"x","include":"*.go"}),
            "wrap the value",
        ),
        (
            "ghSearchHistory",
            json!({"operation":"pullRequests","owner":"o","repo":"r"}),
            "operation",
        ),
    ];
    for (tool, row, hint) in rejected {
        let error = admit(tool, row.clone()).expect_err(&format!("{tool} {row}"));
        let formatted = format_input_error(tool, &error).to_string();
        assert!(formatted.contains(hint), "{tool} {row}: {formatted}");
    }
    let accepted = [
        ("localFetch", json!({"path":"a.rs","fullContent":true})),
        ("localFetch", json!({"path":"a.rs","minify":"symbols"})),
        ("localFetch", json!({"path":"a.rs","ranges":["140-150"]})),
        (
            "localSearch",
            json!({"path":".","matchString":"x","include":["*.go"]}),
        ),
    ];
    for (tool, row) in accepted {
        admit(tool, row.clone()).unwrap_or_else(|error| panic!("{tool} {row}: {error:?}"));
    }
}

/// `localSearch` takes `localFetch`'s `matchString` form: one text or a
/// list of 1 to 10 non-empty texts that match any.
#[test]
fn local_search_match_string_is_one_text_or_a_bounded_list() {
    let admit = |match_string: Value| {
        crate::contracts::prepare_many_and_validate(
            "localSearch",
            normalize_input(
                "localSearch",
                json!({"queries":[{"path":".","matchString":match_string}]}),
            ),
        )
    };
    let many = |count: usize| (0..count).map(|n| format!("v{n}")).collect::<Vec<_>>();
    for accepted in [json!("x"), json!(["a", "b"]), json!(many(10))] {
        admit(accepted.clone()).unwrap_or_else(|error| panic!("{accepted}: {error:?}"));
    }
    for rejected in [json!(""), json!([]), json!(["a", ""]), json!(many(11))] {
        assert!(admit(rejected.clone()).is_err(), "{rejected}");
    }
}

#[test]
fn matches_generated_reference_corpus() {
    let fixtures: Value = serde_json::from_str(crate::contracts::generated::CONTRACT_FIXTURES_JSON)
        .expect("generated fixture JSON");
    // Clasify's batch relation rules run at engine admission; its
    // fixtures are checked there (`tools::clasify::admission`).
    for fixture in fixtures.as_array().expect("fixture array") {
        if fixture["tool"] == "clasify" {
            continue;
        }
        let tool = fixture["tool"].as_str().expect("tool");
        let result = crate::contracts::prepare_many_and_validate(tool, fixture["input"].clone());
        assert_eq!(
            result.is_ok(),
            fixture["accepted"].as_bool().expect("accepted"),
            "{}: {result:?}",
            fixture["id"],
        );
        if let Ok(rows) = result {
            let normalized = super::super::validate(tool, fixture["input"].clone())
                .expect("accepted fixture validates as a complete envelope");
            assert_eq!(normalized, fixture["normalized"], "{}", fixture["id"]);
            let expected_rows = fixture["normalized"]
                .get("queries")
                .and_then(Value::as_array)
                .map_or(&[][..], Vec::as_slice);
            assert_eq!(rows, expected_rows, "{}", fixture["id"]);
        }
    }
}
