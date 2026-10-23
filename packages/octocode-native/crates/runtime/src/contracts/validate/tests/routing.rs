use super::*;

#[test]
fn names_the_selector_a_sibling_branch_needs_for_a_rejected_literal() {
    let error = validate(
            "localSearch",
            json!({"queries":[{"path":"/tmp","matchString":"foo","unique":"list","mainGoal": "test", "reasoning":"List values."}]}),
        )
        .expect_err("unique:list needs matchOnly");
    let formatted = format_input_error("localSearch", &error);
    assert!(
        formatted["error"].as_str().is_some_and(
            |error| error.contains("unique:\"list\" requires resultView:\"matchOnly\"")
        ),
        "{formatted}"
    );
}

/// The MCP surface renders the same guidance from core Zod issues; the
/// MCP parity suite runs these inputs through both surfaces.
#[test]
fn guidance_names_every_form_alias_and_core_required_message() {
    let details = |tool: &str, mut row: Value| {
        row["mainGoal"] = json!("g");
        row["reasoning"] = json!("r");
        let error = validate(tool, json!({"queries":[row]})).expect_err(tool);
        let formatted = format_input_error(tool, &error);
        // The first repair is the error; any others follow in details.
        std::iter::once(&formatted["error"])
            .chain(formatted["details"].as_array().into_iter().flatten())
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join("\n")
    };
    let symbols = details(
        "astSearch",
        json!({"operation":"symbols","path":"/r","include":["*.ts"]}),
    );
    // `include` belongs to the match operation: adding a pattern to a
    // symbols query would not make it valid, so the operation is named.
    assert!(
        symbols.contains("it applies only with operation:\"match\"."),
        "{symbols}"
    );
    assert!(!symbols.contains("localSearch"), "in-tool field: {symbols}");
    let owner = details("ghSearchCode", json!({"keywords":["x"]}));
    assert!(
            owner.contains(
                "Missing required field: owner (Set owner (a GitHub user or org): code search cannot span all of GitHub"
            ),
            "{owner}"
        );
    let history = details(
        "ghSearchHistory",
        json!({"operation":"pullRequest","owner":"a","repo":"b","qualifers":"author:x"}),
    );
    assert!(history.contains("did you mean 'qualifiers'?"), "{history}");
}

#[test]
fn names_the_mode_of_a_field_declared_by_a_sibling_branch() {
    let error = validate(
            "artifactSearch",
            json!({"queries":[{"ecosystem":"npm","packageName":"zod","pageSize":3,"mainGoal": "test", "reasoning":"Exact lookup."}]}),
        )
        .expect_err("pageSize is discovery-only");
    let formatted = format_input_error("artifactSearch", &error);
    assert_eq!(
        formatted,
        json!({"error":"Remove 'pageSize' from queries[0]: it applies only with keywords."})
    );
}

#[test]
fn names_operations_when_no_branch_shares_the_query_mode() {
    let query = json!({
        "mainGoal":"g","reasoning":"r","operation":"issue",
        "owner":"o","repo":"r","number":1,"matchString":"thanks"
    });
    let error = validate("ghGetHistoryItem", json!({ "queries": [query] }))
        .expect_err("issues take no matchString");
    let formatted = format_input_error("ghGetHistoryItem", &error);
    let message = formatted["error"].as_str().unwrap_or_default();
    assert!(
        message.starts_with(
            "Remove 'matchString' from queries[0]: it applies only with operation one of"
        ),
        "{formatted}"
    );
    for operation in ["pullRequest", "commit", "compare"] {
        assert!(message.contains(operation), "{formatted}");
    }
}

#[test]
fn names_the_operation_that_declares_a_field_of_another_operation() {
    let query = json!({
        "mainGoal":"g","reasoning":"r","operation":"commit",
        "owner":"octocat","repo":"Hello-World","state":"open"
    });
    let error = validate("ghSearchHistory", json!({ "queries": [query] }))
        .expect_err("the field belongs to another operation");
    let formatted = format_input_error("ghSearchHistory", &error);
    assert_eq!(
        formatted["error"],
        "Remove 'state' from queries[0]: it applies only with operation one of \"pullRequest\", \"issue\".",
        "{formatted}"
    );
}

#[test]
fn offers_either_fix_for_a_field_of_clasify_sections() {
    let legacy = json!({
        "id":"legacy",
        "resources":[{"id":"evidence","value":"Supplied evidence."}],
        "questions":[{"id":"bounded","type":"yesno","ask":"Is evidence supplied?"}]
    });
    let error = validate("clasify", json!({"queries":[legacy]}))
        .expect_err("resources belongs to the sections form");
    assert_eq!(
        format_input_error("clasify", &error),
        json!({
            "error":"'resources' in queries[0] applies only with operation:\"sections\": set it, or remove 'resources'.",
            "details":[
                "queries.0.state: Missing required field: state",
                "queries.0.questions: Expected object {<name>: {type: noul|choice|score|box, criteria, instructions}}"
            ]
        })
    );
}

/// An unknown field whose value is one of a field's listed values names
/// that field: `{"analysis":"dependents"}` meant `operation`.
#[test]
fn an_unknown_field_holding_a_listed_value_suggests_its_field() {
    for (tool, row) in [
        (
            "structureSearch",
            json!({"type":"tree","path":".","mainGoal":"g","reasoning":"r"}),
        ),
        (
            "astSearch",
            json!({"analysis":"symbols","path":".","mainGoal":"g","reasoning":"r"}),
        ),
    ] {
        let error = validate(tool, json!({"queries":[row]})).expect_err("rejected");
        let formatted = format_input_error(tool, &error).to_string();
        assert!(
            formatted.contains("did you mean 'operation'?"),
            "{tool}: {formatted}"
        );
    }
}

#[test]
fn suggests_reasoning_for_a_typo_across_every_tool_shape() {
    // The enforcement IR carries no presentation examples; the accepted
    // parity fixtures are the generated per-tool query corpus instead.
    let fixtures: Value = serde_json::from_str(crate::contracts::generated::CONTRACT_FIXTURES_JSON)
        .expect("generated fixtures");
    let contract = crate::contracts::parsed_contract().expect("generated contract");
    for tool in contract["tools"].as_array().expect("tool array") {
        let name = tool["name"].as_str().expect("tool name");
        let mut query = fixtures
            .as_array()
            .expect("fixture array")
            .iter()
            .find(|fixture| fixture["tool"] == *name && fixture["accepted"] == Value::Bool(true))
            .expect("accepted fixture for every tool")["input"]["queries"][0]
            .clone();
        let object = query.as_object_mut().expect("query example");
        object.remove("reasoning");
        object.insert("reasonng".into(), json!("Exercise typo recovery."));
        let error = validate(name, json!({"queries":[query]})).expect_err("typo must be rejected");
        let formatted = format_input_error(name, &error).to_string();
        assert!(
            formatted.contains("did you mean 'reasoning'?"),
            "{name}: {formatted}"
        );
    }
}

#[test]
fn a_row_sent_to_the_wrong_tool_names_the_tool_that_owns_its_fields() {
    let error = validate(
            "localFetch",
            json!({"queries":[{"path":"package.json","matchString":"name","multiline":"on","resultView":"files","mainGoal":"g","reasoning":"r"}]}),
        )
        .expect_err("localSearch fields");
    let formatted = format_input_error("localFetch", &error).to_string();
    assert!(
        formatted.contains("multiline, resultView are localSearch fields"),
        "{formatted}"
    );

    let error = validate(
        "structureSearch",
        json!({"queries":[{"path":"packages","depth":2,"mainGoal":"g","reasoning":"r"}]}),
    )
    .expect_err("misspelled maxDepth");
    let formatted = format_input_error("structureSearch", &error).to_string();
    assert!(
        formatted.contains("did you mean 'maxDepth'?"),
        "{formatted}"
    );
    assert!(
        !formatted.contains("send queries[0] to"),
        "a field the tool can name is not rerouted: {formatted}"
    );

    let error = validate(
            "astSearch",
            json!({"queries":[{"path":"packages","operation":"files","pageSize":5,"mainGoal":"g","reasoning":"r"}]}),
        )
        .expect_err("structureSearch operation");
    let formatted = format_input_error("astSearch", &error).to_string();
    assert!(
        formatted.contains("operation \\\"files\\\" is a structureSearch operation"),
        "{formatted}"
    );
}

/// A row goes to another tool only when that tool runs the whole row:
/// it declares every field and requires none the row lacks.
#[test]
fn a_row_another_tool_cannot_run_is_not_sent_there() {
    for (tool, row) in [
        (
            "ghSearchCode",
            json!({"owner":"o","repo":"r","matchString":"x"}),
        ),
        (
            "localSearch",
            json!({"path":".","matchString":"x","extensions":["rs"]}),
        ),
    ] {
        let error = validate(tool, json!({ "queries": [row] })).expect_err(tool);
        let formatted = format_input_error(tool, &error).to_string();
        assert!(
            !formatted.contains("send queries[0] to"),
            "{tool}: {formatted}"
        );
    }
}
