use super::*;
use octocode_test_support::json_fixture;

#[test]
fn a_commit_shows_the_date_its_range_filter_uses() {
    let rebased = json_fixture(
        r#"{
            "commit": {
                "author": {"date": "2023-01-01T00:00:00Z"},
                "committer": {"date": "2024-06-01T00:00:00Z"}
            }
        }"#,
    );
    assert_eq!(commit_date(&rebased), "2024-06-01T00:00:00Z");
    let bare = json_fixture(r#"{"commit": {"author": {"date": "2023-01-01T00:00:00Z"}}}"#);
    assert_eq!(commit_date(&bare), "2023-01-01T00:00:00Z");
}

/// Every history list shows one commit row: headline, author, the
/// committer date in UTC; a missing person is absent, not invented.
#[test]
fn every_history_list_shows_one_commit_row() {
    let commit = json_fixture(
        r#"{
            "sha": "c1",
            "author": {"login": "octo"},
            "commit": {
                "message": "fix: parser\n\nLong body.\nMore.\n",
                "author": {"name": "Ada", "date": "2024-01-01T00:00:00Z"},
                "committer": {"date": "2024-02-01T02:00:00+02:00"}
            }
        }"#,
    );
    assert_eq!(
        row(&commit),
        json_fixture(
            r#"{
                "commitSha": "c1",
                "messageHeadline": "fix: parser",
                "author": "octo",
                "committedAt": "2024-02-01T00:00:00Z"
            }"#
        )
    );
    assert_eq!(message_body(&commit), Some("Long body.\nMore."));
    let bare = json_fixture(r#"{"sha": "c2", "commit": {"message": "one line"}}"#);
    assert_eq!(
        row(&bare),
        json_fixture(
            r#"{
                "commitSha": "c2",
                "messageHeadline": "one line",
                "committedAt": ""
            }"#
        )
    );
    assert_eq!(message_body(&bare), None);
}

#[test]
fn squash_headline_names_its_pull_request() {
    assert_eq!(
        headline_pull_request("io: use `spawn_mandatory_blocking` (#8506)"),
        Some(8506)
    );
    assert_eq!(headline_pull_request("Merge branch main"), None);
    assert_eq!(headline_pull_request("fix (#abc)"), None);
    assert_eq!(headline_pull_request("revert (#12) partially"), None);
}

#[test]
fn a_commit_message_names_its_pull_request_by_headline_merge_or_trailer() {
    let named = |message: &str| commit_pull_request(message, "nodejs", "node");
    assert_eq!(
        named("fix (#12)\n\nPR-URL: https://github.com/nodejs/node/pull/99"),
        Some(12)
    );
    assert_eq!(named("Merge pull request #34 from a/b\n\nbody"), Some(34));
    assert_eq!(
        named("url: fix\n\nPR-URL: https://github.com/nodejs/node/pull/65801\nReviewed-By: x"),
        Some(65801)
    );
    assert_eq!(
        named("url: fix\n\npr-url: https://github.com/NodeJS/Node/pull/7/"),
        Some(7)
    );
    // Another repository's pull request is not this one's.
    assert_eq!(
        named("x\n\nPR-URL: https://github.com/other/node/pull/5"),
        None
    );
    assert_eq!(
        named("x\n\nPR-URL: https://github.com/nodejs/node/issues/5"),
        None
    );
    assert_eq!(named("x\n\nRefs: #5"), None);
}

#[test]
fn a_person_is_the_login_else_the_git_name() {
    let linked = json_fixture(
        r#"{
            "author": {"login": "octo"},
            "commit": {"author": {"name": "Ada"}}
        }"#,
    );
    assert_eq!(person(&linked, "author"), "octo");
    let unlinked = json_fixture(r#"{"author": null, "commit": {"author": {"name": "Ada"}}}"#);
    assert_eq!(person(&unlinked, "author"), "Ada");
    let empty_login = json_fixture(
        r#"{
            "author": {"login": ""},
            "commit": {"author": {"name": "Ada"}}
        }"#,
    );
    assert_eq!(person(&empty_login, "author"), "Ada");
    assert_eq!(person(&json!({}), "committer"), Value::Null);
}
