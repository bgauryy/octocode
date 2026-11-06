//! Release-tag candidates, current upstream checks, and repository status.
use super::{RegistryClient, github_release_tag, tag_candidates};

#[test]
fn a_monorepo_package_is_tagged_by_the_names_it_goes_by() {
    // pydantic-core is released from pydantic/pydantic as `core-v<version>`.
    let tags = tag_candidates("pydantic", "pydantic_core", Some("pydantic-core"), "2.50.1");
    assert_eq!(&tags[..2], ["v2.50.1", "2.50.1"]);
    assert!(tags.contains(&"core-v2.50.1".to_owned()), "{tags:?}");
    assert!(
        tags.contains(&"pydantic-core-v2.50.1".to_owned()),
        "{tags:?}"
    );
    // A scoped package keeps its scope in the `@` form changesets writes.
    let tags = tag_candidates("changesets", "@changesets/cli", None, "2.29.0");
    assert!(
        tags.contains(&"@changesets/cli@2.29.0".to_owned()),
        "{tags:?}"
    );
    assert!(tags.contains(&"cli@2.29.0".to_owned()), "{tags:?}");
    // A package that is its repository takes only the plain tags.
    assert_eq!(
        tag_candidates("requests", "requests", None, "2.32.3"),
        ["v2.32.3", "2.32.3"]
    );
    assert_eq!(
        tag_candidates("serde_json", "serde-json", None, "1.0.1"),
        ["v1.0.1", "1.0.1"]
    );
}

fn tag_client<'a>(
    http: &'a super::super::test_http::RouteHttp,
    budget: &'a octocode_io::budget::RequestBudget,
    tags: &'a super::super::test_http::KnownTags,
) -> RegistryClient<'a> {
    RegistryClient {
        tags: Some(tags),
        ..RegistryClient::new(http, budget)
    }
}

/// A package with no matching tag is checked again on each lookup.
#[tokio::test]
async fn each_release_lookup_checks_current_upstream_tags() {
    let http = super::super::test_http::RouteHttp::new();
    let budget = super::super::test_http::test_budget();
    let tags = super::super::test_http::KnownTags::always(Some(false));
    let client = tag_client(&http, &budget, &tags);
    let repository = "https://github.com/acme/monorepo";
    for _ in 0..2 {
        assert_eq!(
            github_release_tag(
                repository,
                Some("packages/widget"),
                "widget",
                "1.0.0",
                None,
                &client
            )
            .await,
            None
        );
    }
    let candidates = tag_candidates("monorepo", "widget", Some("packages/widget"), "1.0.0");
    assert_eq!(tags.seen().len(), candidates.len() * 2, "{:?}", tags.seen());
    // A failed check is not an absence: it is asked again.
    let failing = super::super::test_http::KnownTags::always(None);
    let client = tag_client(&http, &budget, &failing);
    for _ in 0..2 {
        assert_eq!(
            github_release_tag(repository, None, "other", "2.0.0", None, &client).await,
            None
        );
    }
    assert_eq!(failing.seen().len(), 4, "{:?}", failing.seen());
}

/// The plain tags are asked first; a monorepo package's own tags are
/// asked together after them, and the earliest candidate that exists
/// wins whatever order the answers arrive in.
#[tokio::test]
async fn release_tags_are_asked_in_two_rounds_and_the_first_candidate_wins() {
    let http = super::super::test_http::RouteHttp::new();
    let budget = super::super::test_http::test_budget();
    let plain = super::super::test_http::KnownTags::new(vec!["acme/widget@v1.0.0"]);
    let client = tag_client(&http, &budget, &plain);
    assert_eq!(
        github_release_tag(
            "https://github.com/acme/widget",
            Some("packages/core"),
            "core",
            "1.0.0",
            None,
            &client
        )
        .await
        .map(|release| release.tag)
        .as_deref(),
        Some("v1.0.0")
    );
    assert_eq!(plain.seen(), ["acme/widget@v1.0.0", "acme/widget@1.0.0"]);
    let stems = super::super::test_http::KnownTags::new(vec![
        "acme/widget@core@1.0.0",
        "acme/widget@core-v1.0.0",
    ]);
    let client = tag_client(&http, &budget, &stems);
    assert_eq!(
        github_release_tag(
            "https://github.com/acme/widget",
            Some("packages/core"),
            "core",
            "1.0.0",
            None,
            &client
        )
        .await
        .map(|release| release.tag)
        .as_deref(),
        Some("core-v1.0.0")
    );
    assert_eq!(
        stems.seen().len(),
        tag_candidates("widget", "core", Some("packages/core"), "1.0.0").len()
    );
}

fn unpinned(repository: &str) -> super::ArtifactItem {
    let mut item = super::ArtifactItem::new(
        octocode_contract_types::ArtifactType::Npm,
        "minimist".into(),
        String::new(),
    );
    item.repository = Some(repository.into());
    item.version = Some("1.2.6".into());
    item
}

/// No tag and a repository GitHub does not have (minimist's
/// `substack/minimist`): the row says the repository is gone and keeps no
/// ref; a redirect says it moved; a repository that is there says nothing.
#[tokio::test]
async fn a_repository_github_lacks_or_redirects_is_recorded_on_the_row() {
    use super::{CommitCheck, RepositoryStatus, pin_release_tag};
    let http = super::super::test_http::RouteHttp::new();
    let budget = super::super::test_http::test_budget();
    for (check, status) in [
        (
            CommitCheck::RepositoryMissing,
            Some(RepositoryStatus::Missing),
        ),
        (
            CommitCheck::FoundAfterRedirect,
            Some(RepositoryStatus::Moved),
        ),
        (CommitCheck::Found, None),
    ] {
        let tags = super::super::test_http::KnownTags::new(vec![]).with_repository(check);
        let client = tag_client(&http, &budget, &tags);
        let mut item = unpinned("https://github.com/substack/minimist");
        pin_release_tag(&mut item, "minimist", "1.2.6", None, &client).await;
        assert_eq!(item.repository_status, status, "{check:?}");
        assert_eq!(item.source_ref, None);
        assert_eq!(
            tags.seen().last().map(String::as_str),
            Some("substack/minimist@HEAD")
        );
    }
    // A tag that exists settles it: no further check.
    let tags = super::super::test_http::KnownTags::new(vec!["o/r@v1.2.6"])
        .with_repository(CommitCheck::RepositoryMissing);
    let client = tag_client(&http, &budget, &tags);
    let mut item = unpinned("https://github.com/o/r");
    pin_release_tag(&mut item, "minimist", "1.2.6", None, &client).await;
    assert_eq!(item.source_ref.as_deref(), Some("v1.2.6"));
    assert_eq!(item.repository_status, None);
}

/// A Go version's tag is its version with its `v`, below the module's directory
/// when it has one; `+incompatible` is not part of the release.
#[tokio::test]
async fn a_go_release_tag_is_the_version_under_the_module_directory() {
    use super::go_release_tag;
    let http = super::super::test_http::RouteHttp::new();
    let budget = super::super::test_http::test_budget();
    let tags = super::super::test_http::KnownTags::new(vec!["o/r@v3.0.0", "o/r@sub/v1.2.3"]);
    let client = tag_client(&http, &budget, &tags);
    let repository = "https://github.com/o/r";
    let found = |directory, version| {
        let client = &client;
        async move {
            go_release_tag(repository, directory, version, client)
                .await
                .map(|release| release.tag)
        }
    };
    assert_eq!(
        found(None, "v3.0.0+incompatible").await.as_deref(),
        Some("v3.0.0")
    );
    assert_eq!(found(Some(""), "v3.0.0").await.as_deref(), Some("v3.0.0"));
    assert_eq!(
        found(Some("sub"), "v1.2.3").await.as_deref(),
        Some("sub/v1.2.3")
    );
    // A subdirectory module is never pinned to the root's tag.
    assert_eq!(found(Some("other"), "v3.0.0").await, None);
    assert_eq!(
        tags.seen(),
        [
            "o/r@v3.0.0",
            "o/r@v3.0.0",
            "o/r@sub/v1.2.3",
            "o/r@other/v3.0.0"
        ]
    );
}
