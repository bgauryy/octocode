//! securityCheck's GitHub reads: a commit and where it lives, and a repository's
//! advisory pages, each against GitHub's own answers and refusals.
use super::*;
use serde_json::json;

const SHA: &str = "4f0b76e2eca13de1c1fe8b4305abc1f7d63f4b86";

fn context() -> RequestContext {
    RequestContext::with_timeout(Duration::from_secs(5), 1 << 20)
}

async fn transport(server: &MockServer) -> GitHubTransport {
    executor_transport(server, GitHubBudget::relaxed(), short_retry()).0
}

async fn mount(server: &MockServer, route: &str, response: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/o/r/{route}")))
        .respond_with(response)
        .mount(server)
        .await;
}

fn json_ok(body: serde_json::Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(body)
}

#[tokio::test]
async fn a_commit_reads_as_its_signature_and_a_hash_github_cannot_resolve_as_none() {
    let server = MockServer::builder().start().await;
    mount(
        &server,
        &format!("commits/{SHA}"),
        json_ok(json!({"commit": {"verification": {"verified": true, "reason": "valid"}}})),
    )
    .await;
    mount(&server, "commits/unsigned", json_ok(json!({"commit": {}}))).await;
    mount(
        &server,
        "commits/unknown",
        ResponseTemplate::new(422).set_body_json(json!({"message": "No commit found for SHA"})),
    )
    .await;
    mount(
        &server,
        "commits/gone",
        ResponseTemplate::new(404).set_body_json(json!({"message": "Not Found"})),
    )
    .await;
    let github = transport(&server).await;
    let signed = github
        .commit_read("o", "r", SHA, &context())
        .await
        .expect("read")
        .expect("found");
    assert!(signed.verified);
    assert_eq!(signed.verification_reason.as_deref(), Some("valid"));
    let bare = github
        .commit_read("o", "r", "unsigned", &context())
        .await
        .expect("read")
        .expect("found");
    assert!(!bare.verified && bare.verification_reason.is_none());
    assert_eq!(
        github
            .commit_read("o", "r", "unknown", &context())
            .await
            .expect("read"),
        None,
        "422 is a commit the repository lacks"
    );
    let error = github
        .commit_read("o", "r", "gone", &context())
        .await
        .expect_err("a missing repository");
    assert_eq!(error.reason, Some(ProviderErrorReason::RepositoryNotFound));
}

#[tokio::test]
async fn branch_heads_and_pull_numbers_follow_every_linked_page() {
    let server = MockServer::builder().start().await;
    for (route, pages) in [
        ("branches-where-head", ["main", "next"]),
        ("pulls", ["1", "2"]),
    ] {
        for (index, name) in pages.iter().enumerate() {
            let body = if route == "pulls" {
                json!([{"number": name.parse::<u64>().expect("number")}])
            } else {
                json!([{"name": name}])
            };
            let mut response = json_ok(body);
            if index == 0 {
                let next = format!(
                    "{}/api/v3/repos/o/r/commits/{SHA}/{route}?page=2",
                    server.uri()
                );
                response = response.insert_header("link", format!("<{next}>; rel=\"next\""));
            }
            Mock::given(method("GET"))
                .and(path(format!("/api/v3/repos/o/r/commits/{SHA}/{route}")))
                .and(query_param("page", (index + 1).to_string()))
                .respond_with(response)
                .expect(1)
                .mount(&server)
                .await;
        }
    }
    let github = transport(&server).await;
    assert_eq!(
        github
            .commit_branch_heads("o", "r", SHA, &context())
            .await
            .expect("heads"),
        ["main", "next"]
    );
    assert_eq!(
        github
            .commit_pull_numbers("o", "r", SHA, &context())
            .await
            .expect("pulls"),
        [1, 2]
    );
}

#[tokio::test]
async fn tags_at_a_commit_are_those_whose_peeled_hash_matches_in_any_letter_case() {
    use crate::tags_pointing_at;
    let server = MockServer::builder().start().await;
    mount(
        &server,
        "tags",
        json_ok(json!([
            {"name": "4.18.1", "commit": {"sha": SHA.to_uppercase()}},
            {"name": "4.17.0", "commit": {"sha": "0".repeat(40)}},
        ])),
    )
    .await;
    let tips = transport(&server)
        .await
        .tag_tips("o", "r", &context())
        .await
        .expect("tags");
    assert_eq!(tips.len(), 2);
    assert_eq!(tags_pointing_at(&tips, SHA), ["4.18.1"]);
}

#[tokio::test]
async fn a_commit_is_on_a_branch_when_the_compare_says_identical_or_behind() {
    let server = MockServer::builder().start().await;
    for (base, status) in [
        ("same", "identical"),
        ("older", "behind"),
        ("newer", "ahead"),
        ("split", "diverged"),
    ] {
        mount(
            &server,
            &format!("compare/{base}...{SHA}"),
            json_ok(json!({"status": status})),
        )
        .await;
    }
    mount(
        &server,
        &format!("compare/unrelated...{SHA}"),
        ResponseTemplate::new(404).set_body_json(json!({"message": "Not Found"})),
    )
    .await;
    let github = transport(&server).await;
    for (base, on) in [
        ("same", true),
        ("older", true),
        ("newer", false),
        ("split", false),
        ("unrelated", false),
    ] {
        assert_eq!(
            github
                .commit_is_on("o", "r", base, SHA, &context())
                .await
                .expect(base),
            on,
            "{base}"
        );
    }
}

fn held(holders: &[(RefKind, &str)], complete: bool) -> RefHolders {
    RefHolders {
        holders: holders
            .iter()
            .map(|(kind, name)| RefHolder {
                kind: *kind,
                name: (*name).to_owned(),
            })
            .collect(),
        complete,
        failed: None,
    }
}

fn tips(rows: &[(&str, &str)]) -> Vec<NamedRef> {
    rows.iter()
        .map(|(name, sha)| NamedRef {
            name: (*name).to_owned(),
            sha: (*sha).to_owned(),
        })
        .collect()
}

fn branches(rows: &[(&str, &str)]) -> ResponseTemplate {
    json_ok(json!(
        rows.iter()
            .map(|(name, head)| json!({"name": name, "commit": {"sha": head}}))
            .collect::<Vec<_>>()
    ))
}

#[tokio::test]
async fn another_branch_that_holds_the_commit_is_named_and_the_default_is_not_compared() {
    let server = MockServer::builder().start().await;
    mount(
        &server,
        "branches",
        branches(&[("main", "a1"), ("feature/x", "b2"), ("release/4.17", "c3")]),
    )
    .await;
    mount(
        &server,
        &format!("compare/b2...{SHA}"),
        json_ok(json!({"status": "diverged"})),
    )
    .await;
    mount(
        &server,
        &format!("compare/c3...{SHA}"),
        json_ok(json!({"status": "behind"})),
    )
    .await;
    // The default branch was compared by the caller: asking again is a defect.
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/o/r/compare/a1...{SHA}")))
        .respond_with(json_ok(json!({"status": "behind"})))
        .expect(0)
        .mount(&server)
        .await;
    let found = transport(&server)
        .await
        .commit_in_other_ref(("o", "r"), "main", &[], SHA, &context())
        .await
        .expect("read");
    assert_eq!(found, held(&[(RefKind::Branches, "release/4.17")], true));
}

/// Two branches hold it: both are named, in name order, whichever comparison
/// answers first; tags left uncompared make the list incomplete.
#[tokio::test]
async fn every_branch_of_the_page_that_holds_the_commit_is_named_in_name_order() {
    let server = MockServer::builder().start().await;
    mount(
        &server,
        "branches",
        branches(&[
            ("main", "a1"),
            ("zeta", "b2"),
            ("alpha", "c3"),
            ("mid", "d4"),
        ]),
    )
    .await;
    for (tip, status) in [("b2", "behind"), ("c3", "identical"), ("d4", "ahead")] {
        mount(
            &server,
            &format!("compare/{tip}...{SHA}"),
            json_ok(json!({"status": status})),
        )
        .await;
    }
    let github = transport(&server).await;
    let found = github
        .commit_in_other_ref(("o", "r"), "main", &tips(&[("v1", "e5")]), SHA, &context())
        .await
        .expect("read");
    assert_eq!(
        found,
        held(
            &[(RefKind::Branches, "alpha"), (RefKind::Branches, "zeta")],
            false
        ),
        "the slow holder is named too, and the tag left uncompared says it is not whole"
    );
}

/// The tag pages are listed once by the caller: the walk reads none.
#[tokio::test]
async fn the_tag_pages_are_listed_once_and_the_walk_reuses_them() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/tags"))
        .respond_with(json_ok(json!([{"name": "v1", "commit": {"sha": "c3"}}])))
        .expect(1)
        .mount(&server)
        .await;
    mount(&server, "branches", branches(&[("main", "a1")])).await;
    mount(
        &server,
        &format!("compare/c3...{SHA}"),
        json_ok(json!({"status": "behind"})),
    )
    .await;
    let github = transport(&server).await;
    let listed = github.tag_tips("o", "r", &context()).await.expect("tags");
    let found = github
        .commit_in_other_ref(("o", "r"), "main", &listed, SHA, &context())
        .await
        .expect("read");
    assert_eq!(found, held(&[(RefKind::Tags, "v1")], true));
}

#[tokio::test]
async fn a_tag_whose_history_holds_the_commit_holds_it_when_no_branch_does() {
    let server = MockServer::builder().start().await;
    mount(
        &server,
        "branches",
        branches(&[("main", "a1"), ("gone", "b2")]),
    )
    .await;
    for (tip, status) in [
        ("b2", "ahead"),
        ("c3", "diverged"),
        ("d4", "behind"),
        ("e5", "identical"),
    ] {
        mount(
            &server,
            &format!("compare/{tip}...{SHA}"),
            json_ok(json!({"status": status})),
        )
        .await;
    }
    let tags = tips(&[("v1.2.1", "e5"), ("v1.1.0", "c3"), ("v1.2.0", "d4")]);
    let found = transport(&server)
        .await
        .commit_in_other_ref(("o", "r"), "main", &tags, SHA, &context())
        .await
        .expect("read");
    assert_eq!(
        found,
        held(
            &[(RefKind::Tags, "v1.2.0"), (RefKind::Tags, "v1.2.1")],
            true
        ),
        "both tags are named, in name order"
    );
}

#[tokio::test]
async fn no_other_branch_holding_the_commit_is_proved_only_by_every_branch_compared() {
    let server = MockServer::builder().start().await;
    mount(
        &server,
        "branches",
        branches(&[("main", "a1"), ("feature", "b2"), ("old", "c3")]),
    )
    .await;
    mount(
        &server,
        &format!("compare/b2...{SHA}"),
        json_ok(json!({"status": "ahead"})),
    )
    .await;
    mount(
        &server,
        &format!("compare/c3...{SHA}"),
        json_ok(json!({"status": "diverged"})),
    )
    .await;
    let github = transport(&server).await;
    assert_eq!(
        github
            .commit_in_other_ref(("o", "r"), "main", &[], SHA, &context())
            .await
            .expect("read"),
        held(&[], true)
    );
    // A comparison GitHub refused leaves it unproven: an error, never `None`.
    let refused = MockServer::builder().start().await;
    mount(
        &refused,
        "branches",
        branches(&[("main", "a1"), ("feature", "b2"), ("old", "c3")]),
    )
    .await;
    mount(
        &refused,
        &format!("compare/b2...{SHA}"),
        json_ok(json!({"status": "ahead"})),
    )
    .await;
    mount(
        &refused,
        &format!("compare/c3...{SHA}"),
        ResponseTemplate::new(500),
    )
    .await;
    transport(&refused)
        .await
        .commit_in_other_ref(("o", "r"), "main", &[], SHA, &context())
        .await
        .expect_err("not proved");
}

#[tokio::test]
async fn a_tag_comparison_that_fails_leaves_the_commit_unproven_unless_another_ref_holds_it() {
    let server = MockServer::builder().start().await;
    let tags = tips(&[("v1", "c3"), ("v2", "d4")]);
    mount(&server, "branches", branches(&[("main", "a1")])).await;
    mount(
        &server,
        &format!("compare/c3...{SHA}"),
        ResponseTemplate::new(500),
    )
    .await;
    mount(
        &server,
        &format!("compare/d4...{SHA}"),
        json_ok(json!({"status": "ahead"})),
    )
    .await;
    let github = transport(&server).await;
    github
        .commit_in_other_ref(("o", "r"), "main", &tags, SHA, &context())
        .await
        .expect_err("a refused tag comparison is no proof of absence");
    let holding = MockServer::builder().start().await;
    mount(&holding, "branches", branches(&[("main", "a1")])).await;
    mount(
        &holding,
        &format!("compare/c3...{SHA}"),
        ResponseTemplate::new(500),
    )
    .await;
    mount(
        &holding,
        &format!("compare/d4...{SHA}"),
        json_ok(json!({"status": "identical"})),
    )
    .await;
    let found = transport(&holding)
        .await
        .commit_in_other_ref(("o", "r"), "main", &tags, SHA, &context())
        .await
        .expect("a holder proves it");
    assert_eq!(
        (found.holders.clone(), found.complete),
        (held(&[(RefKind::Tags, "v2")], false).holders, false),
        "the failed comparison says the list may be short"
    );
    assert_eq!(
        found.failed.map(|error| error.status),
        Some(Some(500)),
        "and names the comparison that failed"
    );
}

#[tokio::test]
async fn advisory_pages_name_their_next_cursor_and_a_link_without_one_is_a_decode_failure() {
    let server = MockServer::builder().start().await;
    let advisory = json!([{"ghsa_id": "GHSA-aaaa-bbbb-cccc", "cve_id": "CVE-2026-0001"}]);
    let link = |target: &str| {
        format!(
            "<{}/api/v3/repos/o/r/security-advisories?{target}>; rel=\"next\"",
            server.uri()
        )
    };
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/security-advisories"))
        .and(query_param("state", "published"))
        .and(query_param("after", "c1"))
        .respond_with(json_ok(advisory.clone()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/security-advisories"))
        .and(query_param("per_page", "100"))
        .respond_with(json_ok(advisory.clone()).insert_header("link", link("after=c1")))
        .mount(&server)
        .await;
    let github = transport(&server).await;
    let first = github
        .repository_advisories_page("o", "r", None, &context())
        .await
        .expect("first page");
    assert_eq!(first.next.as_deref(), Some("c1"));
    assert_eq!(first.advisories[0].ghsa_id, "GHSA-aaaa-bbbb-cccc");
    let last = github
        .repository_advisories_page("o", "r", Some("c1"), &context())
        .await
        .expect("last page");
    assert_eq!(last.next, None);

    let broken = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/security-advisories"))
        .respond_with(json_ok(advisory).insert_header(
            "link",
            format!(
                "<{}/api/v3/repos/o/r/security-advisories?page=2>; rel=\"next\"",
                broken.uri()
            ),
        ))
        .mount(&broken)
        .await;
    let error = transport(&broken)
        .await
        .repository_advisories_page("o", "r", None, &context())
        .await
        .expect_err("no cursor");
    assert_eq!(error.kind, ProviderErrorKind::Decode);
}

#[tokio::test]
async fn advisories_of_a_missing_repository_are_repository_not_found() {
    let server = MockServer::builder().start().await;
    mount(
        &server,
        "security-advisories",
        ResponseTemplate::new(404).set_body_json(json!({"message": "Not Found"})),
    )
    .await;
    let error = transport(&server)
        .await
        .repository_advisories_page("o", "r", None, &context())
        .await
        .expect_err("missing");
    assert_eq!(error.reason, Some(ProviderErrorReason::RepositoryNotFound));
}

/// The failure a page keeps is the first by the page's order, however the
/// comparisons finish: the row's errorCode and why do not vary run to run.
#[test]
fn the_failure_kept_is_the_first_by_ref_order_not_by_arrival() {
    use crate::security::settle;
    let refused = |message: &str| ProviderError::new(ProviderErrorKind::Transport, message);
    let arrived = vec![
        (2, "zeta".to_owned(), Err(refused("third ref"))),
        (1, "mid".to_owned(), Ok(true)),
        (0, "alpha".to_owned(), Err(refused("first ref"))),
    ];
    let (mut found, mut failed) = (Vec::new(), None);
    settle(RefKind::Branches, arrived, (&mut found, &mut failed));
    assert_eq!(
        failed.map(|error| error.message.to_string()),
        Some("first ref".to_owned())
    );
    assert_eq!(
        found,
        [RefHolder {
            kind: RefKind::Branches,
            name: "mid".into()
        }]
    );
}
