use super::*;
use octocode_test_support::json_fixture;

#[test]
fn every_typed_history_filter_reaches_the_provider_query() {
    let cases = [
        (json_fixture(r#"{"state": "closed"}"#), "is:closed"),
        (json_fixture(r#"{"author": "alice"}"#), "author:alice"),
        (json_fixture(r#"{"assignee": "alice"}"#), "assignee:alice"),
        (json_fixture(r#"{"commenter": "alice"}"#), "commenter:alice"),
        (json_fixture(r#"{"mentions": "bot"}"#), "mentions:bot"),
        (
            json_fixture(r#"{"labels": ["bug", "needs work"]}"#),
            "label:bug label:\"needs work\"",
        ),
        (
            json_fixture(r#"{"commentsCount": {"min": 0, "max": 10}}"#),
            "comments:0..10",
        ),
        (
            json_fixture(r#"{"reactionCount": {"max": 2}}"#),
            "reactions:<=2",
        ),
        (
            json_fixture(r#"{"createdAt": {"from": "2024-01-01T00:00:00Z"}}"#),
            "created:>=2024-01-01T00:00:00Z",
        ),
        (
            json_fixture(r#"{"updatedAt": {"to": "2024-02-01T00:00:00Z"}}"#),
            "updated:<=2024-02-01T00:00:00Z",
        ),
        (
            json_fixture(
                r#"{
                    "closedAt": {
                        "from": "2024-01-01T00:00:00Z",
                        "to": "2024-02-01T00:00:00Z"
                    }
                }"#,
            ),
            "closed:2024-01-01T00:00:00Z..2024-02-01T00:00:00Z",
        ),
        (json_fixture(r#"{"archive": "only"}"#), "archived:true"),
        (json_fixture(r#"{"archive": "exclude"}"#), "archived:false"),
        (
            json_fixture(r#"{"matchIn": ["title", "body"]}"#),
            "in:title,body",
        ),
    ];
    for operation in ["issue", "pullRequest"] {
        for (filters, expected) in &cases {
            let q: GhSearchHistoryQuery = serde_json::from_value(
                json!({"operation":operation,"owner":"o","repo":"r","filters":filters}),
            )
            .expect("canonical typed filters");
            let actual = build_query(&q).expect("query");
            for term in expected.split(" ").filter(|term| !term.is_empty()) {
                assert!(actual.contains(term), "{filters}: {actual} lacks {term}");
            }
        }
    }
}

#[test]
fn pull_request_predicates_are_distinct_and_negation_survives() {
    for (filters, expected) in [
        (json_fixture(r#"{"isMerged": false}"#), "is:unmerged"),
        (json_fixture(r#"{"isMerged": true}"#), "is:merged"),
        (json_fixture(r#"{"isDraft": false}"#), "-is:draft"),
        (json_fixture(r#"{"isDraft": true}"#), "is:draft"),
        (
            json_fixture(r#"{"review": "changesRequested"}"#),
            "review:changes_requested",
        ),
        (
            json_fixture(r#"{"checkStatus": "failure"}"#),
            "status:failure",
        ),
        (
            json_fixture(r#"{"reviewRequested": "alice"}"#),
            "review-requested:alice",
        ),
        (json_fixture(r#"{"reviewedBy": "bot"}"#), "reviewed-by:bot"),
        (
            json_fixture(r#"{"headRef": "feature/x"}"#),
            "head:feature/x",
        ),
        (json_fixture(r#"{"baseRef": "main"}"#), "base:main"),
        (json_fixture(r#"{"linked": "issue"}"#), "linked:issue"),
        (
            json_fixture(r#"{"mergedAt": {"from": "2024-01-01T00:00:00Z"}}"#),
            "merged:>=2024-01-01T00:00:00Z",
        ),
    ] {
        let q = serde_json::from_value(
            json!({"operation":"pullRequest","owner":"o","repo":"r","filters":filters}),
        )
        .expect("query");
        assert!(
            build_query(&q).expect("provider query").contains(expected),
            "{filters}: {expected}"
        );
        assert!(
            should_use_search_for_prs(&q),
            "filters must not be discarded by REST list routing"
        );
    }
    let q = serde_json::from_value(json_fixture(
        r#"{
            "operation": "issue",
            "owner": "o",
            "repo": "r",
            "filters": {"linked": "pullRequest"}
        }"#,
    ))
    .expect("query");
    assert!(build_query(&q).expect("query").contains("linked:pr"));
}

#[test]
fn commit_search_terms_carry_the_flat_date_range() {
    let range = json_fixture(
        r#"{
            "from": "2024-01-01T00:00:00+02:00",
            "to": "2024-02-01T00:00:00Z"
        }"#,
    );
    let filters = json!({"author":"a@example.test","committer":"bob","committedAt":range});
    let q = serde_json::from_value(json!({"operation":"commit","owner":"o","repo":"r","keywords":["fix parser"],"filters":filters})).expect("query");
    let terms = build_query(&q).expect("terms");
    for expected in [
        "\"fix parser\"",
        "author-email:a@example.test",
        "committer:bob",
        "committer-date:2024-01-01T00:00:00+02:00..2024-02-01T00:00:00Z",
    ] {
        assert!(terms.contains(expected), "{terms}");
    }
}

#[test]
fn rest_commit_window_keeps_plain_inclusive_timestamp_bounds() {
    let filters = json_fixture(
        r#"{
            "committedAt": {
                "from": "2024-01-01T00:00:00+02:00",
                "to": "2024-02-01T00:00:00Z"
            }
        }"#,
    );
    let q = serde_json::from_value(
        json!({"operation":"commit","owner":"o","repo":"r","path":"src","filters":filters}),
    )
    .expect("query");
    assert_eq!(
        resolve_commit_window(&search(&q).expect("request")),
        (
            Some("2024-01-01T00:00:00+02:00".into()),
            Some("2024-02-01T00:00:00Z".into())
        )
    );
}

#[test]
fn legacy_aliases_and_cross_operation_filters_are_rejected() {
    for extra in [
        json_fixture(r#"{"qualifiers": "author:alice"}"#),
        json_fixture(r#"{"since": "30d"}"#),
        json_fixture(r#"{"state": "open"}"#),
        json_fixture(r#"{"filters": {"review": "approved"}}"#),
    ] {
        let mut row = json_fixture(r#"{"operation": "issue", "owner": "o", "repo": "r"}"#);
        row.as_object_mut()
            .expect("object")
            .extend(extra.as_object().expect("object").clone());
        assert!(serde_json::from_value::<GhSearchHistoryQuery>(row).is_err());
    }
    for row in [
        json_fixture(r#"{"operation": "commit", "keywords": ["fix"]}"#),
        json_fixture(
            r#"{
                "operation": "commit",
                "owner": "o",
                "repo": "r",
                "keywords": ["fix"],
                "path": "src"
            }"#,
        ),
    ] {
        assert!(serde_json::from_value::<GhSearchHistoryQuery>(row).is_err());
    }
}

/// Before 2026-11-19 each filter had a `not<Field>` twin sent as a
/// `-key:value` term (and `linked` with `notLinked` was refused); the owner
/// removed the twins, so admission now refuses every one of them.
#[test]
fn not_field_filters_are_refused_at_admission() {
    let date = "2024-01-01T00:00:00Z";
    let shared = [
        json!({"notAuthor":["bot"]}),
        json!({"notAssignee":["bot"]}),
        json!({"notCommenter":["bot"]}),
        json!({"notMentions":["bot"]}),
        json!({"notLabels":["wip"]}),
        json!({"notCreatedAt":[{"to":date}]}),
        json!({"notUpdatedAt":[{"to":date}]}),
        json!({"notClosedAt":[{"from":date}]}),
        json!({"notCommentsCount":[{"max":5}]}),
        json!({"notReactionCount":[{"max":2}]}),
    ];
    let pull_request = [
        json!({"notLinked":"issue"}),
        json!({"notReview":["changesRequested"]}),
        json!({"notCheckStatus":["failure"]}),
        json!({"notHeadRef":["b"]}),
        json!({"notBaseRef":["main"]}),
        json!({"notMergedAt":[{"to":date}]}),
        json!({"notReviewedBy":["bot"]}),
        json!({"notReviewRequested":["bob"]}),
    ];
    let issue_linked = json!({"notLinked":"pullRequest"});
    let cases = shared
        .iter()
        .flat_map(|filters| [("issue", filters), ("pullRequest", filters)])
        .chain(pull_request.iter().map(|filters| ("pullRequest", filters)))
        .chain(std::iter::once(("issue", &issue_linked)));
    for (operation, filters) in cases {
        let row = json!({"operation":operation,"owner":"o","repo":"r","filters":filters});
        assert!(
            octocode_runtime_base::contracts::validate("ghSearchHistory", json!({"queries":[row]}))
                .is_err(),
            "{operation} {filters}: a not<Field> filter must be refused"
        );
    }
    let q = serde_json::from_value(json_fixture(
        r#"{
            "operation": "issue",
            "owner": "o",
            "repo": "r",
            "filters": {"labels": ["good first issue"]}
        }"#,
    ))
    .expect("query");
    let terms = build_query(&q).expect("query");
    assert!(!terms.contains(" -"), "no exclusion term is sent: {terms}");
}

fn query_terms(operation: &str, filters: Value) -> Vec<String> {
    let q: GhSearchHistoryQuery = serde_json::from_value(
        json!({"operation":operation,"owner":"o","repo":"r","filters":filters}),
    )
    .expect("query");
    let query = build_query(&q).expect("provider query");
    // Split on spaces outside double quotes, so `-label:"a b"` stays one term.
    let mut terms = vec![String::new()];
    let mut quoted = false;
    for c in query.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                terms.last_mut().expect("term").push(c);
            }
            ' ' if !quoted => terms.push(String::new()),
            _ => terms.last_mut().expect("term").push(c),
        }
    }
    terms
}

#[test]
fn review_requests_name_a_team_by_its_slash_and_a_login_by_its_name() {
    let terms = query_terms(
        "pullRequest",
        json_fixture(r#"{"reviewRequested": "acme/core"}"#),
    );
    assert!(
        terms.iter().any(|t| t == "team-review-requested:acme/core"),
        "{terms:?} lacks team-review-requested:acme/core"
    );
    assert!(
        !terms.iter().any(|t| t == "review-requested:acme/core"),
        "{terms:?}"
    );
    let terms = query_terms(
        "pullRequest",
        json_fixture(r#"{"reviewRequested": "alice"}"#),
    );
    assert!(
        terms.iter().any(|t| t == "review-requested:alice"),
        "{terms:?}"
    );
}

/// Before 2026-11-19 this pinned the quoted form of the `not<Field>` twins
/// (`-label:"good first issue"`); the twins are gone, so it pins the
/// positive filters' quoted form.
#[test]
fn filter_values_reach_the_provider_query_in_their_quoted_form() {
    for (operation, filters, expected) in [
        (
            "issue",
            json_fixture(r#"{"assignee": "bot"}"#),
            "assignee:bot",
        ),
        (
            "issue",
            json_fixture(r#"{"commenter": "bot"}"#),
            "commenter:bot",
        ),
        (
            "issue",
            json_fixture(r#"{"closedAt": {"from": "2024-01-01T00:00:00Z"}}"#),
            "closed:>=2024-01-01T00:00:00Z",
        ),
        (
            "issue",
            json_fixture(r#"{"labels": ["good first issue"]}"#),
            "label:\"good first issue\"",
        ),
    ] {
        let terms = query_terms(operation, filters.clone());
        assert!(
            terms.iter().any(|t| t == expected),
            "{filters}: {terms:?} lacks {expected}"
        );
    }
}
