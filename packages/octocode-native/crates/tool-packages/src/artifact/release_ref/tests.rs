//! Release-tag candidates, current upstream checks, and repository status.
use super::{Lookup, RegistryClient, TagSearch, github_release_tag, tag_candidates};

#[test]
fn a_monorepo_package_is_tagged_by_the_names_it_goes_by() {
    // pydantic-core is released from pydantic/pydantic as `core-v<version>`.
    let tags = tag_candidates("pydantic", "pydantic_core", Some("pydantic-core"), "2.50.1");
    assert_eq!(tags.plain, ["v2.50.1", "2.50.1"]);
    assert!(
        tags.own.contains(&"core-v2.50.1".to_owned()),
        "{:?}",
        tags.own
    );
    assert!(
        tags.own.contains(&"pydantic-core-v2.50.1".to_owned()),
        "{:?}",
        tags.own
    );
    // A workspace crate of ripgrep is tagged `<name>-<version>`.
    let tags = tag_candidates("ripgrep", "grep-searcher", None, "0.1.6");
    assert!(
        tags.own.contains(&"grep-searcher-0.1.6".to_owned()),
        "{:?}",
        tags.own
    );
    // A scoped package keeps its scope in the `@` form changesets writes.
    let tags = tag_candidates("changesets", "@changesets/cli", None, "2.29.0");
    assert!(
        tags.own.contains(&"@changesets/cli@2.29.0".to_owned()),
        "{:?}",
        tags.own
    );
    assert!(
        tags.own.contains(&"cli@2.29.0".to_owned()),
        "{:?}",
        tags.own
    );
    // A package that is its repository takes only the plain tags.
    for (repo, package, version) in [
        ("requests", "requests", "2.32.3"),
        ("serde_json", "serde-json", "1.0.1"),
    ] {
        let tags = tag_candidates(repo, package, None, version);
        assert!(tags.own.is_empty(), "{:?}", tags.own);
        assert_eq!(tags.plain, [format!("v{version}"), version.to_owned()]);
    }
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

/// A package with no matching tag is checked again on each lookup: every
/// candidate, then the repository's tag list, read whole.
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
                Lookup::default(),
                &client
            )
            .await,
            TagSearch::Unnamed {
                differing: Vec::new(),
                foreign: Vec::new()
            }
        );
    }
    let candidates = tag_candidates("monorepo", "widget", Some("packages/widget"), "1.0.0");
    assert_eq!(
        tags.seen().len(),
        (candidates.own.len() + candidates.plain.len() + 1) * 2,
        "{:?}",
        tags.seen()
    );
    assert_eq!(
        tags.seen().last().map(String::as_str),
        Some("acme/monorepo#tags")
    );
    // A failed check is not an absence: it is asked again, and the tag list
    // that could not be read says why.
    let failing = super::super::test_http::KnownTags::always(None);
    let client = tag_client(&http, &budget, &failing);
    for _ in 0..2 {
        assert_eq!(
            github_release_tag(
                repository,
                None,
                "other",
                "2.0.0",
                Lookup::default(),
                &client
            )
            .await,
            TagSearch::Unlisted {
                cause: super::CheckFailure::Request,
                shapes_checked: false
            }
        );
    }
    // The first round's checks (the package's own shapes) fail, then the
    // tag list is asked.
    let own = tag_candidates("monorepo", "other", None, "2.0.0").own;
    assert_eq!(
        failing.seen().len(),
        (own.len() + 1) * 2,
        "{:?}",
        failing.seen()
    );
}

/// A monorepo package with no registry commit asks its own tags first and
/// the plain ones after them; the earliest candidate of a round that exists
/// wins whatever order the answers arrive in.
#[tokio::test]
async fn a_monorepo_package_asks_its_own_tags_before_the_plain_ones() {
    let http = super::super::test_http::RouteHttp::new();
    let budget = super::super::test_http::test_budget();
    let candidates = tag_candidates("widget", "core", Some("packages/core"), "1.0.0");
    let plain = super::super::test_http::KnownTags::new(vec!["acme/widget@v1.0.0"]);
    let client = tag_client(&http, &budget, &plain);
    assert_eq!(
        github_release_tag(
            "https://github.com/acme/widget",
            Some("packages/core"),
            "core",
            "1.0.0",
            Lookup::default(),
            &client
        )
        .await
        .found()
        .map(|release| release.tag)
        .as_deref(),
        Some("v1.0.0")
    );
    let asked = candidates
        .own
        .iter()
        .chain(&candidates.plain)
        .map(|tag| format!("acme/widget@{tag}"))
        .collect::<Vec<_>>();
    assert_eq!(plain.seen(), asked);
    let stems = super::super::test_http::KnownTags::new(vec![
        "acme/widget@core@1.0.0",
        "acme/widget@core-v1.0.0",
        "acme/widget@v1.0.0",
    ]);
    let client = tag_client(&http, &budget, &stems);
    assert_eq!(
        github_release_tag(
            "https://github.com/acme/widget",
            Some("packages/core"),
            "core",
            "1.0.0",
            Lookup::default(),
            &client
        )
        .await
        .found()
        .map(|release| release.tag)
        .as_deref(),
        Some("core-v1.0.0")
    );
    assert_eq!(stems.seen().len(), candidates.own.len());
}

/// grep-searcher 0.1.6 lives in ripgrep, whose own `0.1.6` is another
/// release: with the registry's commit, the plain tag naming another commit
/// does not count and the crate's own `grep-searcher-0.1.6` proves the
/// commit (no "commit differs" warning); without it, the own tag is still
/// asked first and pinned.
#[tokio::test]
async fn a_workspace_crate_takes_its_own_tag_over_the_repository_release_of_its_number() {
    use super::{check_commit_against_tag, pin_release_tag};
    let http = super::super::test_http::RouteHttp::new();
    let budget = super::super::test_http::test_budget();
    let known = || {
        super::super::test_http::KnownTags::new(vec![
            "BurntSushi/ripgrep@0.1.6=9e2f10b",
            "BurntSushi/ripgrep@grep-searcher-0.1.6=0972c6e",
        ])
    };
    let tags = known();
    let client = tag_client(&http, &budget, &tags);
    let mut item = unpinned("https://github.com/BurntSushi/ripgrep");
    item.package_name = "grep-searcher".into();
    item.source_ref = Some("0972c6e".into());
    check_commit_against_tag(&mut item, "grep-searcher", "0.1.6", "0972c6e", &client).await;
    assert!(item.source_tag, "{:?}", tags.seen());
    assert_eq!(item.release_tag.as_deref(), Some("grep-searcher-0.1.6"));
    assert_eq!(item.tag_differs, None);
    assert!(item.disclosures.is_empty(), "{:?}", item.disclosures);
    // The plain tags (one naming another commit), then the own ones: no tag
    // list is read.
    assert!(
        !tags.seen().iter().any(|seen| seen.ends_with("#tags")),
        "{:?}",
        tags.seen()
    );
    let tags = known();
    let client = tag_client(&http, &budget, &tags);
    let mut item = unpinned("https://github.com/BurntSushi/ripgrep");
    item.package_name = "grep-searcher".into();
    pin_release_tag(&mut item, "grep-searcher", "0.1.6", None, &client).await;
    assert_eq!(item.source_ref.as_deref(), Some("0972c6e"));
    assert_eq!(item.release_tag.as_deref(), Some("grep-searcher-0.1.6"));
    assert_eq!(
        tags.seen().first().map(String::as_str),
        Some("BurntSushi/ripgrep@grep-searcher-v0.1.6"),
        "the own shapes are asked first: {:?}",
        tags.seen()
    );
}

/// With the registry's commit known, a plain tag naming it is the release
/// of a monorepo package too (a repository released in lockstep): its two
/// checks settle it. One naming another commit leaves the tag list to
/// decide, and stays the answer when the list cannot be read.
#[tokio::test]
async fn a_plain_tag_counts_for_a_monorepo_package_only_at_the_registry_commit() {
    let http = super::super::test_http::RouteHttp::new();
    let budget = super::super::test_http::test_budget();
    let lockstep = super::super::test_http::KnownTags::new(vec!["babel/babel@v7.24.0=abc"]);
    let client = tag_client(&http, &budget, &lockstep);
    let lookup = Lookup {
        declared: None,
        commit: Some("ABC"),
    };
    let search = github_release_tag(
        "https://github.com/babel/babel",
        Some("packages/babel-core"),
        "@babel/core",
        "7.24.0",
        lookup,
        &client,
    )
    .await;
    assert_eq!(
        search.found().map(|release| release.tag).as_deref(),
        Some("v7.24.0")
    );
    assert_eq!(lockstep.seen().len(), 2, "{:?}", lockstep.seen());
    let unread = super::super::test_http::KnownTags::new(vec!["o/mono@1.0.0=other"])
        .with_listing_failure(super::CheckFailure::Request);
    let client = tag_client(&http, &budget, &unread);
    let search = github_release_tag(
        "https://github.com/o/mono",
        None,
        "widget",
        "1.0.0",
        Lookup {
            declared: None,
            commit: Some("registry"),
        },
        &client,
    )
    .await;
    assert_eq!(
        search.found().map(|release| release.commit).as_deref(),
        Some("other"),
        "{:?}",
        unread.seen()
    );
    assert!(
        unread.seen().iter().any(|seen| seen == "o/mono#tags"),
        "{:?}",
        unread.seen()
    );
}

/// In the tag list, this package's name before a `-` is its own tag: ignore
/// 0.4.7's `ignore-0.4.7` wins over a repository release `0.4.7` at another
/// commit instead of the two being stated as ambiguous.
#[tokio::test]
async fn a_listed_tag_led_by_the_package_name_is_its_own() {
    use super::ReleaseTag;
    let tags = super::super::test_http::KnownTags::new(vec![
        "BurntSushi/ripgrep@0.4.7=aaa",
        "BurntSushi/ripgrep@ignore-0.4.7=bbb",
    ]);
    let own = super::listing::OwnNames::new("ripgrep", "ignore", None);
    assert_eq!(
        super::listing::listed_release_tag(&tags, "BurntSushi", "ripgrep", "0.4.7", &own, true)
            .await,
        TagSearch::Found(ReleaseTag {
            tag: "ignore-0.4.7".into(),
            commit: "bbb".into()
        })
    );
}

/// A release tag of a shape no candidate spells is found in the
/// repository's own tag list: whatever prefix the project's tags carry
/// (`rel/`, `release-`, `<name>-`, `release/v`), a tag naming the version
/// is its release. A tag of another version (`rel/11.4.0`, `rel/1.4.0-rc1`)
/// never is.
#[tokio::test]
async fn a_release_tag_of_any_prefix_is_found_in_the_repository_tag_list() {
    let http = super::super::test_http::RouteHttp::new();
    let budget = super::super::test_http::test_budget();
    for shape in [
        "rel/1.4.0",
        "release-1.4.0",
        "widget-1.4.0",
        "release/v1.4.0",
    ] {
        let named = format!("acme/monorepo@{shape}");
        let known: Vec<&'static str> = vec![
            "acme/monorepo@rel/11.4.0",
            "acme/monorepo@rel/1.4.0-rc1",
            "acme/monorepo@v1.3.9",
            Box::leak(named.into_boxed_str()),
        ];
        let tags = super::super::test_http::KnownTags::new(known);
        let client = tag_client(&http, &budget, &tags);
        let found = github_release_tag(
            "https://github.com/acme/monorepo",
            None,
            "widget-core",
            "1.4.0",
            Lookup::default(),
            &client,
        )
        .await
        .found();
        assert_eq!(
            found.map(|release| (release.tag, release.commit)),
            Some((shape.to_owned(), format!("commit-of-{shape}"))),
            "{shape}: {:?}",
            tags.seen()
        );
    }
}

/// Tags that name the version at one commit are that release, the shortest
/// pinned; tags naming it at different commits are listed, never chosen
/// between, and a pin says so on the row instead of pinning either.
#[tokio::test]
async fn listed_tags_name_one_release_or_are_stated_as_ambiguous() {
    use super::{ReleaseTag, pin_release_tag};
    let http = super::super::test_http::RouteHttp::new();
    let budget = super::super::test_http::test_budget();
    let same =
        super::super::test_http::KnownTags::new(vec!["o/r@release-1.2.6=abc", "o/r@rel/1.2.6=ABC"]);
    let client = tag_client(&http, &budget, &same);
    assert_eq!(
        github_release_tag(
            "https://github.com/o/r",
            None,
            "r",
            "1.2.6",
            Lookup::default(),
            &client
        )
        .await,
        TagSearch::Found(ReleaseTag {
            tag: "rel/1.2.6".into(),
            commit: "ABC".into()
        })
    );
    let split = super::super::test_http::KnownTags::new(vec![
        "o/r@rel/1.2.6=aaa",
        "o/r@release-1.2.6=bbb",
        "o/r@HEAD",
    ]);
    let client = tag_client(&http, &budget, &split);
    let mut item = unpinned("https://github.com/o/r");
    pin_release_tag(&mut item, "minimist", "1.2.6", Some("c@1.2.5"), &client).await;
    assert_eq!(item.source_ref, None, "{:?}", split.seen());
    assert!(!item.declared_ref);
    assert_eq!(
        item.disclosures,
        [
            "minimist: tags rel/1.2.6 (commit aaa), release-1.2.6 (commit bbb) each name version 1.2.6 at different commits, so none is taken as its release tag."
        ]
    );
    assert!(
        !split.seen().iter().any(|seen| seen == "o/r@c@1.2.5"),
        "the declared tag of another release is not asked as a fallback: {:?}",
        split.seen()
    );
}

/// A tag list that cannot be read is recorded with its cause, so the row's
/// warning claims no absence the lookup did not check.
#[tokio::test]
async fn an_unread_tag_list_is_recorded_with_its_cause() {
    use super::{CheckFailure, pin_release_tag};
    use octocode_github::ProviderErrorKind;
    let http = super::super::test_http::RouteHttp::new();
    let budget = super::super::test_http::test_budget();
    let failure = CheckFailure::Provider {
        kind: ProviderErrorKind::RateLimited,
        resets: None,
    };
    let tags = super::super::test_http::KnownTags::new(vec!["o/r@HEAD"])
        .with_listing_failure(failure.clone());
    let client = tag_client(&http, &budget, &tags);
    let mut item = unpinned("https://github.com/o/r");
    pin_release_tag(&mut item, "minimist", "1.2.6", None, &client).await;
    assert_eq!(item.source_ref, None);
    assert_eq!(
        item.tag_absence,
        Some(super::super::TagAbsence::ListUnread(failure.describe()))
    );
}

/// A lightweight tag's listed ref names its commit: the list settles it
/// with no read of the tag itself; an annotated tag is read once.
#[tokio::test]
async fn a_listed_lightweight_tag_needs_no_further_read() {
    let http = super::super::test_http::RouteHttp::new();
    let budget = super::super::test_http::test_budget();
    let tags = super::super::test_http::KnownTags::new(vec!["o/r@rel/1.2.1=abc"]);
    let client = tag_client(&http, &budget, &tags);
    let found = github_release_tag(
        "https://github.com/o/r",
        None,
        "r",
        "1.2.1",
        Lookup::default(),
        &client,
    )
    .await
    .found();
    assert_eq!(found.map(|release| release.commit).as_deref(), Some("abc"));
    assert!(
        !tags.seen().iter().any(|seen| seen == "o/r@rel/1.2.1"),
        "the listed commit is used as listed: {:?}",
        tags.seen()
    );
}

/// The repository's layout is read only when a scoped stem decides the
/// release: a generic tag at the same commit already names it (no read);
/// a stem tag at another commit is read for, and with no package of that
/// name it names the version as well, so neither tag is chosen.
#[tokio::test]
async fn a_tree_is_read_only_when_a_stem_decides_the_release() {
    use super::ReleaseTag;
    let http = super::super::test_http::RouteHttp::new();
    let budget = super::super::test_http::test_budget();
    let same = super::super::test_http::KnownTags::new(vec![
        "o/r@rel/1.2.1=abc",
        "o/r@release-v1.2.1=abc",
    ]);
    let client = tag_client(&http, &budget, &same);
    assert_eq!(
        github_release_tag(
            "https://github.com/o/r",
            None,
            "r",
            "1.2.1",
            Lookup::default(),
            &client
        )
        .await,
        TagSearch::Found(ReleaseTag {
            tag: "rel/1.2.1".into(),
            commit: "abc".into()
        })
    );
    assert!(
        !same.seen().iter().any(|seen| seen.contains("^tree@")),
        "{:?}",
        same.seen()
    );
    let split = super::super::test_http::KnownTags::new(vec![
        "o/r@rel/1.2.1=abc",
        "o/r@release-v1.2.1=def",
    ]);
    let client = tag_client(&http, &budget, &split);
    let search = github_release_tag(
        "https://github.com/o/r",
        None,
        "r",
        "1.2.1",
        Lookup::default(),
        &client,
    )
    .await;
    assert!(search.ambiguous(), "{search:?}: {:?}", split.seen());
    assert!(
        split.seen().iter().any(|seen| seen == "o/r^tree@def"),
        "{:?}",
        split.seen()
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
            CommitCheck::FoundAfterRedirect { to: None },
            Some(RepositoryStatus::Moved),
        ),
        (
            CommitCheck::FoundAfterRedirect {
                to: Some("minimistjs/minimist".into()),
            },
            Some(RepositoryStatus::Moved),
        ),
        (CommitCheck::Found, None),
    ] {
        let tags = super::super::test_http::KnownTags::new(vec![]).with_repository(check.clone());
        let client = tag_client(&http, &budget, &tags);
        let mut item = unpinned("https://github.com/substack/minimist");
        pin_release_tag(&mut item, "minimist", "1.2.6", None, &client).await;
        assert_eq!(item.repository_status, status, "{check:?}");
        // A redirect's target is where the project moved.
        let moved_to = match &check {
            CommitCheck::FoundAfterRedirect { to } => to.clone(),
            _ => None,
        };
        assert_eq!(item.successor_repository, moved_to, "{check:?}");
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
    assert_eq!(item.source_ref.as_deref(), Some("commit-of-v1.2.6"));
    assert_eq!(item.release_tag.as_deref(), Some("v1.2.6"));
    assert_eq!(item.repository_status, None);
}

/// A commit check is found, missing, or failed with its cause: a link to no
/// GitHub repository and a call without GitHub are told apart from a
/// GitHub failure, and only the last is stated on the row.
#[tokio::test]
async fn a_commit_check_names_why_it_failed() {
    use super::{CheckFailure, CommitCheck, commit_check, pin_release_tag};
    use octocode_github::ProviderErrorKind;
    let http = super::super::test_http::RouteHttp::new();
    let budget = super::super::test_http::test_budget();
    let tags = super::super::test_http::KnownTags::new(vec!["o/r@abc"]);
    let client = tag_client(&http, &budget, &tags);
    assert_eq!(
        commit_check("https://github.com/o/r", "abc", &client).await,
        Ok(CommitCheck::Found)
    );
    assert_eq!(
        commit_check("https://github.com/o/r", "def", &client).await,
        Ok(CommitCheck::Missing)
    );
    assert_eq!(
        commit_check("https://gitlab.com/o/r", "abc", &client).await,
        Err(CheckFailure::NotGitHub)
    );
    let bare = RegistryClient::new(&http, &budget);
    assert_eq!(
        commit_check("https://github.com/o/r", "abc", &bare).await,
        Err(CheckFailure::Unavailable)
    );
    // Only a failure of the check itself is stated on the row.
    for (failure, stated) in [
        (CheckFailure::Request, true),
        (
            CheckFailure::Provider {
                kind: ProviderErrorKind::RateLimited,
                resets: None,
            },
            true,
        ),
        (
            CheckFailure::Provider {
                kind: ProviderErrorKind::Server,
                resets: None,
            },
            true,
        ),
        (CheckFailure::Services("bad API URL".to_owned()), true),
        (CheckFailure::NotGitHub, false),
        (CheckFailure::Unavailable, false),
    ] {
        let tags = super::super::test_http::KnownTags::new(vec![]).with_failure(failure.clone());
        let client = tag_client(&http, &budget, &tags);
        let mut item = unpinned("https://github.com/o/r");
        pin_release_tag(&mut item, "p", "1.0.0", None, &client).await;
        assert_eq!(item.disclosures.len(), usize::from(stated), "{failure:?}");
        assert_eq!(item.repository_status, None, "{failure:?}");
        if stated {
            assert!(
                item.disclosures[0].contains(&failure.describe()),
                "{:?}",
                item.disclosures
            );
        }
    }
}

/// A call whose GitHub services cannot be built still checks, and each row
/// says why: the cause with its message, not a silent skip.
#[tokio::test]
async fn unbuildable_github_services_are_stated_with_their_message() {
    use super::{CheckFailure, FailedTags, pin_release_tag};
    let http = super::super::test_http::RouteHttp::new();
    let budget = super::super::test_http::test_budget();
    let tags = FailedTags(CheckFailure::Services("Invalid GitHub API URL.".to_owned()));
    let client = RegistryClient {
        tags: Some(&tags),
        ..RegistryClient::new(&http, &budget)
    };
    let mut item = unpinned("https://github.com/o/r");
    pin_release_tag(&mut item, "p", "1.0.0", None, &client).await;
    assert_eq!(item.disclosures.len(), 1, "{:?}", item.disclosures);
    assert!(
        item.disclosures[0].contains("Invalid GitHub API URL"),
        "{:?}",
        item.disclosures
    );
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
                .ok()
                .flatten()
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

/// Once a tag ties the version, the source is read at the commit the tag
/// names and the tag is named apart: `ref` is a commit in every ecosystem.
#[tokio::test]
async fn a_tag_pin_reads_at_the_tag_commit_and_names_the_tag() {
    use super::pin_release_tag;
    let http = super::super::test_http::RouteHttp::new();
    let budget = super::super::test_http::test_budget();
    let tags = super::super::test_http::KnownTags::new(vec![
        "o/r@v1.2.6=1234567890abcdef1234567890abcdef12345678",
    ]);
    let client = tag_client(&http, &budget, &tags);
    let mut item = unpinned("https://github.com/o/r");
    pin_release_tag(&mut item, "minimist", "1.2.6", None, &client).await;
    assert_eq!(
        item.source_ref.as_deref(),
        Some("1234567890abcdef1234567890abcdef12345678")
    );
    assert_eq!(item.release_tag.as_deref(), Some("v1.2.6"));
    assert!(item.source_tag);
}
