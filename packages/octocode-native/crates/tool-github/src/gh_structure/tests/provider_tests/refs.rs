//! Refs reads: a missing repository, a named ref, tag order, branch leads and refs paging.
use super::*;
use octocode_test_support::json_fixture;

/// A transient failure cannot change the tag ordering used by an emitted
/// cursor. Otherwise a GitHub-order first page followed by a version-order
/// page silently duplicates some tags and loses others.
#[tokio::test]
async fn tag_listing_failure_never_emits_an_incompatible_order_cursor() {
    use octocode_tools_shared::gh_shared::test_support::one_shot_provider;

    let server = server().await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/tags",
        200,
        json!([{"name":"v2","commit":{"sha":SHA}}]),
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/git/matching-refs/tags"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let provider = one_shot_provider(&server);
    let home = tempfile::tempdir().expect("home");
    let result = execute(
        &provider,
        &query(json_fixture(
            r#"{
                "operation": "refs",
                "owner": "a",
                "repo": "b",
                "refKinds": ["tags"],
                "limit": 1
            }"#,
        )),
        &default_context(),
        home.path(),
        None,
    )
    .await;
    assert!(
        matches!(result, Err(ref error) if error.kind == ProviderErrorKind::Server),
        "a tag ordering failure must remain a typed failure, not a cursor over another order"
    );
}

/// A repository that does not resolve (a typo) leads to a search for
/// its name, as the search tools' failures do.
#[tokio::test]
async fn a_missing_repository_leads_to_a_repository_search() {
    let server = MockServer::builder().start().await;
    for route in ["/api/v3/repos/a/typo", "/api/v3/repos/a/typo/commits/HEAD"] {
        mount_json(
            &server,
            route,
            404,
            json_fixture(r#"{"message": "Not Found"}"#),
        )
        .await;
    }
    let home = tempfile::tempdir().expect("home");
    let provider = mock_provider(&server, RetryPolicy::default());
    let context = default_context();
    let failure = run(
        &provider,
        &query(json_fixture(
            r#"{"operation": "tree", "owner": "a", "repo": "typo"}"#,
        )),
        Ok(&context),
        home.path(),
        None,
    )
    .await
    .expect_err("missing repository");
    let lead = &failure.calls.as_ref().expect("next")["findRepository"];
    assert_eq!(lead["tool"], "ghSearchRepo", "{lead}");
    assert_eq!(
        lead["input"]["queries"][0]["keywords"],
        json_fixture(r#"["typo"]"#),
        "{lead}"
    );
    // A misspelled owner is not found by the name: the owner's own
    // repositories are the other lead (that listing names a missing login).
    let owner = &failure.calls.as_ref().expect("next")["findRepository2"];
    assert_eq!(owner["tool"], "ghSearchRepo", "{owner}");
    assert_eq!(
        owner["input"]["queries"][0],
        json!({"operation":"list","owner":"a","limit":octocode_tools_shared::gh_shared::RECOVERY_LIMIT}),
        "{owner}"
    );
    // The name search lists repositories named like it under any owner:
    // the shared hint labels both leads.
    assert_eq!(
        failure.hints,
        [octocode_tools_shared::gh_shared::REPOSITORY_SEARCH_HINT.to_owned()]
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
                None => {
                    mount_json(
                        &server,
                        route,
                        404,
                        json_fixture(r#"{"message": "Not Found"}"#),
                    )
                    .await;
                }
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
/// set that cannot be read whole fails without changing the ordering.
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
        json_fixture(r#"{"owner": "a", "repo": "b", "operation": "refs"}"#),
    )
    .await
    .expect("refs");
    assert_eq!(
        tag_names(&first.data),
        [
            "15.2.0",
            "15.1.0",
            "14.1.1",
            "13.0.0",
            "0.10.0",
            "termcolor-1.4.1",
            "ignore-0.4.33",
            "grep-0.3.2",
            "wincolor-0.1.6"
        ],
        "{}",
        first.data
    );
    assert!(first.data.pointer("/next/nextPage").is_none());
    assert!(first.data["branches"].is_array());

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
    let error = list(
        &fallback,
        json_fixture(r#"{"owner": "a", "repo": "b", "operation": "refs"}"#),
    )
    .await
    .expect_err("the failed whole-list read cannot substitute another order");
    assert_eq!(error.kind, ProviderErrorKind::Server);
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
            (1..=31)
                .map(|n| json!({"ref": format!("refs/tags/v{n}"), "object": {"sha": SHA}}))
                .collect::<Vec<_>>()
        ),
    )
    .await;
    let refs = json_fixture(r#"{"owner": "a", "repo": "b", "operation": "refs"}"#);
    let first = list(&server, refs.clone()).await.expect("refs page 1");
    let next = first
        .data
        .pointer("/next/nextPage/input/queries/0")
        .cloned()
        .expect("next page");
    assert_eq!(next["page"], 2);
    assert!(next.get("pageSize").is_none());

    let second = list(&server, next).await.expect("refs page 2");
    // Newest first: the 31st tag (the oldest) is page 2's one tag.
    assert_eq!(
        second.data["tags"],
        json!([format!("v1 {SHA}")]),
        "{}",
        second.data
    );
    assert!(second.data.get("warnings").is_none(), "{}", second.data);
    assert_eq!(second.data["pagination"]["offset"], 30, "{}", second.data);
    assert_eq!(
        second.data["pagination"]["hasMore"], false,
        "{}",
        second.data
    );

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

/// Mount `names` as repository `a/b`'s whole tag set.
async fn tags_server(names: &[&str]) -> MockServer {
    let server = server().await;
    mount_json(
        &server,
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
    server
}

/// The tags a listing of `a/b` shows, every page walked.
async fn listed_tags(server: &MockServer, limit: u64) -> Vec<String> {
    let mut listed = Vec::new();
    let mut request =
        json!({"owner":"a","repo":"b","operation":"refs","refKinds":["tags"],"limit":limit});
    loop {
        let out = list(server, request).await.expect("refs");
        listed.extend(tag_names(&out.data));
        match out.data.pointer("/next/nextPage/input/queries/0") {
            Some(next) => request = next.clone(),
            None => return listed,
        }
    }
}

/// A monorepo whose release tags carry package names: the tags naming the
/// repository itself (no prefix, or a package named like the repository,
/// scoped or not) lead every other package's, highest version first, so
/// page 1 answers "the repository's latest release" whatever numbers its
/// packages reached. An old unprefixed line does not outrank the
/// repository's current package line. Every tag is listed exactly once.
#[tokio::test]
async fn the_repositorys_own_release_line_leads_a_monorepos_tags() {
    // (the repository's own tags, highest first; every other tag)
    let cases: [(&[&str], &[&str]); 2] = [
        (
            &["@scope/b@3.2.0", "@scope/b@3.2.0-next.1", "@scope/b@2.70.4"],
            &[
                "@scope/adapter-cloud@8.0.2",
                "@scope/adapter-cloud@8.0.1",
                "create-b@7.0.0",
                "@scope/adapter-node@6.0.2",
            ],
        ),
        (
            &["b@5.1.0", "b@4.0.0", "v0.0.12", "v0.0.4"],
            &[
                "@scope/cloudflare@15.0.0",
                "@scope/vue@7.1.5",
                "b-plugin@9.0.0",
            ],
        ),
    ];
    for (own, others) in cases {
        let mut names = others.iter().chain(own).copied().collect::<Vec<_>>();
        names.reverse();
        let server = tags_server(&names).await;
        let first = list(
            &server,
            json_fixture(
                r#"{
                    "owner": "a",
                    "repo": "b",
                    "operation": "refs",
                    "refKinds": ["tags"],
                    "limit": 1
                }"#,
            ),
        )
        .await
        .expect("refs");
        assert_eq!(tag_names(&first.data), [own[0]], "{}", first.data);
        let listed = listed_tags(&server, 2).await;
        assert_eq!(&listed[..own.len()], own, "{listed:?}");
        let mut rest = listed[own.len()..].to_vec();
        rest.sort_unstable();
        let mut expected = others
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>();
        expected.sort_unstable();
        assert_eq!(rest, expected, "every other tag once: {listed:?}");
    }
}

/// A listing names no ref, so it carries no top-level `commitSha`: one
/// there would read as the commit of the tags or branches it lists. Each
/// branch row carries its own head; a tag's commit is a `ref` read.
#[tokio::test]
async fn a_refs_listing_states_no_commit_it_did_not_resolve() {
    let server = tags_server(&["v1.0.0"]).await;
    for kinds in [
        json_fixture(r#"["tags"]"#),
        json_fixture(r#"["branches"]"#),
        json_fixture(r#"["tags", "branches"]"#),
    ] {
        let out = list(
            &server,
            json!({"owner":"a","repo":"b","operation":"refs","refKinds":kinds}),
        )
        .await
        .expect("refs");
        assert!(out.data.get("commitSha").is_none(), "{kinds}: {}", out.data);
        assert_eq!(out.data["defaultBranch"], "main", "{kinds}: {}", out.data);
    }
}

/// A glob `ref` names no ref (git refuses `*`, `?` and `[` in one): it
/// selects the listed tags and branches whose name it matches, so a release
/// line is one call, in the listing's own order.
#[tokio::test]
async fn a_glob_ref_lists_only_the_matching_tags_and_branches() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/HEAD"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SHA))
        .mount(&server)
        .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b",
        200,
        json_fixture(r#"{"default_branch": "main", "full_name": "a/b"}"#),
    )
    .await;
    let tags = [
        "v3.9.0",
        "v4.0.0",
        "v4.1.0-rc.1",
        "v4.1.0",
        "v5.0.0",
        "v40.0.0",
    ];
    mount_matching_refs(&server, &tags).await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/branches",
        200,
        json!(
            ["main", "release/4.x", "release/5.x"]
                .iter()
                .map(|name| json!({"name": name, "commit": {"sha": SHA}}))
                .collect::<Vec<_>>()
        ),
    )
    .await;
    let line = list(
        &server,
        json_fixture(
            r#"{
                "owner": "a",
                "repo": "b",
                "operation": "refs",
                "ref": "v4.*"
            }"#,
        ),
    )
    .await
    .expect("refs");
    assert_eq!(
        tag_names(&line.data),
        ["v4.1.0", "v4.1.0-rc.1", "v4.0.0"],
        "{}",
        line.data
    );
    assert_eq!(line.data["branches"], json!([]), "{}", line.data);
    assert!(line.data.get("commitSha").is_none(), "{}", line.data);
    // The glob's literal start is GitHub's to match: only refs under it are read.
    let asked = server.received_requests().await.expect("requests");
    let tag_reads = asked
        .iter()
        .filter(|request| request.url.path().contains("/matching-refs/tags"))
        .map(|request| request.url.path().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        tag_reads,
        ["/api/v3/repos/a/b/git/matching-refs/tags/v4."],
        "{asked:?}"
    );
    let branches = list(
        &server,
        json_fixture(
            r#"{
                "owner": "a",
                "repo": "b",
                "operation": "refs",
                "ref": "release/*",
                "limit": 1
            }"#,
        ),
    )
    .await
    .expect("refs");
    assert_eq!(
        branches.data["branches"],
        json!([format!("release/4.x {SHA}")]),
        "{}",
        branches.data
    );
    assert_eq!(branches.data["tags"], json!([]), "{}", branches.data);
    assert!(
        branches.data.pointer("/next/nextPage").is_some(),
        "the second match is one page on: {}",
        branches.data
    );
}

/// Mount `names` as repository `a/b`'s lightweight tags, answered as GitHub
/// answers `matching-refs/tags/<prefix>`: the refs whose name starts with
/// the route's prefix.
async fn mount_matching_refs(server: &MockServer, names: &[&str]) {
    let names = names
        .iter()
        .map(|name| (*name).to_owned())
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(wiremock::matchers::path_regex(
            "^/api/v3/repos/a/b/git/matching-refs/tags(/.*)?$",
        ))
        .respond_with(move |request: &wiremock::Request| {
            let prefix = request
                .url
                .path()
                .strip_prefix("/api/v3/repos/a/b/git/matching-refs/tags")
                .map(|rest| rest.trim_start_matches('/'))
                .unwrap_or_default();
            ResponseTemplate::new(200).set_body_json(
                names
                    .iter()
                    .filter(|name| name.starts_with(prefix))
                    .map(|name| {
                        json!({"ref": format!("refs/tags/{name}"),
                            "object": {"sha": SHA, "type": "commit"}})
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .mount(server)
        .await;
}

/// Each tag row names the commit its tag points at: a lightweight tag's
/// as listed, an annotated tag's the commit its tag object targets, every
/// annotated tag on the page read in one GraphQL request, never one read
/// per tag.
#[tokio::test]
async fn tag_rows_name_the_commit_each_tag_points_at() {
    let server = server().await;
    let annotated = ["v3.0.0", "v2.0.0"];
    let object = |name: &str| format!("{:0>40}", name.replace('.', "").replace('v', "a"));
    let commit = |name: &str| format!("{:c>40}", name.replace(['.', 'v'], ""));
    let mut refs = annotated
        .iter()
        .map(|name| {
            json!({"ref": format!("refs/tags/{name}"),
                "object": {"sha": object(name), "type": "tag"}})
        })
        .collect::<Vec<_>>();
    refs.push(json!({"ref": "refs/tags/v1.0.0", "object": {"sha": SHA, "type": "commit"}}));
    mount_json(
        &server,
        "/api/v3/repos/a/b/git/matching-refs/tags",
        200,
        json!(refs),
    )
    .await;
    let targets = annotated
        .iter()
        .map(|name| (object(name), commit(name)))
        .collect::<Vec<_>>();
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(move |request: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&request.body).expect("graphql body");
            let mut repository = serde_json::Map::new();
            for (key, value) in body["variables"].as_object().expect("variables") {
                if let Some((_, target)) = targets.iter().find(|(object, _)| value == object) {
                    repository.insert(
                        key.clone(),
                        json!({"target": {"__typename": "Commit", "oid": target}}),
                    );
                }
            }
            ResponseTemplate::new(200).set_body_json(json!({"data": {"repository": repository}}))
        })
        .mount(&server)
        .await;
    let out = list(
        &server,
        json_fixture(
            r#"{
                "owner": "a",
                "repo": "b",
                "operation": "refs",
                "refKinds": ["tags"]
            }"#,
        ),
    )
    .await
    .expect("refs");
    let mut expected = annotated
        .iter()
        .map(|name| json!(format!("{name} {}", commit(name))))
        .collect::<Vec<_>>();
    expected.push(json!(format!("v1.0.0 {SHA}")));
    assert_eq!(out.data["tags"], json!(expected), "{}", out.data);
    let asked = server.received_requests().await.expect("requests");
    let graphql = asked
        .iter()
        .filter(|request| request.url.path() == "/api/graphql")
        .count();
    assert_eq!(graphql, 1, "one read for the page's annotated tags");
    assert!(
        !asked
            .iter()
            .any(|request| request.url.path().contains("/git/tags/")),
        "no read per tag: {asked:?}"
    );
}
