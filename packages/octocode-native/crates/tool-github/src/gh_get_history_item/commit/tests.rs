use super::*;
use octocode_test_support::json_fixture;

/// The first page of every commit read states its keys in one order,
/// whatever it shows: identity, message, people, parents, then the counts
/// (as a pull request states them). The commit's own line counts are exact
/// facts of the commit: a files or patches read states them on its first
/// page as the summary does.
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
        keys(commit_header(&facts, "main", Header::Summary, 2)),
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
        keys(commit_header(&facts, "main", Header::Section, 2)),
        keys(commit_header(&facts, "main", Header::Summary, 2)),
    );
    // Read by its own SHA, the ref is not restated and the order holds: the
    // commit's identity leads, its counts close.
    assert_eq!(
        keys(commit_header(&facts, &"a".repeat(40), Header::Summary, 2)),
        [
            "commitSha",
            "message",
            "author",
            "parentCommitShas",
            "changedFileCount",
            "additions",
            "deletions"
        ]
    );
}

/// A walk states the commit once. A later file page or patch window names
/// the commit and counts its files, and repeats nothing the first page
/// stated: no message, people, parents or line counts. Every key of a
/// continuation header is one the first page holds too, so the pages join.
#[test]
fn a_continuation_page_names_the_commit_and_states_none_of_it_again() {
    let raw = json!({"sha":"a".repeat(40),"parents":[{"sha":"b".repeat(40)}],
            "stats":{"additions":4,"deletions":3},
            "commit":{"message":"fix: x\n\n".to_owned() + &"body ".repeat(400),
                "author":{"name":"Ada","email":"ada@x.dev","date":"2024-01-01T00:00:00Z"},
                "committer":{"name":"Bob","email":"bob@x.dev","date":"2024-02-02T00:00:00Z"}},
            "author":{"login":"ada"},"committer":{"login":"bob"}});
    let facts = CommitFacts::of(&raw);
    let first = commit_header(&facts, "main", Header::Section, 900);
    let later = commit_header(&facts, "main", Header::Continuation, 900);
    let later_keys = later.as_object().expect("row").keys().collect::<Vec<_>>();
    assert_eq!(
        later_keys,
        ["ref", "commitSha", "changedFileCount"],
        "{later}"
    );
    for key in &later_keys {
        assert_eq!(later[key.as_str()], first[key.as_str()], "{key}");
    }
    // The header a later page carries does not grow with the message.
    assert!(later.to_string().len() < 200, "{later}");
    assert!(first.to_string().len() > 2000, "{first}");
    // A SHA the caller sent is not restated as its ref, on any page.
    let sha = "a".repeat(40);
    let by_sha = commit_header(&facts, &sha, Header::Continuation, 900);
    assert!(by_sha.get("ref").is_none(), "{by_sha}");
}

/// The read that continues a commit's file list keeps the sections the
/// read named, so a `files` walk stays a `files` read; a read that named
/// none (the summary) opens its file list as a `files` read, never as the
/// summary again.
#[test]
fn a_file_page_continuation_keeps_the_reads_sections() {
    let sha = "a".repeat(40);
    for sections in [Some(json!(["files"])), None] {
        let mut row = json!({"operation":"commit","owner":"o","repo":"r","ref":"main"});
        if let Some(sections) = &sections {
            row["sections"] = sections.clone();
        }
        let request = HistoryItemRequest::from_row(row).expect("commit row");
        let mut continued = super::super::patch_hop::commit_query(&request, &sha);
        assert_eq!(continued.get("sections"), sections.as_ref(), "{continued}");
        assert_eq!(continued["ref"], sha.as_str(), "{continued}");
        super::super::patch_hop::files_only(&mut continued, ItemOperation::Commit);
        assert_eq!(continued["sections"], json!(["files"]), "{continued}");
    }
}

/// A differing committer is stated on every read of the commit, a patches
/// or files read included, not only on the summary.
#[test]
fn a_differing_committer_is_stated_on_every_read() {
    let raw = json!({"sha":"a".repeat(40),
        "commit":{"message":"m",
            "author":{"name":"Ada","email":"ada@x.dev","date":"2024-01-01T00:00:00Z"},
            "committer":{"name":"Bob","email":"bob@x.dev","date":"2024-02-02T00:00:00Z"}},
        "author":{"login":"ada"},"committer":{"login":"bob"}});
    let facts = CommitFacts::of(&raw);
    for summary in [false, true] {
        let read = if summary {
            Header::Summary
        } else {
            Header::Section
        };
        let header = commit_header(&facts, "main", read, 1);
        assert_eq!(header["committer"]["login"], "bob", "summary {summary}");
        assert!(
            header["committer"]["committedAt"].is_string(),
            "summary {summary}"
        );
    }
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
            let read = if summary {
                Header::Summary
            } else {
                Header::Section
            };
            let out = commit_header(&facts, "head", read, 0);
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
        let read = if summary {
            Header::Summary
        } else {
            Header::Section
        };
        let mut out = commit_header(&facts, "head", read, 0);
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

/// Only GitHub's same-repository autolink form counts as a mention: `#N`
/// alone, never inside a word, an HTML entity or `owner/repo#N`, each
/// number once.
#[test]
fn a_message_mentions_only_same_repository_references() {
    let mentioned = super::mentioned_numbers(
        "Fix (#12), see #2483 and #2483; not a#3, &#38;, o/r#4, #5x, #0 or #.",
    );
    assert_eq!(mentioned, [12, 2483]);
    assert!(super::mentioned_numbers("printer: add hyperlinks").is_empty());
}
