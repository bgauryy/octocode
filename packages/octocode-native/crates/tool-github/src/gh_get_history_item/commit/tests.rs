use super::*;
use octocode_test_support::json_fixture;

/// Every commit read states its keys in one order, whatever it shows:
/// identity, message, people, parents, then the counts (as a pull
/// request states them).
#[test]
fn a_commit_header_keeps_one_key_order() {
    let raw = json!({"sha":"a".repeat(40),"parents":[{"sha":"b".repeat(40)}],
            "stats":{"additions":0,"deletions":3},
            "commit":{"message":"fix: x\n\nbody",
                "author":{"name":"Ada","email":"ada@x.dev","date":"2024-01-01T00:00:00Z"},
                "committer":{"name":"Ada","email":"ada@x.dev","date":"2024-01-01T00:00:00Z"}},
            "author":{"login":"ada"},"committer":{"login":"ada"}});
    let facts = CommitFacts::of(&raw);
    let keys = |header: Value| {
        let mut header = header;
        remove_nulls(&mut header);
        header
            .as_object()
            .expect("row")
            .keys()
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(
        keys(commit_header(&facts, "main", true, 2)),
        [
            "ref",
            "commitSha",
            "message",
            "author",
            "parentCommitShas",
            "changedFileCount",
            "additions",
            "deletions"
        ]
    );
    assert_eq!(
        keys(commit_header(&facts, "main", false, 2)),
        [
            "ref",
            "commitSha",
            "message",
            "author",
            "parentCommitShas",
            "changedFileCount"
        ]
    );
}

/// Patch-only reads need every upstream parent identity to form exact
/// source reads. Missing upstream parents stay unknown, not an invented
/// first parent; a root commit's explicit empty array is known empty.
#[test]
fn every_read_preserves_upstream_parent_provenance() {
    for summary in [false, true] {
        for parents in [
            json!([]),
            json_fixture(r#"[{"sha": "first"}, {"sha": "second"}, {"sha": "third"}]"#),
        ] {
            let raw = json!({"sha":"head", "parents":parents});
            let facts = CommitFacts::of(&raw);
            let out = commit_header(&facts, "head", summary, 0);
            let expected = parents
                .as_array()
                .expect("parents")
                .iter()
                .map(|parent| parent["sha"].clone())
                .collect::<Vec<_>>();
            assert_eq!(out["parentCommitShas"], json!(expected));
            assert_eq!(facts.parent.as_deref(), parents[0]["sha"].as_str());
        }
        let raw = json_fixture(r#"{"sha": "head"}"#);
        let facts = CommitFacts::of(&raw);
        let mut out = commit_header(&facts, "head", summary, 0);
        remove_nulls(&mut out);
        assert!(out.get("parentCommitShas").is_none());
        assert!(facts.parent.is_none());
    }
}

/// A committer that repeats the author whole is not restated; one that
/// differs in any field (a rebased date, another email) is evidence.
#[test]
fn a_committer_is_shown_only_when_it_differs_from_the_author() {
    let raw = |committer: Value| {
        json!({"commit":{
                "author":{"name":"Ada","email":"ada@x.dev","date":"2024-01-01T00:00:00Z"},
                "committer":committer}})
    };
    let same = raw(json_fixture(
        r#"{
                "name": "Ada",
                "email": "ada@x.dev",
                "date": "2024-01-01T00:00:00Z"
            }"#,
    ));
    assert_eq!(committer(&same, &identity(&same, "author")), None);
    let other_email = raw(json_fixture(
        r#"{
                "name": "Ada",
                "email": "ada@y.dev",
                "date": "2024-01-01T00:00:00Z"
            }"#,
    ));
    let kept = committer(&other_email, &identity(&other_email, "author")).expect("kept");
    assert_eq!(kept["email"], "ada@y.dev");
    let rebased = raw(json_fixture(
        r#"{
                "name": "Ada",
                "email": "ada@x.dev",
                "date": "2024-02-01T00:00:00Z"
            }"#,
    ));
    let kept = committer(&rebased, &identity(&rebased, "author")).expect("kept");
    assert_eq!(kept["date"], "2024-02-01T00:00:00Z");
}
