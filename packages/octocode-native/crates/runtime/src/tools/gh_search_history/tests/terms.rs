//! Search query building, routing and validation.
use super::*;

#[test]
fn canonical_issue_qualifier_order() {
    let q: GhSearchHistoryQuery=serde_json::from_str(r#"{"operation":"issue","mainGoal":"test","reasoning":"test","owner":"a","repo":"b","keywords":["x"],"state":"closed","qualifiers":"in:title label:bug"}"#).expect("GitHub history search test data should be valid");
    assert_eq!(
        build_query(&q).expect("GitHub history search test data should be valid"),
        "x in:title is:issue repo:a/b is:closed label:\"bug\""
    );
    let archived: GhSearchHistoryQuery = serde_json::from_str(
        r#"{"operation":"issue","mainGoal":"test","reasoning":"test","owner":"a","repo":"b","keywords":["x"],"qualifiers":"archived:true"}"#,
    )
    .expect("valid");
    assert!(
        build_query(&archived)
            .expect("valid")
            .ends_with("archived:true")
    );
}

/// The `linked:pr` (issues with a linked PR) and `linked:issue` reach the
/// search query on issues and pull requests.
#[test]
fn linked_qualifier_reaches_the_search_query() {
    for (operation, linked) in [("issue", "pr"), ("pullRequest", "issue")] {
        let row = json!({"operation":operation,"mainGoal":"g","reasoning":"r","owner":"o","repo":"r",
            "qualifiers":format!("linked:{linked}")});
        let query = build_query(&serde_json::from_value(row).expect("typed")).expect("query");
        assert!(
            query.contains(&format!("linked:{linked}")),
            "{operation}: {query}"
        );
    }
}

#[test]
fn report_probes_cannot_rewrite_the_history_scope() {
    // A leading-quote keyword, an owner carrying operators,
    // and a label with an interior quote.
    let q = parse(
        r#"{"operation":"issue","mainGoal":"test","reasoning":"test","owner":"octocat","repo":"Hello-World","keywords":["\"hello\" NOT"]}"#,
    );
    assert_eq!(
        build_query(&q).expect("valid"),
        "\"hello NOT\" is:issue repo:octocat/Hello-World"
    );
    let q = parse(
        r#"{"operation":"pullRequest","mainGoal":"test","reasoning":"test","owner":"a","keywords":["repo:evil/x","-y"]}"#,
    );
    assert_eq!(
        build_query(&q).expect("valid"),
        "\"repo:evil/x\" \"-y\" is:pr user:a"
    );
    for raw in [
        r#"{"operation":"issue","mainGoal":"test","reasoning":"test","owner":"octocat OR is:public","repo":"b","keywords":["hello"]}"#,
        r#"{"operation":"issue","mainGoal":"test","reasoning":"test","owner":"a","repo":"b:c","keywords":["hello"]}"#,
        r#"{"operation":"pullRequest","mainGoal":"test","reasoning":"test","owner":"a","qualifiers":"author:\"x OR is:public\""}"#,
        r#"{"operation":"issue","mainGoal":"test","reasoning":"test","owner":"a","repo":"b","qualifiers":"label:x\" OR is:public\""}"#,
        r#"{"operation":"issue","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","qualifiers":"created:\"x OR is:public\""}"#,
        r#"{"operation":"commit","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","keywords":["x"],"qualifiers":"committer:\"a b\""}"#,
    ] {
        // Rejected by the published pattern or by native validation.
        if let Ok(query) = serde_json::from_str::<GhSearchHistoryQuery>(raw) {
            let error = build_query(&query).expect_err(raw);
            assert_eq!(error.kind, ProviderErrorKind::Validation, "{raw}");
        }
    }
    let q = parse(
        r#"{"operation":"issue","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","qualifiers":"label:\"good first issue\" comments:>5 author:dependabot[bot]"}"#,
    );
    let built = build_query(&q).expect("valid");
    assert!(built.contains("label:\"good first issue\""), "{built}");
    assert!(built.contains("comments:>5"), "{built}");
    assert!(built.contains("author:dependabot[bot]"), "{built}");
}

/// `archived` means the same with or without keywords: only search
/// filters on repository archive state, so it always routes there.
#[test]
fn archived_pull_request_filter_always_routes_to_search() {
    let parse = |extra: &str| -> GhSearchHistoryQuery {
        serde_json::from_str(&format!(
            r#"{{"operation":"pullRequest","mainGoal":"test","reasoning":"test","owner":"a","repo":"b"{extra}}}"#
        ))
        .expect("GitHub history search test data should be valid")
    };
    assert!(!should_use_search_for_prs(&parse("")));
    for extra in [
        r#","qualifiers":"archived:true""#,
        r#","qualifiers":"archived:false""#,
        r#","qualifiers":"archived:true","keywords":["README"]"#,
    ] {
        let query = parse(extra);
        assert!(should_use_search_for_prs(&query), "{extra}");
        let built = build_query(&query).expect("query");
        assert!(built.contains("archived:"), "{extra}: {built}");
    }
}

#[test]
fn quotes_multiword_history_keywords() {
    let q: GhSearchHistoryQuery = serde_json::from_str(
        r#"{"operation":"issue","mainGoal":"test","reasoning":"test","owner":"a","repo":"b","keywords":["fix login"]}"#,
    )
    .expect("GitHub history search test data should be valid");
    assert!(
        build_query(&q)
            .expect("GitHub history search test data should be valid")
            .starts_with("\"fix login\"")
    );
    assert!(needs_issue_search_qualifiers(&q));
    let listed: GhSearchHistoryQuery = serde_json::from_str(
        r#"{"operation":"issue","mainGoal":"test","reasoning":"test","owner":"a","repo":"b"}"#,
    )
    .expect("GitHub history search test data should be valid");
    assert!(!needs_issue_search_qualifiers(&listed));
}

#[test]
fn commit_search_uses_email_and_committer_date() {
    let q: GhSearchHistoryQuery = serde_json::from_str(
        r#"{"operation":"commit","mainGoal":"test","reasoning":"test","owner":"a","repo":"b","keywords":["fix"],"qualifiers":"author:dev@example.com committer:web-flow","since":"2026-01-01T00:00:00Z"}"#,
    )
    .expect("GitHub history search test data should be valid");
    let query = build_query(&q).expect("GitHub history search test data should be valid");
    assert!(query.contains("author-email:dev@example.com"), "{query}");
    assert!(query.contains("committer:web-flow"), "{query}");
    assert!(query.contains("committer-date:>=2026-01-01T00:00:00Z"));
}

/// Commit people filters ride `qualifiers` (one name per purpose);
/// issue/PR keys are refused on commits, and `committer:` on PR/issue.
#[test]
fn commit_qualifiers_take_author_and_committer_only() {
    // The contract types each operation's keys: other operations' keys
    // never reach a filter.
    for raw in [
        r#"{"operation":"commit","owner":"a","repo":"b","qualifiers":"label:bug"}"#,
        r#"{"operation":"issue","owner":"a","repo":"b","qualifiers":"committer:x"}"#,
        r#"{"operation":"commit","owner":"a","repo":"b","qualifiers":"reviewed-by:x"}"#,
    ] {
        assert!(
            serde_json::from_str::<GhSearchHistoryQuery>(raw).is_err(),
            "{raw}"
        );
    }
    // The native key table agrees (its errors name where a key applies).
    let commit: GhSearchHistoryQuery = serde_json::from_str(
        r#"{"operation":"commit","owner":"a","repo":"b","qualifiers":"committer:web-flow author:dev"}"#,
    )
    .expect("commit qualifiers");
    let built = build_query(&commit).expect("valid");
    assert!(
        built.contains("author:dev") && built.contains("committer:web-flow"),
        "{built}"
    );
    for field in ["author", "committer"] {
        let raw = format!(r#"{{"operation":"commit","owner":"a","repo":"b","{field}":"x"}}"#);
        assert!(
            serde_json::from_str::<GhSearchHistoryQuery>(&raw).is_err(),
            "typed {field} is gone"
        );
    }
}

#[test]
fn rejects_unscoped_commit() {
    // Commit history is repository-scoped by the wire contract.
    assert!(
        serde_json::from_str::<GhSearchHistoryQuery>(
            r#"{"operation":"commit","mainGoal":"test","reasoning":"test","owner":"a"}"#
        )
        .is_err()
    );
}

/// The `qualifiers` string sets the search filters; scope and unknown
/// keys are rejected.
#[test]
fn qualifiers_set_the_search_filters() {
    let built = |row: Value| build_query(&serde_json::from_value(row).expect("typed"));
    let base = json!({"operation":"pullRequest","mainGoal":"g","reasoning":"r","owner":"o","repo":"r","keywords":["x"]});
    let mut flat = base.clone();
    flat["qualifiers"] = json!(
        r#"reviewed-by:dev review:approved comments:>5 label:"good first issue" -is:draft merged:>2026-01-01"#
    );
    let query = built(flat).expect("flat");
    for term in [
        "reviewed-by:dev",
        "review:approved",
        "comments:>5",
        "label:\"good first issue\"",
        "-is:draft",
        "merged:>2026-01-01",
    ] {
        assert!(query.contains(term), "{term}: {query}");
    }
    // Scope, unknown keys and free text fail the published pattern.
    for bad in ["repo:evil/x", "reviewd-by:dev", "loose words"] {
        let mut row = base.clone();
        row["qualifiers"] = json!(bad);
        assert!(
            serde_json::from_value::<GhSearchHistoryQuery>(row).is_err(),
            "{bad}"
        );
    }
    for (bad, needle) in [("is:pr", "operation"), ("author:a author:b", "set it once")] {
        let mut row = base.clone();
        row["qualifiers"] = json!(bad);
        let error = built(row).expect_err(bad);
        assert!(error.message.contains(needle), "{bad}: {}", error.message);
    }
    assert!(
        serde_json::from_value::<GhSearchHistoryQuery>(json!({"operation":"issue","mainGoal":"g","reasoning":"r","owner":"o","repo":"r","qualifiers":"review:approved"})).is_err(),
        "a PR-only key is no issue qualifier"
    );
}

/// A leading `-` excludes on any pull-request or issue key: the schema
/// admits it, native runs it on the search route, and a typed flag keeps
/// its own spelling (`-is:draft`); commit qualifiers take no negation.
#[test]
fn negated_qualifiers_exclude_on_the_search_route() {
    let row = |operation: &str, qualifiers: &str| {
        json!({"operation":operation,"mainGoal":"g","reasoning":"r","owner":"o","repo":"r",
            "qualifiers":qualifiers})
    };
    let parse = |operation: &str, qualifiers: &str| {
        let prepared =
            crate::contracts::validate_query("ghSearchHistory", row(operation, qualifiers))
                .expect(qualifiers);
        Filters::parse(&serde_json::from_value(prepared).expect("typed"))
    };
    let draft = parse("pullRequest", "-is:draft").expect("native runs -is:draft");
    assert_eq!(draft.draft, Some(false));
    let excluded = parse(
        "pullRequest",
        "label:bug -label:wontfix -author:dependabot[bot]",
    )
    .expect("negated labels and people run");
    assert_eq!(excluded.label, vec!["bug".to_owned()]);
    assert_eq!(
        excluded.excluded,
        vec![
            ("label".to_owned(), "wontfix".to_owned()),
            ("author".to_owned(), "dependabot[bot]".to_owned())
        ]
    );
    let issue = parse("issue", "-label:bug").expect("issue negation runs");
    assert_eq!(issue.excluded, vec![("label".to_owned(), "bug".to_owned())]);
    let state = parse("pullRequest", "-is:open").expect_err("state is the field's");
    assert!(state.message.contains("set state instead"), "{state:?}");
    for (operation, qualifiers, message) in [
        ("commit", "-author:x", "negation is not supported"),
        (
            "issue",
            r#"label:"good first issue" repo:x/y"#,
            "\"repo:x/y\" is not an allowed key:value filter",
        ),
    ] {
        let error = crate::contracts::validate_query("ghSearchHistory", row(operation, qualifiers))
            .expect_err(qualifiers);
        assert!(
            error
                .issues
                .iter()
                .any(|issue| issue.message.contains(message)),
            "{operation} {qualifiers}: {error:?}"
        );
    }
}

#[test]
fn pull_request_search_allows_cross_repo_and_owner_scopes() {
    let both = build_query(&parse(
        r#"{"operation":"pullRequest","mainGoal":"test","reasoning":"test","owner":"a","repo":"b","keywords":["x"]}"#,
    ))
    .expect("scoped");
    assert!(both.contains("repo:a/b"), "{both}");
    let owner = build_query(&parse(
        r#"{"operation":"pullRequest","mainGoal":"test","reasoning":"test","owner":"a","keywords":["x"]}"#,
    ))
    .expect("owner-scoped PR search");
    assert!(
        owner.contains("user:a") && !owner.contains("repo:"),
        "{owner}"
    );
    let global = build_query(&parse(
        r#"{"operation":"pullRequest","mainGoal":"test","reasoning":"test","keywords":["x"]}"#,
    ))
    .expect("cross-repo PR search");
    assert!(
        !global.contains("repo:") && !global.contains("user:"),
        "{global}"
    );
    assert!(!global.contains("archived:"), "{global}");
    // Without a full repo scope the REST list endpoint is unusable, so the
    // PR path must route through search.
    assert!(should_use_search_for_prs(&parse(
        r#"{"operation":"pullRequest","mainGoal":"test","reasoning":"test","owner":"a"}"#
    )));
    assert!(!should_use_search_for_prs(&parse(
        r#"{"operation":"pullRequest","mainGoal":"test","reasoning":"test","owner":"a","repo":"b"}"#
    )));
    // Issues still require the repository scope.
    assert!(
        serde_json::from_str::<GhSearchHistoryQuery>(
            r#"{"operation":"issue","mainGoal":"test","reasoning":"test","keywords":["x"]}"#
        )
        .is_err()
    );
}

/// An unparseable `since`/`until` is invalid input, never a silently
/// unfiltered listing (search or REST list).
#[test]
fn commit_window_with_an_invalid_date_is_rejected() {
    for row in [
        r#"{"operation":"commit","mainGoal":"test","reasoning":"test","owner":"a","repo":"b","keywords":["fix"],"since":"yesterday-ish"}"#,
        r#"{"operation":"commit","mainGoal":"test","reasoning":"test","owner":"a","repo":"b","since":"zz"}"#,
        r#"{"operation":"commit","mainGoal":"test","reasoning":"test","owner":"a","repo":"b","until":"2026-13-01"}"#,
    ] {
        let q = parse(row);
        let error = resolve_commit_window(&q, &mut Vec::new()).expect_err(row);
        assert_eq!(error.kind, ProviderErrorKind::Validation, "{row}");
        assert!(
            error.message.contains("since") || error.message.contains("until"),
            "{}",
            error.message
        );
        assert!(error.message.contains("30d"), "{}", error.message);
        assert!(!error.message.contains("skipped"), "{}", error.message);
    }
}

#[test]
fn inverted_since_until_is_a_validation_error() {
    let q = parse(
        r#"{"operation":"commit","mainGoal":"test","reasoning":"test","owner":"a","repo":"b","keywords":["fix"],"since":"2026-05-01","until":"2026-01-01"}"#,
    );
    let error = build_query(&q).expect_err("since after until");
    assert_eq!(error.kind, ProviderErrorKind::Validation);
    assert!(error.message.contains("since"), "{}", error.message);
    let listed = parse(
        r#"{"operation":"commit","mainGoal":"test","reasoning":"test","owner":"a","repo":"b","since":"2026-05-01","until":"2026-01-01"}"#,
    );
    assert!(build_query(&listed).is_err());
}

/// Branch and label filters are qualifiers: `head:`/`base:` route a pull
/// request search through search, labels add up, and issues reject the
/// pull-request-only branch keys.
#[test]
fn branch_and_label_filters_come_from_qualifiers() {
    let pr = parse(
        r#"{"operation":"pullRequest","owner":"o","repo":"r","qualifiers":"head:feature base:main label:bug label:ui"}"#,
    );
    assert!(should_use_search_for_prs(&pr));
    let built = build_query(&pr).expect("query");
    for term in ["head:feature", "base:main", "label:\"bug\"", "label:\"ui\""] {
        assert!(built.contains(term), "{term}: {built}");
    }
    // Issue qualifiers are typed by the issue key list: a PR-only key is
    // refused before any filter runs.
    assert!(
        serde_json::from_str::<GhSearchHistoryQuery>(
            r#"{"operation":"issue","owner":"o","repo":"r","qualifiers":"head:x"}"#
        )
        .is_err()
    );
    for hidden in ["label", "sourceBranch", "targetBranch"] {
        let mut row = json!({"operation":"pullRequest","owner":"o","repo":"r"});
        row[hidden] = json!(if hidden == "label" {
            json!(["bug"])
        } else {
            json!("x")
        });
        assert!(
            serde_json::from_value::<GhSearchHistoryQuery>(row).is_err(),
            "{hidden} is a qualifier now"
        );
    }
}

/// A resolved window is one range value: both bounds `a..b`, one bound
/// `>=a` / `<=b` (commit `committer-date:` and PR/issue `created:` alike).
#[test]
fn date_windows_render_as_one_search_range() {
    use super::query::window_range;
    assert_eq!(
        window_range(Some("2026-09-01"), Some("2026-09-30")).as_deref(),
        Some("2026-09-01..2026-09-30")
    );
    assert_eq!(
        window_range(Some("2026-09-01"), None).as_deref(),
        Some(">=2026-09-01")
    );
    assert_eq!(
        window_range(None, Some("2026-09-30")).as_deref(),
        Some("<=2026-09-30")
    );
    assert_eq!(window_range(None, None), None);
}

#[test]
fn fields_reach_the_query_without_changing_the_search() {
    let plain =
        r#"{"operation":"pullRequest","mainGoal":"test","reasoning":"test","keywords":["x"]}"#;
    let picked = r#"{"operation":"pullRequest","mainGoal":"test","reasoning":"test","keywords":["x"],"fields":["number","title"]}"#;
    assert_eq!(
        gh_search_history_query::fields(&search(&parse(picked)).expect("search")),
        vec!["number".to_string(), "title".to_string()]
    );
    assert!(gh_search_history_query::fields(&search(&parse(plain)).expect("search")).is_empty());
    assert_eq!(
        build_query(&parse(picked)).expect("query"),
        build_query(&parse(plain)).expect("query")
    );
}

/// PR and issue `since`/`until` reach the search as a `created:` window
/// (they were accepted and silently ignored).
#[test]
fn pull_request_and_issue_windows_reach_the_search() {
    for operation in ["pullRequest", "issue"] {
        let terms = build_query(&parse(&format!(
            r#"{{"operation":"{operation}","owner":"a","repo":"b","since":"2026-10-01","until":"2026-10-03"}}"#
        )))
        .expect("query");
        assert!(
            terms.contains("created:2026-10-01..2026-10-03"),
            "{operation}: {terms}"
        );
    }
}

/// Unmerged pull requests (open or closed) are a GitHub search filter
/// (`is:unmerged`, or `-is:merged`): both run on the search route as
/// `is:unmerged`, beside a `state`; issues have no merge state.
#[test]
fn unmerged_pull_requests_are_a_search_filter() {
    let typed = |operation: &str, state: Option<&str>, qualifiers: &str| {
        let mut row = json!({"operation":operation,"owner":"o","repo":"r","qualifiers":qualifiers});
        if let Some(state) = state {
            row["state"] = json!(state);
        }
        let prepared = crate::contracts::validate_query("ghSearchHistory", row).expect(qualifiers);
        serde_json::from_value::<GhSearchHistoryQuery>(prepared).expect("typed")
    };
    for (state, qualifiers) in [(Some("closed"), "is:unmerged"), (None, "-is:merged")] {
        let query = typed("pullRequest", state, qualifiers);
        let filters = Filters::parse(&query).expect(qualifiers);
        assert_eq!(filters.unmerged, Some(true), "{qualifiers}");
        let terms = build_query(&query).expect(qualifiers);
        assert!(terms.contains("is:unmerged"), "{qualifiers}: {terms}");
        assert_eq!(
            terms.contains("is:closed"),
            state.is_some(),
            "{qualifiers}: {terms}"
        );
        assert!(!terms.contains("-is:merged"), "{qualifiers}: {terms}");
        assert!(should_use_search_for_prs(&query), "{qualifiers}");
    }
    let issue = Filters::parse(&typed("issue", None, "is:unmerged")).expect_err("issues");
    assert!(issue.message.contains("is not supported"), "{issue:?}");
}

/// A merged state beside an unmerged filter asks for no pull request:
/// refused before any request, naming both as the caller wrote them.
#[test]
fn a_merged_state_beside_an_unmerged_filter_is_refused_naming_both() {
    let typed = |state: Option<&str>, qualifiers: &str| {
        let mut row =
            json!({"operation":"pullRequest","owner":"o","repo":"r","qualifiers":qualifiers});
        if let Some(state) = state {
            row["state"] = json!(state);
        }
        let prepared = crate::contracts::validate_query("ghSearchHistory", row).expect(qualifiers);
        serde_json::from_value::<GhSearchHistoryQuery>(prepared).expect("typed")
    };
    for (state, qualifiers, named) in [
        (
            Some("merged"),
            "is:unmerged",
            ["is:unmerged", "state \"merged\""],
        ),
        (
            Some("merged"),
            "-is:merged",
            ["-is:merged", "state \"merged\""],
        ),
        (None, "is:merged is:unmerged", ["is:unmerged", "is:merged"]),
    ] {
        let error = Filters::parse(&typed(state, qualifiers)).expect_err(qualifiers);
        assert_eq!(error.kind, octocode_github::ProviderErrorKind::Validation);
        for name in named {
            assert!(
                error.message.contains(name),
                "{qualifiers}: {}",
                error.message
            );
        }
    }
    // An open or closed state keeps the filter.
    for state in ["open", "closed"] {
        assert!(
            Filters::parse(&typed(Some(state), "is:unmerged")).is_ok(),
            "{state}"
        );
    }
}

/// Without its person qualifiers a search may name nothing else: no lead
/// then, never an all-of-GitHub search.
#[test]
fn without_users_never_leads_to_an_unscoped_search() {
    let unscoped: GhSearchHistoryQuery = serde_json::from_value(json!({
        "operation":"pullRequest","qualifiers":"author:ghost","pageSize":2
    }))
    .expect("query");
    assert!(super::leads::without_users(&unscoped).is_none());
    let scoped: GhSearchHistoryQuery = serde_json::from_value(json!({
        "operation":"pullRequest","owner":"o","qualifiers":"author:ghost"
    }))
    .expect("query");
    let lead = super::leads::without_users(&scoped).expect("an owner-scoped lead");
    assert_eq!(lead["query"]["queries"][0]["owner"], "o", "{lead}");
}

/// A missing login's failure states GitHub's words and the `withoutUsers`
/// repair; no hint restates that the login does not resolve.
#[test]
fn a_missing_login_has_no_restating_hint() {
    use octocode_github::ProviderErrorReason;
    let error = ProviderError::new(ProviderErrorKind::NotFound, "no user named \"ghost\"")
        .with_reason(ProviderErrorReason::UserNotFound);
    let (message, hint) = history_failure(&error, true);
    assert!(message.contains("ghost"), "{message}");
    assert_eq!(hint, None);
}

/// A relative window (`30d`) on any date qualifier resolves to the
/// absolute lower bound GitHub understands, as ghSearchRepo's
/// `pushed`/`created` do; search would read a bare `30d` as no date and
/// match nothing. A value that is no date is refused, not sent.
#[test]
fn date_qualifiers_resolve_relative_windows_or_refuse_non_dates() {
    for (operation, key) in [
        ("issue", "updated"),
        ("issue", "closed"),
        ("issue", "created"),
        ("pullRequest", "merged"),
    ] {
        let row = json!({"operation":operation,"mainGoal":"g","reasoning":"r","owner":"o","repo":"r",
            "qualifiers":format!("{key}:30d")});
        let query = build_query(&serde_json::from_value(row).expect("typed")).expect("query");
        let term = query
            .split(' ')
            .find(|term| term.starts_with(&format!("{key}:")))
            .unwrap_or_else(|| panic!("{key} term in {query}"));
        assert!(term.starts_with(&format!("{key}:>=")), "{query}");
        assert_eq!(term.len(), format!("{key}:>=2026-01-01").len(), "{query}");
    }
    for value in [
        ">=2026-01-01",
        "2026-01-01..2026-02-01",
        "<2026-01-01T10:00:00Z",
        "*..2026-01-01",
    ] {
        let row = json!({"operation":"issue","mainGoal":"g","reasoning":"r","owner":"o","repo":"r",
            "qualifiers":format!("updated:{value}")});
        let query = build_query(&serde_json::from_value(row).expect("typed")).expect("query");
        assert!(query.contains(&format!("updated:{value}")), "{query}");
    }
    for value in ["yesterday", "30days", "2026"] {
        let row = json!({"operation":"issue","mainGoal":"g","reasoning":"r","owner":"o","repo":"r",
            "qualifiers":format!("updated:{value}")});
        let error = build_query(&serde_json::from_value(row).expect("typed"))
            .expect_err("a non-date is refused");
        assert!(error.message.contains("updated"), "{}", error.message);
        assert!(error.message.contains("30d"), "{}", error.message);
    }
}
