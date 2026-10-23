use super::*;

#[test]
fn clasify_question_selectors_report_the_selected_forms_missing_field() {
    for (question, field) in [
        (
            json!({"type":"choice","instructions":"Choose a label"}),
            "criteria",
        ),
        (json!({"type":"score","instructions":"Rate it"}), "criteria"),
    ] {
        let error = prepare_many_and_validate(
            "clasify",
            json!({"queries":[{
                "id":"missing-question-field",
                "state":"Observed evidence",
                "questions":{"check":question}
            }]}),
        )
        .expect_err("the selected question lacks a required field");
        assert_eq!(error.issues.len(), 1, "{error:?}");
        assert_eq!(
            error.issues[0].path,
            ["queries", "0", "questions", "check", field]
        );
        assert_eq!(error.issues[0].rule_id, "schema.required");
    }
}

#[test]
fn clasify_noul_criteria_and_box_allow_missing_or_null_instructions() {
    for question in [
        json!({"type":"noul","criteria":{"true":"The state contains evidence."}}),
        json!({"type":"noul","instructions":null,"criteria":{"false":"The state is empty."}}),
        json!({"type":"box"}),
        json!({"type":"box","instructions":null}),
    ] {
        // A box question locates something in an image, so its row carries one.
        prepare_many_and_validate(
            "clasify",
            json!({"queries":[{
                "state":"Observed evidence",
                "assets":{"photo":{"type":"image","source":{"url":"https://example.org/cat.png"}}},
                "questions":{"check":question}
            }]}),
        )
        .expect("Noul with criteria and Box do not require instructions");
    }
}

#[test]
fn clasify_noul_rejects_missing_instructions_and_unusable_criteria() {
    for question in [
        json!({"type":"noul"}),
        json!({"type":"noul","instructions":null,"criteria":null}),
        json!({"type":"noul","instructions":" ","criteria":{"true":[],"false":{}}}),
    ] {
        assert!(
            prepare_many_and_validate(
                "clasify",
                json!({"queries":[{
                    "state":"Observed evidence",
                    "questions":{"check":question}
                }]}),
            )
            .is_err(),
            "a Noul question needs a usable predicate: {question}"
        );
    }
}

#[test]
fn missing_literal_selector_names_its_value_without_changing_plain_requirements() {
    let root = json!({"$defs":{"mode":{"const":"match"}}});
    let branch = json!({
        "properties":{"operation":{"$ref":"#/$defs/mode"},"pattern":{"type":"string"}},
        "required":["operation","pattern"]
    });
    assert_eq!(
        super::super::siblings::sibling_missing_fields(&root, &[&branch], &json!({}), "payload"),
        Some(vec![json!("operation:\"match\" and pattern")])
    );
}

/// A field of the one form whose omitted selector is a single literal
/// carries the fix that sets it, and so does one whose selector the row
/// spells at its default value (`operation:"tree"` says what omitting it
/// says): the fix keeps the field and switches the selector. A selector set to a value that is no default
/// (`operation:"hover"`) is the caller's choice: the field is that other
/// form's (`siblingConflict`).
#[test]
fn a_field_of_one_unselected_form_carries_its_selector_as_the_fix() {
    let issues = |tool: &str, row: Value| {
        prepare_many_and_validate(tool, json!({"queries":[row]}))
            .expect_err("a field of another form")
            .issues
    };
    let found = issues("structureSearch", json!({"path":".","nameRegex":"x"}));
    let schema = found[0].schema.as_ref().expect("annotated");
    assert_eq!(
        schema["setFields"],
        json!({"operation":"files"}),
        "{found:?}"
    );
    let explicit = issues(
        "structureSearch",
        json!({"operation":"tree","path":".","nameRegex":"x"}),
    );
    let schema = explicit[0].schema.as_ref().expect("annotated");
    assert_eq!(
        schema["setFields"],
        json!({"operation":"files"}),
        "{explicit:?}"
    );
    let input = json!({"queries":[{"operation":"tree","path":".","nameRegex":"x"}]});
    let repaired =
        crate::contracts::validate::repair_input(&input, &explicit, |_| Ok(())).expect("a repair");
    assert_eq!(
        repaired.input,
        json!({"queries":[{"operation":"files","path":".","nameRegex":"x"}]})
    );
    assert!(repaired.dropped.is_empty() && repaired.approximate.is_empty());
    let chosen = issues(
        "lspSearch",
        json!({"operation":"hover","path":"a.ts","symbolName":"x","line":1,"includeDeclaration":false}),
    );
    let schema = chosen[0].schema.as_ref().expect("annotated");
    assert!(schema.get("setFields").is_none(), "{chosen:?}");
    assert_eq!(schema["siblingConflict"], true, "{chosen:?}");
}

#[test]
fn section_fields_without_operation_name_the_sections_literal() {
    for (field, value) in [
        (
            "resources",
            json!({"file":{"tool":"localFetch","query":{"path":"a.rs"}}}),
        ),
        ("assessment", json!("opaque")),
    ] {
        let mut row = json!({"state":"evidence","questions":{"fact":{"type":"noul","instructions":"Supported?"}}});
        row[field] = value;
        let error = prepare_many_and_validate("clasify", json!({"queries":[row]}))
            .expect_err("section-only field on evaluation");
        let projected = crate::contracts::format_input_error("clasify", &error);
        assert_eq!(
            projected["error"],
            format!(
                "Remove '{field}' from queries[0]: it applies only with operation:\"sections\"."
            ),
            "{projected}"
        );
    }
}

/// A field that only another form's selector values accept names those
/// values, not "unknown field".
#[test]
fn a_field_of_another_selector_value_names_the_values_that_accept_it() {
    let error = prepare_many_and_validate(
        "artifactSearch",
        json!({"queries":[{"ecosystem":"pypi","keywords":["http"]}]}),
    )
    .expect_err("pypi has no keyword discovery");
    let projected = crate::contracts::format_input_error("artifactSearch", &error);
    assert!(
        projected["error"]
            .as_str()
            .is_some_and(|error| error.starts_with(
                "Remove 'keywords' from queries[0]: it applies only with ecosystem one of \
                 \"npm\", \"crates\""
            )),
        "{projected}"
    );
    assert!(
        !projected.to_string().contains("Unknown field"),
        "{projected}"
    );
}

/// An enum error lists the values of the branch the selector
/// chose, not every branch's merged enum (compare has no issue sections).
#[test]
fn enum_error_lists_only_the_selected_branch_values() {
    let error = prepare_many_and_validate(
        "ghGetHistoryItem",
        json!({"queries":[{
            "mainGoal":"g","reasoning":"r","operation":"compare",
            "owner":"o","repo":"r","base":"v1","head":"v2","sections":["bogus"]
        }]}),
    )
    .expect_err("bogus section");
    let message = error
        .issues
        .iter()
        .map(|issue| issue.message.as_str())
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(message.contains("patches"), "{message}");
    for foreign in ["body", "comments", "reviews"] {
        assert!(!message.contains(foreign), "{foreign}: {message}");
    }
    // A selector that matches no branch still lists every branch's value.
    let error = prepare_many_and_validate(
            "ghGetHistoryItem",
            json!({"queries":[{"mainGoal":"g","reasoning":"r","operation":"bogus","owner":"o","repo":"r"}]}),
        )
        .expect_err("bogus operation");
    let message = error
        .issues
        .iter()
        .map(|issue| issue.message.as_str())
        .collect::<Vec<_>>()
        .join(" | ");
    for operation in ["pullRequest", "issue", "commit", "compare"] {
        assert!(message.contains(operation), "{operation}: {message}");
    }
}

fn clasify_issues(query: Value) -> Vec<super::super::super::ValidationIssue> {
    prepare_many_and_validate("clasify", json!({"queries":[query]}))
        .expect_err("invalid clasify query")
        .issues
}

#[test]
fn a_present_alternative_outranks_a_branch_that_forbids_it() {
    let schema = json!({"anyOf":[
        {"type":"object","required":["inline"],"properties":{
            "inline":{"type":"string"},"resource":{"not":{}}
        }},
        {"type":"object","required":["resource"],"properties":{
            "inline":{"not":{}},
            "resource":{"type":"object","required":["path","line","format"],"properties":{
                "path":{"type":"string"},"line":{"type":"integer"},"format":{"enum":["text"]}
            }}
        }}
    ]});
    let mut value = json!({"resource":{"path":false,"line":"wrong","format":"other"}});
    let error = super::super::validate_schema(&schema, &schema, &mut value, &mut Vec::new())
        .expect_err("the selected resource is malformed");
    assert_eq!(error.issues.len(), 3, "{error:?}");
    assert!(
        error.issues.iter().all(|issue| issue.path[0] == "resource"),
        "{error:?}"
    );
}

/// A branch missing its required `operation` literal is rejected like one
/// pinning it to another literal (core Zod: `invalid_value`), so the
/// default form's own errors lead.
#[test]
fn a_missing_required_selector_rejects_its_branch() {
    let issues = clasify_issues(json!({
        "questions":["q"],
        "source":{"tool":"ghGetFileContent","query":{"path":"a.ts","ranges":["1-2"]}}
    }));
    assert_eq!(issues[0].path, ["queries", "0", "questions"], "{issues:?}");
    assert!(
        !issues
            .iter()
            .any(|issue| issue.path.last().is_some_and(|field| field == "operation")),
        "{issues:?}"
    );
}

/// A wrong discriminator is the first repair, ahead of the fields of the
/// branch it was compared with.
#[test]
fn selector_issues_lead_the_chosen_branch() {
    let issues = clasify_issues(json!({
        "questions":{"check":{"type":"noul","instructions":"The state contains evidence."}},
        "source":{"tool":"astSearch","query":{"path":"a.ts"}}
    }));
    assert_eq!(
        issues[0].path,
        ["queries", "0", "source", "tool"],
        "{issues:?}"
    );
    assert_eq!(issues[0].rule_id, "schema.enum", "{issues:?}");
    for tool in ["localFetch", "ghGetFileContent"] {
        assert!(issues[0].message.contains(tool), "{tool}: {issues:?}");
    }
}

#[test]
fn forbidden_field_names_the_mutually_exclusive_fields() {
    let error = prepare_many_and_validate(
        "lspSearch",
        json!({"queries":[{
            "mainGoal":"g","reasoning":"r","operation":"documentSymbols",
            "path":"/tmp/a.rs","symbolName":"finish","line":3
        }]}),
    )
    .expect_err("a document query forbids symbolName/line");
    let message = error
        .issues
        .iter()
        .map(|issue| issue.message.as_str())
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(!message.contains("forbidden schema"), "{message}");
    assert!(message.contains("mutually exclusive"), "{message}");
    assert!(message.contains("`symbolName`"), "{message}");
    assert!(message.contains("`line`"), "{message}");
}
