use super::*;
use octocode_test_support::json_fixture;

#[test]
fn a_listing_entry_names_its_path_under_its_group() {
    let data = json_fixture(r#"{"files": [{"dir": "src", "files": ["a.rs (10)", "sub/"]}]}"#);
    let rows = Rows::of(ToolId::StructureSearch, &data);
    assert_eq!(
        rows.states(),
        vec![
            Some(json_fixture(r#"{"path": "src/a.rs"}"#)),
            Some(json_fixture(r#"{"path": "src/sub/"}"#))
        ]
    );
}

#[test]
fn retaining_keeps_shown_units_and_drops_emptied_groups() {
    let mut data = json_fixture(
        r#"{
            "files": [
                {"dir": "a", "files": ["x (1)", "y (2)"]},
                {"dir": "b", "files": ["z (3)"]}
            ]
        }"#,
    );
    let rows = Rows::of(ToolId::StructureSearch, &data);
    rows.retain(&mut data, &[1]);
    assert_eq!(
        data,
        json_fixture(r#"{"files": [{"dir": "a", "files": ["y (2)"]}]}"#)
    );
}

#[test]
fn history_rows_are_judged_by_title_and_body_and_empty_ones_stay_unknown() {
    let data = json_fixture(
        r#"{
            "commits": [{"messageHeadline": "fix: crash"}, {"commitSha": "abc"}]
        }"#,
    );
    let rows = Rows::of(ToolId::GhSearchHistory, &data);
    assert_eq!(
        rows.states(),
        vec![
            Some(json_fixture(r#"{"title": "fix: crash", "body": null}"#)),
            None
        ]
    );
}

#[test]
fn a_commit_message_body_and_a_pull_request_body_are_both_judged() {
    let data = json_fixture(
        r#"{
            "commits": [
                {"messageHeadline": "tidy", "messageBody": "fixes a crash"}
            ],
            "pullRequests": [{"title": "tidy", "body": "renames a field"}]
        }"#,
    );
    let rows = Rows::of(ToolId::GhSearchHistory, &data);
    assert_eq!(
        rows.states(),
        vec![
            Some(json_fixture(
                r#"{"title": "tidy", "body": "renames a field"}"#
            )),
            Some(json_fixture(
                r#"{"title": "tidy", "body": "fixes a crash"}"#
            )),
        ]
    );
}

#[test]
fn a_review_is_judged_on_its_verdict_when_it_has_no_body() {
    let data = json_fixture(
        r#"{
            "pullRequests": [
                {
                    "reviews": [
                        {"state": "APPROVED", "author": "a"},
                        {
                            "state": "CHANGES_REQUESTED",
                            "body": "fix the leak",
                            "author": "b"
                        },
                        {"author": "c"}
                    ],
                    "comments": [{"author": "d"}]
                }
            ]
        }"#,
    );
    let states = Rows::of(ToolId::GhGetHistoryItem, &data).states();
    assert_eq!(states.len(), 4);
    // Page order: comments, reviews, review comments.
    assert_eq!(states[0], None, "a comment without a body has nothing");
    assert_eq!(
        states[1].as_ref().map(|s| &s["state"]),
        Some(&json!("APPROVED"))
    );
    assert_eq!(
        states[2].as_ref().map(|s| &s["body"]),
        Some(&json!("fix the leak"))
    );
    assert_eq!(states[3], None, "a review with neither stays unknown");
}

#[test]
fn discussion_units_cut_each_list_of_each_row() {
    let mut data = json_fixture(
        r#"{
            "pullRequests": [
                {
                    "comments": [{"body": "a"}, {"body": "b"}],
                    "reviews": [{"body": "c"}]
                }
            ]
        }"#,
    );
    let rows = Rows::of(ToolId::GhGetHistoryItem, &data);
    assert_eq!(rows.states().len(), 3);
    rows.retain(&mut data, &[1, 2]);
    assert_eq!(
        data,
        json_fixture(
            r#"{
                "pullRequests": [{"comments": [{"body": "b"}], "reviews": [{"body": "c"}]}]
            }"#
        )
    );
}
