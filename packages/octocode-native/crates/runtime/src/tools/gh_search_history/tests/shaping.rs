//! Row shaping, leads and broadening of one page.
use super::*;

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

/// The default PR read targets the merged fix, not the first
/// (unmerged) search row; the rows already name the others.
#[test]
fn read_pr_prefers_the_first_merged_row() {
    let rows = [
        json!({"number":13794,"state":"closed","pull_request":{"merged_at":null}}),
        json!({"number":13825,"state":"closed","pull_request":{"merged_at":"2026-09-25T00:29:36Z"}}),
        json!({"number":13787,"state":"open"}),
    ];
    let target = read_target(&rows, is_merged).expect("rows");
    assert_eq!(item_number(&rows[target]), Some(13825));
    // No merged row: the first row, unchanged.
    let open = [
        json!({"number":5,"state":"open"}),
        json!({"number":6,"state":"closed"}),
    ];
    assert_eq!(read_target(&open, is_merged), Some(0));
    assert_eq!(read_target(&[], is_merged), None);
    let read = pr_read_query("o", "r", 13825);
    assert_eq!(read["number"], 13825);
    assert_eq!(read["sections"], json!(["body", "files"]));
    assert!(read.get("pageSize").is_none(), "{read}");
}

/// The issue read prefers an issue closed as completed (its fix landed).
#[test]
fn read_issue_prefers_a_completed_issue() {
    let rows = [
        json!({"number":1,"state":"open"}),
        json!({"number":2,"state":"closed","state_reason":"not_planned"}),
        json!({"number":3,"state":"closed","state_reason":"completed"}),
    ];
    let target = read_target(&rows, is_completed).expect("rows");
    assert_eq!(item_number(&rows[target]), Some(3));
    let read = issue_read_query("o", "r", 3);
    assert_eq!(read["sections"], json!(["body", "comments"]), "{read}");
}

/// A bare issue number in the keywords links straight to that issue's
/// closing pull requests.
#[test]
fn bare_issue_number_keywords_offer_the_issue_links_read() {
    let q = parse(
        r##"{"operation":"pullRequest","mainGoal":"g","reasoning":"r","owner":"o","repo":"r","keywords":["#13786"]}"##,
    );
    let read = issue_links_read(&q).expect("bare number");
    assert_eq!(read["query"]["queries"][0]["operation"], "issue");
    assert_eq!(read["query"]["queries"][0]["number"], 13786);
    let words = parse(
        r#"{"operation":"pullRequest","mainGoal":"g","reasoning":"r","owner":"o","repo":"r","keywords":["fix 13786"]}"#,
    );
    assert!(issue_links_read(&words).is_none());
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
    for row in [map_commit(raw.clone()), map_commit_list(raw.clone())] {
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
    assert_eq!(map_commit(commit.clone())["date"], "2024-02-29T23:30:00Z");
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
/// commits, pull requests and issues all take `since`/`until`).
#[test]
fn empty_searches_offer_a_broader_search() {
    let broadened = |row: Value| {
        let query: GhSearchHistoryQuery = serde_json::from_value(row).expect("query");
        super::leads::broaden_search(&query).map(|lead| lead["query"]["queries"][0].clone())
    };
    let scoped: GhSearchHistoryQuery = serde_json::from_value(json!({"operation":"pullRequest",
        "owner":"o","repo":"r","keywords":["fix"],"qualifiers":"author:x","page":2}))
    .expect("query");
    let scoped = super::leads::broaden_search(&scoped).expect("lead")["query"]["queries"].clone();
    // A variant keeps at least one keyword: dropping the only one would
    // list the scope, unrelated to the topic searched.
    assert_eq!(scoped.as_array().map(Vec::len), Some(1), "{scoped}");
    assert!(scoped[0].get("qualifiers").is_none(), "{scoped}");
    assert!(scoped[0].get("page").is_none(), "{scoped}");
    assert_eq!(scoped[0]["keywords"], json!(["fix"]), "{scoped}");
    // Every narrowing filter gets its own variant: a scoped keyword search
    // that is empty because of its date window is recoverable too.
    let both: GhSearchHistoryQuery = serde_json::from_value(json!({"operation":"pullRequest",
        "owner":"o","repo":"r","keywords":["fix"],"since":"2030-01-01"}))
    .expect("query");
    let both = super::leads::broaden_search(&both).expect("lead")["query"]["queries"].clone();
    assert_eq!(both.as_array().map(Vec::len), Some(1), "{both}");
    assert!(
        both[0].get("since").is_none() && both[0]["keywords"] == json!(["fix"]),
        "{both}"
    );
    let qualifiers = broadened(json!({"operation":"issue","owner":"o","repo":"r",
        "qualifiers":"label:bug"}))
    .expect("qualifiers dropped");
    assert!(qualifiers.get("qualifiers").is_none(), "{qualifiers}");
    let window = broadened(json!({"operation":"commit","owner":"o","repo":"r",
        "path":"src","since":"30d"}))
    .expect("window dropped");
    assert!(window.get("since").is_none(), "{window}");
    assert_eq!(window["path"], "src", "{window}");
    for operation in ["pullRequest", "issue"] {
        let window = broadened(json!({"operation":operation,"owner":"o","repo":"r",
            "since":"2026-10-01","until":"2026-10-03"}))
        .expect("window dropped");
        assert!(window.get("since").is_none(), "{operation}: {window}");
        assert!(window.get("until").is_none(), "{operation}: {window}");
    }
    assert!(broadened(json!({"operation":"commit","owner":"o","repo":"r"})).is_none());
}

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
            map_commit(commit(message)),
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

/// An empty search with several keywords offers one drop-one variant
/// per keyword (at most 5, the last keyword dropped first) instead of
/// dropping them all; a lone keyword is never dropped.
#[test]
fn empty_multi_keyword_search_broadens_one_keyword_at_a_time() {
    let variants = |keywords: Value| {
        let query: GhSearchHistoryQuery = serde_json::from_value(json!({"operation":"pullRequest",
            "owner":"o","repo":"r","keywords":keywords,"pageSize":7}))
        .expect("query");
        super::leads::broaden_search(&query).expect("lead")["query"]["queries"].clone()
    };
    let three = variants(json!(["a", "b", "c"]));
    let kept = three
        .as_array()
        .expect("queries")
        .iter()
        .map(|row| row["keywords"].clone())
        .collect::<Vec<_>>();
    assert_eq!(
        kept,
        vec![json!(["a", "b"]), json!(["a", "c"]), json!(["b", "c"])]
    );
    // A variant is a fresh search from its first page.
    assert!(
        three
            .as_array()
            .expect("queries")
            .iter()
            .all(|row| row.get("pageSize").is_none())
    );
    // The only keyword is never dropped: nothing broader is offered.
    let one: GhSearchHistoryQuery = serde_json::from_value(json!({"operation":"pullRequest",
        "owner":"o","repo":"r","keywords":["a"]}))
    .expect("query");
    assert!(super::leads::broaden_search(&one).is_none());
    let seven = variants(json!(["a", "b", "c", "d", "e", "f", "g"]));
    assert_eq!(seven.as_array().map(Vec::len), Some(5), "{seven}");
    assert_eq!(seven[0]["keywords"], json!(["a", "b", "c", "d", "e", "f"]));
    assert_eq!(seven[4]["keywords"], json!(["a", "b", "d", "e", "f", "g"]));
}

/// The PR read of a relevance-ordered search targets row 0 (the best
/// match), not the first merged row; a sorted page keeps merged-first.
#[test]
fn pull_request_read_targets_the_best_match_unless_sorted() {
    let rows = [
        json!({"number":10,"state":"open"}),
        json!({"number":11,"state":"closed","pull_request":{"merged_at":"2026-01-01T00:00:00Z"}}),
    ];
    let read = |query: Value| {
        let query: GhSearchHistoryQuery = serde_json::from_value(query).expect("query");
        super::leads::read_pull_request(&query, &rows).expect("read")["query"]["queries"][0]["number"].clone()
    };
    assert_eq!(
        read(json!({"operation":"pullRequest","owner":"o","repo":"r","keywords":["x"]})),
        10
    );
    assert_eq!(
        read(
            json!({"operation":"pullRequest","owner":"o","repo":"r","keywords":["x"],"sort":"updated:desc"})
        ),
        11
    );
    assert_eq!(
        read(json!({"operation":"pullRequest","owner":"o","repo":"r"})),
        11
    );
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
        r#"{"operation":"pullRequest","owner":"a","repo":"b","state":"merged","since":"2026-09-01"}"#,
    );
    assert!(super::shape::keeps_created(
        &search(&window).expect("query")
    ));
    let plain = parse(r#"{"operation":"pullRequest","owner":"a","repo":"b","state":"merged"}"#);
    assert!(!super::shape::keeps_created(
        &search(&plain).expect("query")
    ));
    let qualifier = parse(
        r#"{"operation":"pullRequest","owner":"a","repo":"b","qualifiers":"created:>2026-01-01"}"#,
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
