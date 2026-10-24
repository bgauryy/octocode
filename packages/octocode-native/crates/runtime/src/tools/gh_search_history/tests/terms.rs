use super::*;

#[test]
fn every_typed_history_filter_reaches_the_provider_query() {
    let cases = [
        (json!({"state":"closed"}), "is:closed"),
        (
            json!({"author":{"include":["alice"],"exclude":["bot"]}}),
            "author:alice -author:bot",
        ),
        (json!({"assignee":{"include":["alice"]}}), "assignee:alice"),
        (
            json!({"commenter":{"include":["alice"]}}),
            "commenter:alice",
        ),
        (json!({"mentions":{"exclude":["bot"]}}), "-mentions:bot"),
        (
            json!({"labels":{"include":["bug","needs work"],"exclude":["blocked"]}}),
            "label:bug label:\"needs work\" -label:blocked",
        ),
        (
            json!({"commentCount":{"include":[{"min":0,"max":10}]}}),
            "comments:0..10",
        ),
        (
            json!({"reactionCount":{"exclude":[{"max":2}]}}),
            "-reactions:<=2",
        ),
        (
            json!({"createdAt":{"include":[{"from":"2024-01-01T00:00:00Z"}]}}),
            "created:>=2024-01-01T00:00:00Z",
        ),
        (
            json!({"updatedAt":{"exclude":[{"to":"2024-02-01T00:00:00Z"}]}}),
            "-updated:<=2024-02-01T00:00:00Z",
        ),
        (
            json!({"closedAt":{"include":[{"from":"2024-01-01T00:00:00Z","to":"2024-02-01T00:00:00Z"}]}}),
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
            json!({"review":{"include":["changesRequested"]}}),
            "review:changes_requested",
        ),
        (
            json!({"checkStatus":{"include":["failure"]}}),
            "status:failure",
        ),
        (
            json!({"reviewRequested":{"include":["alice"]}}),
            "review-requested:alice",
        ),
        (
            json!({"reviewedBy":{"exclude":["bot"]}}),
            "-reviewed-by:bot",
        ),
        (
            json!({"headRef":{"include":["feature/x"]}}),
            "head:feature/x",
        ),
        (json!({"baseRef":{"exclude":["main"]}}), "-base:main"),
        (json!({"linked":{"include":["issue"]}}), "linked:issue"),
        (
            json!({"mergedAt":{"include":[{"from":"2024-01-01T00:00:00Z"}]}}),
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
    let q = serde_json::from_value(json!({"operation":"issue","owner":"o","repo":"r","filters":{"linked":{"include":["pullRequest"]}}})).expect("query");
    assert!(build_query(&q).expect("query").contains("linked:pr"));
}

#[test]
fn commit_terms_and_rest_window_share_inclusive_timestamp_bounds() {
    let filters = json!({"author":{"include":["a@example.test"]},"committer":{"include":["bob"]},"committedAt":{"from":"2024-01-01T00:00:00+02:00","to":"2024-02-01T00:00:00Z"}});
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
        json!({"operation":"commit","owner":"o","repo":"r","keywords":["fix"],"pathPrefix":"src"}),
    ] {
        assert!(serde_json::from_value::<GhSearchHistoryQuery>(row).is_err());
    }
}
