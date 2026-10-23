use super::*;

/// Normalization only wraps a bare row as a one-row `queries`; it never
/// repairs a value (an encoded `queries` string, a string number), which
/// validation then rejects.
#[test]
fn normalization_only_wraps_a_bare_row() {
    let row =
        json!({"mainGoal":"g","reasoning":"r","path":"/tmp","matchString":"x","include":["*.ts"]});
    let expected = json!({"queries":[row.clone()]});
    let mut bare = row.clone();
    bare["responseLength"] = json!(100);
    let wrapped = normalize_input("localSearch", bare);
    assert_eq!(wrapped["queries"], expected["queries"]);
    assert_eq!(wrapped["responseLength"], 100);
    let prepared =
        prepare_many_and_validate("localSearch", wrapped).expect("a bare row validates as one row");
    assert_eq!(prepared.len(), 1);
    let encoded = serde_json::to_string(&json!([row])).expect("encode");
    let envelope = json!({"queries": encoded, "responseLength": "100"});
    assert_eq!(normalize_input("localSearch", envelope.clone()), envelope);
    assert!(prepare_many_and_validate("localSearch", envelope).is_err());
    // An array, or an object with only envelope fields, is not a row.
    assert_eq!(
        normalize_input("localSearch", json!([row.clone()])),
        json!([row])
    );
    let only_envelope = json!({"responseLength": 100});
    assert_eq!(
        normalize_input("localSearch", only_envelope.clone()),
        only_envelope
    );
    let error = prepare_many_and_validate("localSearch", json!([row]))
        .expect_err("an array is not the envelope");
    assert!(
        error.issues[0].message.contains("{\"queries\":[...]}"),
        "{error:?}"
    );
    // Not validated or defaulted: invalid rows and empty input pass through.
    let invalid = json!({"queries":[{"bogus":1}]});
    assert_eq!(normalize_input("localSearch", invalid.clone()), invalid);
    assert_eq!(normalize_input("localSearch", json!({})), json!({}));
}

#[test]
fn row_scoped_output_violations_degrade_to_row_errors() {
    let output = json!({"results":[
        {"index":0,"data":{"error":"upstream failed"}},
        {"index":1,"data":{"bogusKey":true}}
    ]});
    let error = validate_output("localSearch", &output).expect_err("row 1 violates");
    let patched = isolate_row_violations("localSearch", &output, &error).expect("isolated");
    assert_eq!(patched["results"][0]["data"]["error"], "upstream failed");
    assert!(patched["results"][0].get("status").is_none());
    assert_eq!(patched["results"][1]["status"], "error");
    assert_eq!(
        patched["results"][1]["data"]["errorCode"],
        "outputContractViolation"
    );
    assert!(validate_output("localSearch", &patched).is_ok());
    // Envelope-level violations stay whole-call failures.
    let envelope = json!({"results":"not-an-array"});
    let error = validate_output("localSearch", &envelope).expect_err("envelope violates");
    assert!(isolate_row_violations("localSearch", &envelope, &error).is_none());
}

#[test]
fn public_queries_take_an_optional_brief_and_drop_a_blank_one() {
    let omitted = prepare_and_validate("localFetch", json!({"path":"/tmp/source.rs"}))
        .expect("a call without a brief is accepted");
    assert!(omitted.get("mainGoal").is_none() && omitted.get("reasoning").is_none());
    let blank = prepare_and_validate(
        "localFetch",
        json!({"path":"/tmp/source.rs","mainGoal":"   ","reasoning":"   "}),
    )
    .expect("a blank brief is dropped, not rejected");
    assert!(blank.get("mainGoal").is_none() && blank.get("reasoning").is_none());
    let error = prepare_and_validate(
        "localFetch",
        json!({"path":"/tmp/source.rs","goal":" Read the source. "}),
    )
    .expect_err("goal is an unknown field, not a second name for mainGoal");
    assert_eq!(error.issues[0].rule_id, "schema.unknown-field");
    let ok = prepare_and_validate(
            "localFetch",
            json!({"path":"/tmp/source.rs","mainGoal":" Read the source. ","reasoning":" The next step needs these lines. "}),
        )
        .expect("a trimmed brief is accepted");
    assert_eq!(ok["mainGoal"], "Read the source.");
    assert_eq!(ok["reasoning"], "The next step needs these lines.");
}

#[test]
fn bulk_queries_are_defaulted_and_validated_with_stable_order() {
    let queries = prepare_many_and_validate(
        "localFetch",
        json!({"queries":[
            {"path":"/tmp/a","mainGoal": "test", "reasoning":"Read both."},
            {"path":"/tmp/b","mainGoal": "test", "reasoning":"Read both."}
        ]}),
    )
    .expect("valid bulk input");
    assert_eq!(queries.len(), 2);
    assert_eq!(queries[0]["path"], "/tmp/a");
    assert_eq!(queries[1]["path"], "/tmp/b");
    assert!(queries[0].get("extras").is_none());
    assert!(queries[1].get("extras").is_none());
    let split = prepare_many_and_validate(
        "localFetch",
        json!({"queries":[
            {"path":"/tmp/a","mainGoal":"Find the writer.","reasoning":"Read both."},
            {"path":"/tmp/b","mainGoal":"Find the caller.","reasoning":"Read one."}
        ]}),
    )
    .expect("each batch row carries its own goal and reasoning");
    assert_eq!(split[0]["mainGoal"], "Find the writer.");
    assert_eq!(split[1]["mainGoal"], "Find the caller.");
    assert_eq!(split[1]["reasoning"], "Read one.");
}

#[test]
fn a_clasify_row_keeps_its_keyed_questions_and_trims_its_brief() {
    let queries = prepare_many_and_validate(
        "clasify",
        json!({"queries":[{
            "id":"row",
            "reasoning":"  Classify the state.  ","mainGoal":"Decide the next read.",
            "state":{"text":"one"},
            "questions":{
                "q1":{"type":"noul","instructions":"Is it relevant?"},
                "q2":{"type":"score","instructions":"Rate risk","criteria":["low","high"]}
            }
        }]}),
    )
    .expect("valid clasify row");
    assert_eq!(queries.len(), 1);
    assert_eq!(queries[0]["state"], json!({"text":"one"}));
    assert_eq!(queries[0]["questions"]["q1"]["type"], "noul");
    assert_eq!(
        queries[0]["questions"]["q2"]["criteria"],
        json!(["low", "high"])
    );
    assert_eq!(queries[0]["reasoning"], "Classify the state.");
}
