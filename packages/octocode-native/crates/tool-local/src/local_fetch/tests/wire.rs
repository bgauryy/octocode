use super::*;
use octocode_contract_types::LocalFetchQuery;
use octocode_test_support::json_fixture;

#[test]
fn canonical_match_wire_keeps_source_coordinates() {
    let fixture = Fixture::new();
    let path = fixture.write("a.txt", "zero\none\nneedle\nthree\nfour\n");
    let req = qj(
        &path,
        json_fixture(
            r#"{
                "matchString": ["needle"],
                "contextLines": {"before": 1, "after": 1}
            }"#,
        ),
    );
    let result = fetch(&req, &Paths(fixture.0.clone()));
    let wire = serde_json::to_value(&result).expect("result");
    assert_eq!(wire["content"], "one\nneedle\nthree\n");
    assert_eq!(
        wire["sourceLineRanges"],
        json_fixture(r#"[{"line": 2, "endLine": 4}]"#)
    );
    assert_eq!(wire["matchedLines"], json_fixture(r#"[3]"#));
    assert_eq!(wire["selectedMatchCount"], 1);
    assert!(wire.get("sourceBytes").is_none() && wire.get("returnedBytes").is_none());
    assert_eq!(wire["totalLines"], 5);
    octocode_runtime_base::test_support::published(
        octocode_runtime_base::id::ToolId::LocalFetch,
        &wire,
    );
}

/// C3eK A-R9 / B-R6: a read whose matchString selects nothing states its
/// zero once: `content:""` beside the row's `empty` status (and, on GitHub,
/// `matchNotFound`), not again as `matchedLines:[]`, `selectedMatchCount:0`
/// and `returnedLines:0`. A page with text keeps them.
#[test]
fn an_empty_match_read_states_its_zero_once() {
    let fixture = Fixture::new();
    let path = fixture.write("a.txt", "zero\none\nneedle\n");
    let req = qj(&path, json_fixture(r#"{"matchString": ["zzqq_absent"]}"#));
    let result = fetch(&req, &Paths(fixture.0.clone()));
    let wire = serde_json::to_value(&result).expect("result");
    assert_eq!(wire["content"], "", "{wire}");
    assert_eq!(wire["totalLines"], 3, "{wire}");
    for restated in ["matchedLines", "selectedMatchCount", "returnedLines"] {
        assert!(wire.get(restated).is_none(), "{restated}: {wire}");
    }
    assert_eq!(
        format!("{:?}", result.status),
        "Empty",
        "the row's status states it"
    );
    octocode_runtime_base::test_support::published(
        octocode_runtime_base::id::ToolId::LocalFetch,
        &wire,
    );
}

#[test]
fn canonical_reader_rejects_aliases_and_incompatible_views() {
    for fields in [
        json_fixture(r#"{"fullContent": true}"#),
        json_fixture(r#"{"minify": "symbols"}"#),
        json_fixture(r#"{"offset": 0}"#),
        json_fixture(r#"{"unit": "lines"}"#),
        json_fixture(r#"{"length": 3}"#),
        json_fixture(r#"{"matchString": "needle"}"#),
        json_fixture(r#"{"matchString": ["needle"], "regex": "rust"}"#),
        json_fixture(r#"{"matchString": ["needle"], "contextLines": 1}"#),
        json_fixture(r#"{"ranges": ["1-3"]}"#),
        json_fixture(r#"{"view": "symbols", "matchString": ["needle"]}"#),
        json_fixture(
            r#"{
                "view": "compact",
                "classification": {"predicates": ["relevant"]}
            }"#,
        ),
    ] {
        let mut row = json_fixture(r#"{"path": "file.txt"}"#);
        row.as_object_mut()
            .expect("row")
            .extend(fields.as_object().expect("fields").clone());
        assert!(
            serde_json::from_value::<LocalFetchQuery>(row.clone()).is_err(),
            "{row}"
        );
        assert!(
            octocode_runtime_base::contracts::validate_query("localFetch", row.clone()).is_err(),
            "{row}"
        );
    }
}
