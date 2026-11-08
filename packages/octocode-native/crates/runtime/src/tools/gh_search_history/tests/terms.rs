use super::*;

#[test]
fn every_typed_history_filter_reaches_the_provider_query() {
    let cases = [
        (json!({"state":"closed"}), "is:closed"),
        (
            json!({"author":"alice","notAuthor":["bot"]}),
            "author:alice -author:bot",
        ),
        (json!({"assignee":"alice"}), "assignee:alice"),
        (json!({"commenter":"alice"}), "commenter:alice"),
        (json!({"notMentions":["bot"]}), "-mentions:bot"),
        (
            json!({"labels":["bug","needs work"],"notLabels":["blocked"]}),
            "label:bug label:\"needs work\" -label:blocked",
        ),
        (json!({"commentCount":{"min":0,"max":10}}), "comments:0..10"),
        (json!({"notReactionCount":[{"max":2}]}), "-reactions:<=2"),
        (
            json!({"createdAt":{"from":"2024-01-01T00:00:00Z"}}),
            "created:>=2024-01-01T00:00:00Z",
        ),
        (
            json!({"notUpdatedAt":[{"to":"2024-02-01T00:00:00Z"}]}),
            "-updated:<=2024-02-01T00:00:00Z",
        ),
        (
            json!({"closedAt":{"from":"2024-01-01T00:00:00Z","to":"2024-02-01T00:00:00Z"}}),
            "closed:2024-01-01T00:00:00Z..2024-02-01T00:00:00Z",
        ),
        (json!({"archive":"only"}), "archived:true"),
        (json!({"archive":"exclude"}), "archived:false"),
        (json!({"matchIn":["title","body"]}), "in:title,body"),
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
        (json!({"isMerged":false}), "is:unmerged"),
        (json!({"isMerged":true}), "is:merged"),
        (json!({"isDraft":false}), "-is:draft"),
        (json!({"isDraft":true}), "is:draft"),
        (
            json!({"review":"changesRequested"}),
            "review:changes_requested",
        ),
        (json!({"checkStatus":"failure"}), "status:failure"),
        (json!({"reviewRequested":"alice"}), "review-requested:alice"),
        (json!({"notReviewedBy":["bot"]}), "-reviewed-by:bot"),
        (json!({"headRef":"feature/x"}), "head:feature/x"),
        (json!({"notBaseRef":["main"]}), "-base:main"),
        (json!({"linked":"issue"}), "linked:issue"),
        (
            json!({"mergedAt":{"from":"2024-01-01T00:00:00Z"}}),
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
    let q = serde_json::from_value(
        json!({"operation":"issue","owner":"o","repo":"r","filters":{"linked":"pullRequest"}}),
    )
    .expect("query");
    assert!(build_query(&q).expect("query").contains("linked:pr"));
}

#[test]
fn commit_search_terms_carry_the_flat_date_range() {
    let range = json!({"from":"2024-01-01T00:00:00+02:00","to":"2024-02-01T00:00:00Z"});
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
    let filters =
        json!({"committedAt":{"from":"2024-01-01T00:00:00+02:00","to":"2024-02-01T00:00:00Z"}});
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
        json!({"qualifiers":"author:alice"}),
        json!({"since":"30d"}),
        json!({"state":"open"}),
        json!({"filters":{"review":"approved"}}),
    ] {
        let mut row = json!({"operation":"issue","owner":"o","repo":"r"});
        row.as_object_mut()
            .expect("object")
            .extend(extra.as_object().expect("object").clone());
        assert!(serde_json::from_value::<GhSearchHistoryQuery>(row).is_err());
    }
    for row in [
        json!({"operation":"commit","keywords":["fix"]}),
        json!({"operation":"commit","owner":"o","repo":"r","keywords":["fix"],"path":"src"}),
    ] {
        assert!(serde_json::from_value::<GhSearchHistoryQuery>(row).is_err());
    }
}

#[test]
fn a_field_and_its_not_field_both_reach_the_provider_query() {
    let date = "2024-01-01T00:00:00Z";
    let shared = [
        (
            json!({"author":"alice","notAuthor":["bot","dep"]}),
            vec!["author:alice", "-author:bot", "-author:dep"],
        ),
        (
            json!({"labels":["bug"],"notLabels":["wip"]}),
            vec!["label:bug", "-label:wip"],
        ),
        (
            json!({"createdAt":{"from":date},"notCreatedAt":[{"to":"2023-01-01T00:00:00Z"}]}),
            vec![
                "created:>=2024-01-01T00:00:00Z",
                "-created:<=2023-01-01T00:00:00Z",
            ],
        ),
        (
            json!({"commentCount":{"min":1},"notCommentCount":[{"max":5}]}),
            vec!["comments:>=1", "-comments:<=5"],
        ),
    ];
    for (filters, terms) in &shared {
        for operation in ["issue", "pullRequest"] {
            let q = serde_json::from_value(
                json!({"operation":operation,"owner":"o","repo":"r","filters":filters}),
            )
            .expect("query");
            let actual = build_query(&q).expect("provider query");
            for term in terms {
                assert!(
                    actual.contains(term),
                    "{operation} {filters}: {actual} lacks {term}"
                );
            }
        }
    }
    let pull_request = json!({
        "review":"approved","notReview":["changesRequested"],
        "checkStatus":"success","notCheckStatus":["failure"],
        "headRef":"a","notHeadRef":["b"],
        "mergedAt":{"from":date},"notMergedAt":[{"to":"2023-01-01T00:00:00Z"}],
        "linked":"issue",
    });
    let q = serde_json::from_value(
        json!({"operation":"pullRequest","owner":"o","repo":"r","filters":pull_request}),
    )
    .expect("query");
    let actual = build_query(&q).expect("provider query");
    for term in [
        "review:approved",
        "-review:changes_requested",
        "status:success",
        "-status:failure",
        "head:a",
        "-head:b",
        "merged:>=2024-01-01T00:00:00Z",
        "-merged:<=2023-01-01T00:00:00Z",
        "linked:issue",
    ] {
        assert!(actual.contains(term), "{actual} lacks {term}");
    }
}

#[test]
fn not_linked_excludes_the_linked_kind_and_linked_with_it_is_rejected() {
    let q = serde_json::from_value(
        json!({"operation":"issue","owner":"o","repo":"r","filters":{"notLinked":"pullRequest"}}),
    )
    .expect("query");
    assert!(build_query(&q).expect("query").contains("-linked:pr"));
    for (operation, linked) in [("issue", "pullRequest"), ("pullRequest", "issue")] {
        let row = json!({"operation":operation,"owner":"o","repo":"r",
            "filters":{"linked":linked,"notLinked":linked}});
        assert!(
            crate::contracts::validate("ghSearchHistory", json!({"queries":[row]})).is_err(),
            "{operation}: linked and notLinked together must be rejected"
        );
    }
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
        json!({"reviewRequested":"acme/core","notReviewRequested":["acme/infra","bob"]}),
    );
    for expected in [
        "team-review-requested:acme/core",
        "-team-review-requested:acme/infra",
        "-review-requested:bob",
    ] {
        assert!(
            terms.iter().any(|t| t == expected),
            "{terms:?} lacks {expected}"
        );
    }
    assert!(
        !terms.iter().any(|t| t == "review-requested:acme/core"),
        "{terms:?}"
    );
    let terms = query_terms("pullRequest", json!({"reviewRequested":"alice"}));
    assert!(
        terms.iter().any(|t| t == "review-requested:alice"),
        "{terms:?}"
    );
}

#[test]
fn negated_twins_reach_the_provider_query_in_their_quoted_form() {
    for (operation, filters, expected) in [
        ("issue", json!({"notAssignee":["bot"]}), "-assignee:bot"),
        ("issue", json!({"notCommenter":["bot"]}), "-commenter:bot"),
        (
            "issue",
            json!({"notClosedAt":[{"from":"2024-01-01T00:00:00Z"}]}),
            "-closed:>=2024-01-01T00:00:00Z",
        ),
        (
            "issue",
            json!({"notLabels":["good first issue"]}),
            "-label:\"good first issue\"",
        ),
    ] {
        let terms = query_terms(operation, filters.clone());
        assert!(
            terms.iter().any(|t| t == expected),
            "{filters}: {terms:?} lacks {expected}"
        );
    }
}
