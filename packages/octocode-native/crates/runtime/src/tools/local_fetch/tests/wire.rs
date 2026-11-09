use super::*;
use octocode_contract_types::LocalFetchQuery;

#[test]
fn canonical_match_wire_keeps_source_coordinates() {
    let fixture = Fixture::new();
    let path = fixture.write("a.txt", "zero\none\nneedle\nthree\nfour\n");
    let req = qj(
        &path,
        serde_json::json!({"matchString":["needle"],"contextLines":{"before":1,"after":1}}),
    );
    let result = fetch(&req, &Paths(fixture.0.clone()));
    let wire = serde_json::to_value(&result).expect("result");
    assert_eq!(wire["content"], "one\nneedle\nthree\n");
    assert_eq!(
        wire["sourceLineRanges"],
        serde_json::json!([{"line":2,"endLine":4}])
    );
    assert_eq!(wire["matchedLines"], serde_json::json!([3]));
    assert_eq!(wire["selectedMatchCount"], 1);
    assert!(wire.get("sourceBytes").is_none() && wire.get("returnedBytes").is_none());
    assert_eq!(wire["totalLines"], 5);
    crate::tools::test_support::published(crate::id::ToolId::LocalFetch, &wire);
}

/// C3eK A-R9 / B-R6: a read whose matchString selects nothing states its
/// zero once: `content:""` beside the row's `empty` status (and, on GitHub,
/// `matchNotFound`), not again as `matchedLines:[]`, `selectedMatchCount:0`
/// and `returnedLines:0`. A page with text keeps them.
#[test]
fn an_empty_match_read_states_its_zero_once() {
    let fixture = Fixture::new();
    let path = fixture.write("a.txt", "zero\none\nneedle\n");
    let req = qj(&path, serde_json::json!({"matchString":["zzqq_absent"]}));
    let result = fetch(&req, &Paths(fixture.0.clone()));
    let wire = serde_json::to_value(&result).expect("result");
    assert_eq!(wire["content"], "", "{wire}");
    assert_eq!(wire["totalLines"], 3, "{wire}");
    for restated in ["matchedLines", "selectedMatchCount", "returnedLines"] {
        assert!(wire.get(restated).is_none(), "{restated}: {wire}");
    }
    assert_eq!(format!("{:?}", result.status), "Empty", "the row's status states it");
    crate::tools::test_support::published(crate::id::ToolId::LocalFetch, &wire);
}

#[test]
fn canonical_reader_rejects_aliases_and_incompatible_views() {
    for fields in [
        serde_json::json!({"fullContent":true}),
        serde_json::json!({"minify":"symbols"}),
        serde_json::json!({"offset":0}),
        serde_json::json!({"unit":"lines"}),
        serde_json::json!({"length":3}),
        serde_json::json!({"matchString":"needle"}),
        serde_json::json!({"matchString":["needle"],"regex":"rust"}),
        serde_json::json!({"matchString":["needle"],"contextLines":1}),
        serde_json::json!({"ranges":["1-3"]}),
        serde_json::json!({"view":"symbols","matchString":["needle"]}),
        serde_json::json!({"view":"compact","classification":{"predicates":["relevant"]}}),
    ] {
        let mut row = serde_json::json!({"path":"file.txt"});
        row.as_object_mut()
            .expect("row")
            .extend(fields.as_object().expect("fields").clone());
        assert!(
            serde_json::from_value::<LocalFetchQuery>(row.clone()).is_err(),
            "{row}"
        );
        assert!(
            crate::contracts::validate_query("localFetch", row.clone()).is_err(),
            "{row}"
        );
    }
}
