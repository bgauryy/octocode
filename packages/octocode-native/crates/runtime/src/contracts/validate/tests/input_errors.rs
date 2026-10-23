use super::*;

#[test]
fn guessed_fields_point_at_a_field_the_schema_accepts() {
    let known = ["operation", "keywords", "contextLines", "reasoning"];
    assert_eq!(
        super::super::suggest_field("keywordsToSearch", &known),
        Some("keywords")
    );
    assert_eq!(
        super::super::suggest_field("operaton", &known),
        Some("operation")
    );
    assert_eq!(super::super::suggest_field("pattern", &known), None);
}

#[test]
fn tool_error_envelope_is_the_error_and_details_only_when_more() {
    assert_eq!(tool_error("bad", Vec::new()), json!({"error":"bad"}));
    assert_eq!(
        tool_error("bad", vec!["fix".into()]),
        json!({"error":"bad","details":["fix"]})
    );
}

#[test]
fn formats_stable_cli_input_errors() {
    // A malformed value is echoed beside the form it must take.
    let range = validate(
            "localFetch",
            json!({"queries":[{"path":"/tmp/a","ranges":["5:2"],"mainGoal": "test", "reasoning":"Exercise range validation."}]}),
        )
        .expect_err("range");
    assert_eq!(
        format_input_error("localFetch", &range),
        json!({"error":"queries.0.ranges.0: \"5:2\" is not a line range; ranges: [\"299-342\"] (strings \"start-end\")"})
    );
    // A span spelled another way is named as the span it spells.
    let typo = validate(
        "localFetch",
        json!({"queries":[{"path":"/tmp/a","ranges":["3:6"]}]}),
    )
    .expect_err("range typo");
    assert_eq!(
        format_input_error("localFetch", &typo),
        json!({"error":"queries.0.ranges.0: \"3:6\" is not a line range (did you mean \"3-6\"?)"})
    );
    // A blank brief carries nothing: it is dropped, not rejected.
    let blank = validate(
        "localFetch",
        json!({"queries":[
                {"path":"/tmp/a","mainGoal":"ok","reasoning":"ok"},
                {"path":"/tmp/a","mainGoal":"  ","reasoning":""}]}),
    )
    .expect("blank brief is dropped");
    assert!(blank["queries"][1].get("mainGoal").is_none());
    assert!(blank["queries"][1].get("reasoning").is_none());
    let blank_path = validate(
        "localFetch",
        json!({"queries":[{"path":" ","mainGoal":"ok","reasoning":"ok"}]}),
    )
    .expect_err("blank path");
    assert_eq!(
        format_input_error("localFetch", &blank_path),
        json!({"error":"queries.0.path: is empty; give non-blank text."})
    );
    let unknown = validate(
            "localFetch",
            json!({"queries":[{"path":"/tmp/a","madeUp":true,"mainGoal": "test", "reasoning":"Exercise unknown-field validation."}]}),
        )
        .expect_err("unknown");
    assert_eq!(
        format_input_error("localFetch", &unknown),
        json!({"error":"Remove unknown field(s) from queries[0]: madeUp"})
    );
}

/// A `length` above the window cap names the cap and the same read at it,
/// and says where the rest is.
#[test]
fn a_length_over_the_cap_names_the_cap_and_the_fix() {
    for (unit, length) in [("bytes", 60_000), ("lines", 50_001)] {
        let error = validate(
                "localFetch",
                json!({"queries":[{"path":"/tmp/a","unit":unit,"length":length,"mainGoal":"test","reasoning":"Exercise the length cap."}]}),
            )
            .expect_err("length over the cap");
        let shaped = format_input_error("localFetch", &error);
        let text = shaped["error"].as_str().expect("error text");
        assert!(text.contains("allowed range (1-50000)"), "{shaped}");
        let lead = shaped["details"][0].as_str().expect("a lead");
        assert!(lead.contains("\"length\": 50000"), "{lead}");
        assert!(lead.contains("next"), "{lead}");
    }
}

/// An unknown key inside an `astSearch` rule object shows one valid
/// rule, as an engine rule error does.
#[test]
fn an_unknown_rule_key_shows_one_valid_rule() {
    let unknown = validate(
            "astSearch",
            json!({"queries":[{"operation":"match","path":"/tmp","rule":{"kind":"function_item","has":{"constraints":{}}},"mainGoal":"test","reasoning":"Exercise rule validation."}]}),
        )
        .expect_err("unknown rule key");
    let error = format_input_error("astSearch", &unknown);
    let text = error["error"].as_str().expect("error text").to_owned();
    assert!(text.contains("queries[0].rule.has"), "{text}");
    assert!(
        text.contains(&format!(
            "Example rule: {}",
            octocode_engine::structural::EXAMPLE_RULE
        )),
        "{text}"
    );
    let plain = validate(
            "localFetch",
            json!({"queries":[{"path":"/tmp/a","madeUp":true,"mainGoal":"t","reasoning":"Exercise unknown-field validation."}]}),
        )
        .expect_err("unknown");
    assert!(
        !format_input_error("localFetch", &plain)
            .to_string()
            .contains("Example rule")
    );
}

#[test]
fn input_errors_carry_no_static_schema_note() {
    let unknown = validate(
        "localFetch",
        json!({"queries":[
            {"path":"/tmp/a","mainGoal":"test","reasoning":"Valid row."},
            {"path":"/tmp/a","madeUp":true,"mainGoal":"test","reasoning":"Unknown field."}
        ]}),
    )
    .expect_err("unknown");
    // The per-call repair is the whole error: no CLI `schema` pointer and
    // no inputSchema pointer (that note lives once in the instructions).
    let formatted = format_input_error("localFetch", &unknown);
    let text = formatted.to_string();
    assert!(!text.contains("octocode schema"), "{text}");
    assert!(!text.contains("inputSchema"), "{text}");
    assert!(formatted.get("details").is_none(), "{formatted}");
    assert!(
        formatted["error"]
            .as_str()
            .is_some_and(|error| error.contains("queries[1]") && error.contains("madeUp")),
        "{formatted}"
    );
}

#[test]
fn unrepairable_list_values_get_honest_array_hints() {
    let reject = |include: Value| {
        let error = validate(
                "localSearch",
                json!({"queries":[{"path":"/tmp","matchString":"x","include":include,"mainGoal":"test","reasoning":"Hint."}]}),
            )
            .expect_err("not an array");
        error.issues[0].message.clone()
    };
    assert_eq!(
        reject(json!("[\"*.go\"")),
        "Expected array; send a JSON array, not a JSON-encoded string"
    );
    assert_eq!(reject(json!({"a":1})), "Expected array");
}

/// A guessed field points at the accepted field the schema names, never
/// at an unrelated near-spelling.
#[test]
fn guessed_field_names_suggest_the_accepted_field() {
    let local_search = ["matchString", "include", "regex", "includeHidden", "page"];
    assert_eq!(
        super::super::suggest_field("includes", &local_search),
        Some("include")
    );
    assert_eq!(
        super::super::suggest_field("regexp", &local_search),
        Some("regex")
    );
    assert_eq!(super::super::suggest_field("mode", &local_search), None);
    let structure = ["path", "maxDepth", "debug", "include", "detail"];
    assert_eq!(
        super::super::suggest_field("maxdepth", &structure),
        Some("maxDepth")
    );
    assert_eq!(
        super::super::suggest_field("depth", &["debug", "detail"]),
        None,
        "a short name never matches an unrelated field three edits away"
    );
    // One name ends with the other: a qualifier word added or dropped.
    assert_eq!(
        super::super::suggest_field("depth", &structure),
        Some("maxDepth")
    );
    assert_eq!(
        super::super::suggest_field("isRegex", &local_search),
        Some("regex")
    );
    assert_eq!(
        super::super::suggest_field("filePattern", &local_search),
        None
    );
    // A short name needs one edit and its first letter; another word in
    // front names another field (`lines` is no `contextLines`).
    assert_eq!(
        super::super::suggest_field("name", &["page", "path", "include"]),
        None
    );
    assert_eq!(
        super::super::suggest_field("pth", &["page", "path"]),
        Some("path")
    );
    assert_eq!(
        super::super::suggest_field("lines", &["contextLines", "ranges"]),
        None
    );
    assert_eq!(
        super::super::suggest_field("responseLength", &["length", "offset"]),
        None
    );
    assert_eq!(
        super::super::suggest_field("hidden", &["includeHidden"]),
        Some("includeHidden")
    );
}

/// An unknown field a known one expresses (`x-covers`) names that field.
#[test]
fn a_covered_unknown_field_names_the_field_that_expresses_it() {
    let error = super::super::ContractValidationError {
        issues: vec![super::super::ValidationIssue {
            rule_id: "schema.unknown-field".into(),
            path: vec!["queries".into(), "0".into(), "limit".into()],
            message: "Unknown field: limit".into(),
            schema: Some(json!({"knownFields":["path","matchString","pageSize"],
                    "coverFields":["pageSize"],"rowFields":["path","matchString","limit"]})),
            received: None,
        }],
    };
    assert_eq!(
        format_input_error("localSearch", &error),
        json!({"error":"Remove unknown field 'limit' from queries[0]: 'pageSize' expresses it."})
    );
}

/// Every form of a workspace-symbol query lacks one field: the error
/// names each alternative.
#[test]
fn a_missing_field_names_the_forms_alternative() {
    let error = validate(
        "lspSearch",
        json!({"queries":[{"operation":"workspaceSymbol","symbolName":"run"}]}),
    )
    .expect_err("path or workspaceRoot");
    let formatted = format_input_error("lspSearch", &error);
    assert_eq!(
        formatted["error"],
        "queries.0.path: Missing required field: path; or send workspaceRoot instead",
        "{formatted}"
    );
}

/// No legacy spelling maps to a current field: `uri` is simply unknown.
#[test]
fn a_retired_field_name_has_no_alias() {
    let lsp_search = ["operation", "path", "symbolName", "line", "graphDepth"];
    assert_eq!(super::super::suggest_field("uri", &lsp_search), None);
}
