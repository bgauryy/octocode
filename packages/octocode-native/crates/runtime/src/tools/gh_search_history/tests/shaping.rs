//! Row shaping of one page; the reads and broader searches it offers are
//! its [`follow_ups`].
use super::*;

mod follow_ups;

#[test]
fn pull_request_rows_normalize_merged_state_from_list_and_search_shapes() {
    let listed = map_pr(json!({
        "number":1,
        "title":"listed",
        "state":"closed",
        "merged_at":"2026-09-20T10:00:00Z",
        "user":{"login":"dev"},
        "labels":[],
        "created_at":"2026-09-19T10:00:00Z",
        "comments":2
    }));
    assert_eq!(listed["state"], "merged");
    assert_eq!(listed["mergedAt"], "2026-09-20T10:00:00Z");
    assert_eq!(listed["author"], "dev");
    assert_eq!(listed["commentsCount"], 2);

    let searched = map_pr(json!({
        "number":2,
        "title":"searched",
        "state":"closed",
        "pull_request":{"merged_at":"2026-09-20T11:00:00Z"},
        "user":{"login":"dev"},
        "labels":[],
        "created_at":"2026-09-19T11:00:00Z",
        "comments":0
    }));
    assert_eq!(searched["state"], "merged");
    assert_eq!(searched["mergedAt"], "2026-09-20T11:00:00Z");

    let closed = map_pr(json!({
        "number":3,
        "title":"closed",
        "state":"closed",
        "merged_at":null,
        "user":{"login":"dev"},
        "labels":[],
        "created_at":"2026-09-19T12:00:00Z",
        "comments":0
    }));
    assert_eq!(closed["state"], "closed");
    assert!(closed.get("mergedAt").is_none());
}

/// Row diet: commit authors are a login (else name) without email, list
/// rows drop the message body, issue rows drop updatedAt and empty
/// labels, PR rows drop empty labels and zero comment counts.
#[test]
fn history_rows_carry_no_emails_or_empty_fields() {
    let raw = json!({"sha":"abc","commit":{"message":"Fix (#1)\n\nlong body",
        "author":{"name":"Dev","email":"dev@example.com","date":"2026-01-01T00:00:00Z"},
        "committer":{"name":"GitHub","email":"noreply@github.com"}},
        "author":{"login":"dev"},"committer":{"login":"web-flow"}});
    for row in [
        crate::tools::gh_shared::commit::row(&raw),
        map_commit_list(raw.clone()),
    ] {
        assert_eq!(row["author"], "dev", "{row}");
        crate::tools::test_support::checks::assert_absent(&row, "@");
        assert!(row.get("messageBody").is_none(), "{row}");
        assert!(row.get("committer").is_none(), "{row}");
    }
    let nameless =
        json!({"sha":"abc","commit":{"message":"x","author":{"name":"Dev","email":"d@e.f"}}});
    assert_eq!(map_commit_list(nameless)["author"], "Dev");
    let other = json!({"sha":"abc","commit":{"message":"x","author":{"name":"A"},"committer":{"name":"B"}},
        "author":{"login":"a"},"committer":{"login":"b"}});
    assert_eq!(map_commit_list(other)["committer"], "b");
    let issue = map_issue(
        json!({"number":1,"title":"t","state":"open","user":{"login":"u"},
        "labels":[],"created_at":"c","updated_at":"u"}),
        false,
    );
    assert!(
        issue.get("updatedAt").is_none() && issue.get("labels").is_none(),
        "{issue}"
    );
    let by_update = map_issue(
        json!({"number":1,"updated_at":"u","labels":[{"name":"bug"}]}),
        true,
    );
    assert_eq!(by_update["updatedAt"], "u");
    assert_eq!(by_update["labels"], json!(["bug"]));
    let pr = map_pr(
        json!({"number":1,"title":"t","state":"open","user":{"login":"u"},
        "labels":[],"created_at":"c","comments":0}),
    );
    assert!(
        pr.get("labels").is_none() && pr.get("commentsCount").is_none(),
        "{pr}"
    );
}

#[test]
fn empty_history_rows_are_marked_empty() {
    for key in ["pullRequests", "issues", "commits"] {
        let mut value = json!({ key: [] });
        mark_empty(&mut value, false);
        assert_eq!(value["status"], "empty", "{key}");
    }
    let mut more = json!({"commits": []});
    mark_empty(&mut more, true);
    assert!(more.get("status").is_none());
    let mut rows = json!({"issues": [{"number": 1}]});
    mark_empty(&mut rows, false);
    assert!(rows.get("status").is_none());
}

/// Every row date is UTC `Z`: commit search reports the committer's own
/// offset, the REST listings already use `Z`.
#[test]
fn row_dates_are_utc() {
    let commit = json!({"sha":"abc","commit":{"message":"Fix",
        "author":{"name":"Dev","date":"2024-03-01T01:30:00.000+02:00"}}});
    assert_eq!(
        crate::tools::gh_shared::commit::row(&commit)["date"],
        "2024-02-29T23:30:00Z"
    );
    assert_eq!(map_commit_list(commit)["date"], "2024-02-29T23:30:00Z");
    let pr = map_pr(json!({"number":1,"state":"closed","user":{"login":"u"},
        "created_at":"2024-01-01T10:00:00-01:00","merged_at":"2024-01-02T00:00:00+00:30"}));
    assert_eq!(pr["createdAt"], "2024-01-01T11:00:00Z", "{pr}");
    assert_eq!(pr["mergedAt"], "2024-01-01T23:30:00Z", "{pr}");
    let issue = map_issue(
        json!({"number":1,"created_at":"2024-01-01T10:00:00Z","updated_at":"2024-01-01T10:00:00+01:00"}),
        true,
    );
    assert_eq!(issue["createdAt"], "2024-01-01T10:00:00Z", "{issue}");
    assert_eq!(issue["updatedAt"], "2024-01-01T09:00:00Z", "{issue}");
}

/// An empty search offers one runnable batch: the same search once without
/// each narrowing filter (qualifiers, the date window, then keywords;
/// A commit row whose headline ends in `(#N)` names the PR; only the
/// trailing suffix counts (a revert names the reverting PR).
#[test]
fn a_squash_headline_names_its_pull_request_once() {
    let commit =
        |message: &str| json!({"sha":"abc","commit":{"message":message,"author":{"name":"Dev"}}});
    for (message, number) in [
        ("x (#123)", Some(123)),
        ("Revert \"y (#1)\" (#2)\n\nbody", Some(2)),
        ("(#12) prefix", None),
        ("fix (#abc)", None),
    ] {
        for row in [
            crate::tools::gh_shared::commit::row(&commit(message)),
            map_commit_list(commit(message)),
        ] {
            // The headline states the number; the row adds no copy of it.
            assert!(row.get("prNumber").is_none(), "{row}");
            let headline = row["messageHeadline"].as_str().unwrap_or_default();
            assert_eq!(
                crate::tools::gh_shared::commit::headline_pull_request(headline),
                number,
                "{message}: {row}"
            );
        }
    }
}

/// Issue rows carry the discussion size like PR rows; zero stays out.
#[test]
fn issue_rows_carry_a_non_zero_comment_count() {
    let issue = |comments: u64| {
        map_issue(
            json!({"number":1,"title":"t","state":"open","comments":comments}),
            false,
        )
    };
    assert_eq!(issue(105)["commentsCount"], 105);
    assert!(issue(0).get("commentsCount").is_none());
}

/// A row drops what its query already says: the filtered `state`, and a
/// merged row's `createdAt` unless the rows are sorted by creation.
#[test]
fn rows_drop_the_filtered_state_and_a_merged_rows_created_at() {
    let merged = || {
        map_pr(json!({
            "number":1,"title":"t","state":"closed","merged_at":"2026-09-20T10:00:00Z",
            "user":{"login":"dev"},"created_at":"2026-09-19T10:00:00Z"
        }))
    };
    let mut row = merged();
    super::rows::drop_implied(&mut row, Some("merged"), false, &[]);
    assert!(row.get("state").is_none(), "{row}");
    assert!(row.get("createdAt").is_none(), "{row}");
    assert_eq!(row["mergedAt"], "2026-09-20T10:00:00Z");
    // A key the caller picked is what they asked for: it stays.
    let mut row = merged();
    let picked = ["state", "createdAt"].map(String::from);
    super::rows::drop_implied(&mut row, Some("merged"), false, &picked);
    assert_eq!(row["state"], "merged");
    assert_eq!(row["createdAt"], "2026-09-19T10:00:00Z");
    // `closed` also lists merged PRs: their state stays.
    let mut row = merged();
    super::rows::drop_implied(&mut row, Some("closed"), true, &[]);
    assert_eq!(row["state"], "merged");
    assert_eq!(row["createdAt"], "2026-09-19T10:00:00Z");
}

#[test]
fn fields_keep_the_row_id_then_the_picked_keys_in_request_order() {
    let row = json!({"number": 1, "title": "t", "state": "open", "author": "a"});
    let keys = ["title", "state", "missing"].map(String::from);
    let picked = pick_fields(row.clone(), &["number"], &keys);
    assert_eq!(picked, json!({"number": 1, "title": "t", "state": "open"}));
    assert_eq!(
        picked
            .as_object()
            .map(|row| row.keys().cloned().collect::<Vec<_>>()),
        Some(vec![
            "number".to_owned(),
            "title".to_owned(),
            "state".to_owned()
        ])
    );
    // No pick keeps the row whole.
    assert_eq!(
        pick_fields(row, &["number"], &[]),
        json!({"number": 1, "title": "t", "state": "open", "author": "a"})
    );
}

/// `since`/`until` filter pull requests on their created date, so the rows
/// show it beside `mergedAt`: otherwise a merged row dated outside the window
/// looks like a filter bug.
#[test]
fn rows_filtered_on_the_created_window_keep_created_at() {
    let window = parse(
        r#"{"operation":"pullRequest","owner":"a","repo":"b","filters":{"isMerged":true,"createdAt":{"include":[{"from":"2026-09-01T00:00:00Z"}]}}}"#,
    );
    assert!(super::shape::keeps_created(
        &search(&window).expect("query")
    ));
    let plain =
        parse(r#"{"operation":"pullRequest","owner":"a","repo":"b","filters":{"isMerged":true}}"#);
    assert!(!super::shape::keeps_created(
        &search(&plain).expect("query")
    ));
    let qualifier = parse(
        r#"{"operation":"pullRequest","owner":"a","repo":"b","filters":{"createdAt":{"include":[{"from":"2026-01-02T00:00:00Z"}]}}}"#,
    );
    assert!(super::shape::keeps_created(
        &search(&qualifier).expect("query")
    ));
}

/// A cross-repository PR search names each row's repository as the
/// ghGetHistoryItem inputs (`owner`, `repo`); an owner-scoped search names
/// only `repo` (the owner is the query's own).
#[test]
fn cross_repository_rows_name_owner_and_repo() {
    let item = json!({"number": 3, "repository_url": "https://api.github.com/repos/acme/widget"});
    for ((owner, repo), expected) in [
        ((None, None), json!({"owner":"acme","repo":"widget"})),
        ((Some("Acme"), None), json!({"repo":"widget"})),
        (
            (Some("other"), None),
            json!({"owner":"acme","repo":"widget"}),
        ),
        ((Some("acme"), Some("widget")), json!({})),
    ] {
        let mut row = json!({});
        rows::name_repository(&mut row, &item, owner, repo);
        assert_eq!(row, expected, "{owner:?}/{repo:?}");
    }
}
