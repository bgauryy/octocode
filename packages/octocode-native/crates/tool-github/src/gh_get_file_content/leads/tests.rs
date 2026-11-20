use super::*;
use octocode_test_support::json_fixture;
use octocode_tools_shared::source_view::{FetchCall, LocalFetchResult, NextCalls, ReadStatus};

fn read(fields: Value) -> FetchCall {
    let mut row = json_fixture(r#"{"path": "README.md"}"#);
    for (key, value) in fields.as_object().into_iter().flatten() {
        row[key] = value.clone();
    }
    FetchCall::continued(serde_json::from_value(row).expect("read"))
}

#[test]
fn empty_match_search_uses_the_index_while_history_keeps_the_ref() {
    for reference in ["main", "0123456789abcdef0123456789abcdef01234567"] {
        let source: GhGetFileContentQuery = serde_json::from_value(json!({
            "owner":"o", "repo":"r", "path":"README.md", "ref":reference,
            "matchString":["missing literal"]
        }))
        .expect("query");
        let local: LocalFetchQuery = serde_json::from_value(json_fixture(
            r#"{"path": "README.md", "matchString": ["missing literal"]}"#,
        ))
        .expect("local query");
        let leads = empty_match_leads(&source, &local, true);
        let code = &leads
            .iter()
            .find(|(name, _)| *name == "findCode")
            .expect("code")
            .1;
        assert_eq!(code["kind"], "related");
        let code_query = &code["input"]["queries"][0];
        assert_eq!(
            code_query["keywords"],
            json_fixture(r#"["missing literal"]"#)
        );
        assert!(code_query.get("ref").is_none(), "{code}");
        let history = &leads
            .iter()
            .find(|(name, _)| *name == "listCommits")
            .expect("history")
            .1;
        assert_eq!(history["input"]["queries"][0]["ref"], reference);
    }
}

/// The whole-source recovery of a whole read keeps that scope
/// on GitHub; range and saved-progress reads retain their bounds, and
/// the unknown-units replay stays a GitHub read.
#[test]
fn a_whole_view_lead_keeps_full_content_and_every_row_is_rewritten() {
    let source: GhGetFileContentQuery = serde_json::from_value(json_fixture(
        r#"{"owner": "o", "repo": "r", "path": "README.md"}"#,
    ))
    .expect("query");
    let mut result = LocalFetchResult::blank(ReadStatus::Success);
    result.next = Some(NextCalls {
        unfiltered: Some(read(json!({}))),
        unfiltered_unknown: Some(read(json_fixture(
            r#"{"ranges": [{"startLine": 3, "endLine": 4}]}"#,
        ))),
        continue_window: Some(read(json_fixture(
            r#"{"readProgress": {"offset": 50, "unit": "lines"}}"#,
        ))),
        ..NextCalls::default()
    });
    let next = rewrite_continuations(&mut result, &source, "abc123").expect("next");
    let whole = &next["unfiltered"];
    assert_eq!(whole["tool"], "ghGetFileContent");
    assert_eq!(
        octocode_tools_shared::test_support::read_view(
            ToolId::GhGetFileContent,
            &whole["input"]["queries"][0]
        ),
        octocode_contract_types::ReadView::Content
    );
    let window = &next["nextPage"]["input"]["queries"][0];
    assert_eq!(window["readProgress"]["offset"], 50);
    let unknown = &next["unfilteredUnknown"]["input"]["queries"][0];
    assert!(unknown["owner"] == "o" && unknown["ref"] == "abc123");
}
