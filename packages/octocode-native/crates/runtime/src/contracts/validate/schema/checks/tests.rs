use serde_json::json;

#[test]
fn conditions_preserve_every_record_key_and_leave_values_unchanged() {
    let schema = json!({"x-checks":[{
        "id":"probe.entries", "each":"entries", "message":"Resource required.",
        "when":{"type":"object","properties":{"needs":{"const":true}},"required":["needs"]},
        "assert":{"type":"object","required":["resource"]}
    }]});
    let mut value =
        json!({"entries":{"first":{"needs":true},"ignored":{"needs":false},"last":{"needs":true}}});
    let before = value.clone();
    let error =
        super::validate(&schema, &mut value, &["row".into()]).expect_err("missing resource");
    assert_eq!(value, before);
    let paths: Vec<_> = error
        .issues
        .iter()
        .map(|issue| issue.path.clone())
        .collect();
    assert_eq!(
        paths,
        [
            vec!["row", "entries", "first"],
            vec!["row", "entries", "last"]
        ]
    );
    assert!(
        error
            .issues
            .iter()
            .all(|issue| issue.message == "Resource required." && issue.rule_id == "probe.entries")
    );
    value["resource"] = json!(null);
    super::validate(&schema, &mut value, &[]).expect("present null is still present");
}

#[test]
fn predicate_references_resolve_in_their_own_schema_document() {
    let schema = json!({"x-checks":[{
        "id":"probe.ref", "message":"Dependency required.", "path":["dependency"],
        "when":{"$ref":"#/$defs/Condition","$defs":{"Condition":{"required":["trigger"]}}},
        "assert":{"required":["dependency"]}
    }]});
    let error =
        super::validate(&schema, &mut json!({"trigger":false}), &[]).expect_err("dependency");
    assert_eq!(error.issues[0].path, ["dependency"]);
    super::validate(&schema, &mut json!({}), &[]).expect("condition absent");
}

#[test]
fn migrated_rules_keep_batch_indexes_and_authored_diagnostics() {
    let error = crate::contracts::validate(
        "structureSearch",
        json!({"queries":[
            {"operation":"files","path":".","minDepth":2,"maxDepth":2},
            {"operation":"files","path":".","minDepth":3,"maxDepth":1},
            {"operation":"files","path":".","minDepth":3}
        ]}),
    )
    .expect_err("reversed bounds");
    assert_eq!(error.issues.len(), 1);
    assert_eq!(error.issues[0].path, ["queries", "1", "minDepth"]);
    assert_eq!(
        error.issues[0].message,
        "minDepth must be less than or equal to maxDepth."
    );
    let error = crate::contracts::validate("clasify", json!({"queries":[
        {"operation":"models"},
        {"operation":"evaluate","content":"photo","questions":{"first":{"type":"box"},"last":{"type":"box"}}}
    ]})).expect_err("images required");
    assert_eq!(error.issues.len(), 2);
    assert_eq!(error.issues[0].path, ["queries", "1", "questions", "first"]);
    assert_eq!(error.issues[1].path, ["queries", "1", "questions", "last"]);
}

#[test]
fn invalid_predicates_are_contract_errors_not_negative_matches() {
    for broken in [
        json!({"$ref":"#/$defs/Missing"}),
        json!({"type":"invented"}),
        json!({"type":"string","pattern":"["}),
    ] {
        let schema = json!({"x-checks":[{"id":"probe", "message":"Required", "when":broken, "assert":false}]});
        let error =
            super::validate(&schema, &mut json!("text"), &[]).expect_err("broken predicate");
        assert!(error.is_contract_error(), "{error:?}");
    }
}

#[test]
fn embedded_schema_audit_checks_inactive_branches_and_annotations() {
    for broken in [
        json!({"$ref":"#/$defs/Missing"}),
        json!({"type":"invented"}),
        json!({"type":"string","pattern":"["}),
        json!({"x-order":{"lower":"a","upper":"b","kind":"invented"}}),
    ] {
        for schema in [
            json!({"anyOf":[{},broken]}),
            json!({"not":broken}),
            json!({"contains":broken}),
            json!({"x-checks":[{"id":"probe","message":"Required","when":broken,"assert":{}}]}),
        ] {
            assert!(
                super::super::audit::validate(&schema)
                    .expect_err("invalid contract")
                    .is_contract_error()
            );
        }
    }
}

#[test]
fn probes_propagate_broken_schemas_through_compositions() {
    for schema in [
        json!({"not":{"$ref":"#/$defs/Missing"}}),
        json!({"anyOf":[{"$ref":"#/$defs/Missing"},{}]}),
        json!({"allOf":[{"$ref":"#/$defs/Missing"},{"const":"different"}]}),
        json!({"contains":{"$ref":"#/$defs/Missing"}}),
    ] {
        let error =
            super::super::validate_read_only(&schema, &schema, &mut json!([1]), &mut vec![])
                .expect_err("invalid contract");
        assert!(error.is_contract_error(), "{error:?}");
    }
}

#[test]
fn arithmetic_order_and_logical_tree_depth_are_generic() {
    let sum = json!({"x-sum":{"id":"probe.sum","message":"Bad total","terms":["a","b","optional"],"total":"total"}});
    super::super::relationships::validate(&sum, &json!({"a":1,"b":2,"total":3}), &[]).unwrap();
    assert!(
        super::super::relationships::validate(&sum, &json!({"a":1,"b":2,"total":4}), &[]).is_err()
    );
    let order = json!({"x-order":{"id":"probe.order","message":"Bad order","lower":"a","upper":"b","kind":"lexicographic","keys":["major","minor"],"exclusive":true}});
    super::super::relationships::validate(
        &order,
        &json!({"a":{"major":1,"minor":9},"b":{"major":2,"minor":0}}),
        &[],
    )
    .unwrap();
    assert!(
        super::super::relationships::validate(
            &order,
            &json!({"a":{"major":1,"minor":9},"b":{"major":1,"minor":9}}),
            &[]
        )
        .is_err()
    );
    let tree = json!({"x-tree":{"id":"probe.depth","message":"Too deep","children":[["children","*"]],"maxDepth":1}});
    super::super::relationships::validate(&tree, &json!({"children":[{}]}), &[]).unwrap();
    let error =
        super::super::relationships::validate(&tree, &json!({"children":[{"children":[{}]}]}), &[])
            .expect_err("depth");
    assert_eq!(error.issues[0].path, ["children", "0", "children", "0"]);
}
