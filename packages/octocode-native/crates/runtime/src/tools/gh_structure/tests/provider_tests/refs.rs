//! Refs reads: a missing repository, a named ref, tag order, branch leads and refs paging.
use super::*;

/// A repository that does not resolve (a typo) leads to a search for
/// its name, as the search tools' failures do.
#[tokio::test]
async fn a_missing_repository_leads_to_a_repository_search() {
    let server = MockServer::builder().start().await;
    for route in ["/api/v3/repos/a/typo", "/api/v3/repos/a/typo/commits/HEAD"] {
        mount_json(&server, route, 404, json!({"message":"Not Found"})).await;
    }
    let home = tempfile::tempdir().expect("home");
    let provider = mock_provider(&server, RetryPolicy::default());
    let context = default_context();
    let failure = run(
        &provider,
        &query(json!({"owner":"a","repo":"typo"})),
        Ok(&context),
        home.path(),
    )
    .await
    .expect_err("missing repository");
    let lead = &failure.next.as_ref().expect("next")["findRepository"];
    assert_eq!(lead["tool"], "ghSearchRepo", "{lead}");
    assert_eq!(
        lead["query"]["queries"][0]["keywords"],
        json!(["typo"]),
        "{lead}"
    );
}

/// A `refs` read of one `ref` says what the name resolved to
/// (branch, tag or commit), not only the commit it points at.
#[tokio::test]
async fn a_named_ref_states_its_kind() {
    let server = server().await;
    for (reference, branch, tag) in [
        ("main", Some(SHA), None),
        ("v1.0", None, Some(SHA)),
        (SHA, None, None),
    ] {
        if reference != "main" && reference != SHA {
            Mock::given(method("GET"))
                .and(path(format!("/api/v3/repos/a/b/commits/{reference}")))
                .respond_with(ResponseTemplate::new(200).set_body_string(SHA))
                .mount(&server)
                .await;
        }
        for (kind, found) in [("heads", branch), ("tags", tag)] {
            let route = format!("/api/v3/repos/a/b/git/ref/{kind}/{reference}");
            match found {
                Some(sha) => mount_json(&server, route, 200, json!({"object":{"sha":sha}})).await,
                None => mount_json(&server, route, 404, json!({"message":"Not Found"})).await,
            }
        }
    }
    for (reference, kind) in [("main", "branch"), ("v1.0", "tag"), (SHA, "commit")] {
        let out = list(
            &server,
            json!({"owner":"a","repo":"b","operation":"refs","ref":reference}),
        )
        .await
        .expect("refs");
        assert_eq!(out.data["commitSha"], SHA, "{reference}: {}", out.data);
        assert_eq!(out.data["refKind"], kind, "{reference}: {}", out.data);
    }
}

/// Tags list newest version first across the whole tag set (GitHub's
/// tag pages run reverse-lexicographic, so a repository's own latest
/// release can sit on its last page): page 1 answers "latest tag", the
/// count is exact, and the next page continues the same order. A tag
/// set that cannot be read whole falls back to GitHub's page, which
/// says its order.
#[tokio::test]
async fn tags_list_newest_version_first_across_pages() {
    let ordered = server().await;
    let names = [
        "wincolor-0.1.6",
        "termcolor-1.4.1",
        "ignore-0.4.33",
        "grep-0.3.2",
        "15.2.0",
        "15.1.0",
        "14.1.1",
        "13.0.0",
        "0.10.0",
    ];
    mount_json(
        &ordered,
        "/api/v3/repos/a/b/git/matching-refs/tags",
        200,
        json!(
            names
                .iter()
                .map(|name| json!({"ref": format!("refs/tags/{name}"),
                "object": {"sha": SHA, "type": "commit"}}))
                .collect::<Vec<_>>()
        ),
    )
    .await;
    let first = list(
        &ordered,
        json!({"owner":"a","repo":"b","operation":"refs","pageSize":4}),
    )
    .await
    .expect("refs");
    assert_eq!(
        first.data["tags"],
        json!(["15.2.0", "15.1.0", "14.1.1", "13.0.0"]),
        "{}",
        first.data
    );
    assert_eq!(first.data["pagination"]["hasMore"], true, "{}", first.data);
    let next = first
        .data
        .pointer("/next/nextPage/query/queries/0")
        .cloned()
        .expect("next page");
    let second = list(&ordered, next).await.expect("refs page 2");
    assert_eq!(
        second.data["tags"],
        json!(["termcolor-1.4.1", "0.10.0", "ignore-0.4.33", "grep-0.3.2"]),
        "{}",
        second.data
    );

    let fallback = server().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/git/matching-refs/tags"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&fallback)
        .await;
    mount_json(
        &fallback,
        "/api/v3/repos/a/b/tags",
        200,
        json!([{"name":"v2","commit":{"sha":SHA}},{"name":"v10","commit":{"sha":SHA}}]),
    )
    .await;
    let out = list(
        &fallback,
        json!({"owner":"a","repo":"b","operation":"refs"}),
    )
    .await
    .expect("refs from GitHub's page");
    assert_eq!(out.data["tags"], json!(["v2", "v10"]), "{}", out.data);
    assert!(
        out.data["warnings"].to_string().contains("GitHub's order"),
        "{}",
        out.data
    );
}

/// The branch-listing lead keeps the caller's extras and adds
/// `details` (the extra that lists branches); it never replaces them.
#[tokio::test]
async fn the_branch_listing_lead_adds_details_to_the_callers_extras() {
    let server = server().await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/git/matching-refs/tags",
        200,
        json!([{"ref":"refs/tags/v1","object":{"sha":SHA}}]),
    )
    .await;
    for (extras, expected) in [
        (json!(["hints"]), json!(["hints", "details"])),
        (json!([]), json!(["details"])),
        (
            json!(["stats", "hints"]),
            json!(["stats", "hints", "details"]),
        ),
    ] {
        let out = list(
            &server,
            json!({"owner":"a","repo":"b","operation":"refs","extras":extras}),
        )
        .await
        .expect("refs");
        assert_eq!(
            out.data
                .pointer("/next/viewRefs/query/queries/0/extras")
                .cloned(),
            Some(expected),
            "{}",
            out.data
        );
    }
}

/// A paged refs listing: a clamped `pageSize` warns once and its next
/// page continues at the size read; a later page's `viewRefs` lead
/// lists branches from page 1; the last page still states
/// `pagination`; a page past the end offers no lead.
#[tokio::test]
async fn refs_pages_continue_at_the_read_size_and_lead_from_page_one() {
    let server = server().await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/git/matching-refs/tags",
        200,
        json!(
            (1..=101)
                .map(|n| json!({"ref": format!("refs/tags/v{n}"), "object": {"sha": SHA}}))
                .collect::<Vec<_>>()
        ),
    )
    .await;
    let refs = json!({"owner":"a","repo":"b","operation":"refs"});
    let mut first = refs.clone();
    first["pageSize"] = json!(150);
    let first = list(&server, first).await.expect("refs page 1");
    assert!(
        first.data["warnings"].to_string().contains("pageSize 150"),
        "{}",
        first.data
    );
    let next = first
        .data
        .pointer("/next/nextPage/query/queries/0")
        .cloned()
        .expect("next page");
    assert_eq!(
        (&next["page"], &next["pageSize"]),
        (&json!(2), &json!(100)),
        "{next}"
    );

    let second = list(&server, next).await.expect("refs page 2");
    // Newest first: the 101st tag (the oldest) is page 2's one tag.
    assert_eq!(second.data["tags"], json!(["v1"]), "{}", second.data);
    assert!(second.data.get("warnings").is_none(), "{}", second.data);
    assert_eq!(
        second.data["pagination"]["currentPage"], 2,
        "{}",
        second.data
    );
    assert_eq!(
        second.data["pagination"]["hasMore"], false,
        "{}",
        second.data
    );
    let view = second
        .data
        .pointer("/next/viewRefs/query/queries/0")
        .expect("viewRefs");
    assert!(view.get("page").is_none(), "{view}");
    assert_eq!(view["pageSize"], 100, "{view}");

    let mut past = refs.clone();
    past["page"] = json!(50);
    let past = list(&server, past).await.expect("refs page 50");
    assert_eq!(past.data["pagination"]["outOfRange"], true, "{}", past.data);
    assert!(
        past.data.pointer("/next/viewRefs").is_none(),
        "{}",
        past.data
    );
}
