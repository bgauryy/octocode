use super::*;
use octocode_contract_types::ErrorCode;

#[test]
fn punctuation_is_literal_until_the_caller_selects_a_regex_engine() {
    let files = [("a.txt", "a.c\nabc\nx|y\nx\ny\n")];
    for (engine, expected) in [("literal", 2), ("rust", 5)] {
        let result = search_fixture(
            &files,
            ls_query(
                serde_json::json!({
                    "matchString":["a.c","x|y"], "regexEngine":engine
                }),
                None,
            ),
        );
        assert_eq!(result["matchedLineCount"], expected, "{result}");
    }
    let empty = search_fixture(
        &files,
        ls_query(serde_json::json!({"matchString":["[absent]"]}), None),
    );
    assert_eq!(empty["matchedLineCount"], 0);
    assert!(
        crate::tools::test_support::checks::leads(&empty)
            .iter()
            .all(|call| {
                call.tool != "localSearch" || call.query["queries"][0]["regexEngine"] != "rust"
            }),
        "a miss must not infer a different pattern language: {empty}"
    );
}

#[test]
fn invalid_regex_and_unknown_file_type_fail_without_reinterpreting_the_request() {
    let root = tempfile::tempdir().expect("fixture");
    fs::write(root.path().join("a.rs"), "needle\n").expect("fixture");
    let (policy, security) = policy_for(root.path());
    for (fields, expected) in [
        (
            serde_json::json!({"matchString":["["],"regexEngine":"rust"}),
            ErrorCode::InvalidPattern,
        ),
        (
            serde_json::json!({"matchString":["needle"],"fileType":"unknown_fixture_type"}),
            ErrorCode::InvalidInput,
        ),
    ] {
        let q = ls_query(
            serde_json::json!({"path":root.path()}),
            Some(&ls_query(fields, None)),
        );
        let error = search_in(&q, &policy, &security, None).expect_err("invalid request");
        assert_eq!(error.code, expected, "{error:?}");
    }
}

#[test]
fn file_terms_filter_the_whole_file_and_preserve_an_empty_result() {
    let files = [
        ("a.rs", "needle\nuse tokio;\n"),
        ("b.rs", "needle\nuse tokio;\nuse async_std;\n"),
        ("c.rs", "needle\n"),
    ];
    let found = search_fixture(
        &files,
        ls_query(
            serde_json::json!({
                "matchString":["needle"],"fileHas":["tokio"],"fileLacks":["async_std"]
            }),
            None,
        ),
    );
    assert_eq!(
        found["files"].as_array().expect("files").len(),
        1,
        "{found}"
    );
    assert_eq!(found["files"][0]["path"], "a.rs");
    let empty = search_fixture(
        &files,
        ls_query(
            serde_json::json!({"matchString":["needle"],"fileHas":["absent"]}),
            None,
        ),
    );
    assert!(
        empty["files"].as_array().is_none_or(Vec::is_empty),
        "{empty}"
    );
}

#[test]
fn missing_path_offers_an_executable_listing_of_the_nearest_parent() {
    let root = tempfile::tempdir().expect("fixture");
    fs::create_dir(root.path().join("src")).expect("directory");
    fs::write(root.path().join("src/exists.rs"), "needle\n").expect("file");
    let (policy, security) = policy_for(root.path());
    let q = ls_query(
        serde_json::json!({"path":root.path().join("src/gone/deeper"),"matchString":["needle"]}),
        None,
    );
    let error = search_in(&q, &policy, &security, None).expect_err("missing");
    assert_eq!(error.code, ErrorCode::PathNotFound);
    let calls = error.calls.expect("recovery");
    let listing = calls
        .as_object()
        .expect("calls")
        .values()
        .find(|call| call["tool"] == "structureSearch")
        .expect("parent listing");
    let result = list_structure(listing["input"]["queries"][0].clone(), &policy, &security);
    assert!(result.contains("exists.rs"), "{result}");
}
