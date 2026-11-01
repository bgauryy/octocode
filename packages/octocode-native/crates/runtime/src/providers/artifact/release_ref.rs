//! Release refs for registries whose metadata names no commit. Each is a
//! lead source only: any failure leaves the lookup without a release ref.
use super::util::{github_repo_dir, github_slug, string};
use super::{ArtifactItem, http::RegistryClient};

/// A pending release-tag check.
pub type TagFuture<'a> = std::pin::Pin<Box<dyn std::future::Future<Output = Option<bool>> + 'a>>;

/// Checks that a release tag or a recorded commit exists in a GitHub
/// repository, through the configured GitHub API (its URL, the caller's
/// credential and request budget). Each answers `Some(true)` when it
/// exists, `Some(false)` when the repository has none, `None` when the
/// check failed.
pub trait ReleaseTags: Send + Sync {
    /// `tag` is a tag of `owner/repo` (a branch of that name is not).
    fn tag_exists<'a>(&'a self, owner: &'a str, repo: &'a str, tag: &'a str) -> TagFuture<'a>;
    /// `commit` (a SHA a registry recorded) is a commit of `owner/repo`.
    fn commit_exists<'a>(&'a self, owner: &'a str, repo: &'a str, commit: &'a str)
    -> TagFuture<'a>;
}

/// Pins `artifact`'s source to the upstream release tag of `version` when
/// its registry named no commit: the one fallback every registry without a
/// recorded release commit (or whose commit is unknown) takes. `package` is
/// the name the package's tags go by (Maven's artifactId, not its
/// coordinate).
pub(crate) async fn pin_release_tag(
    artifact: &mut ArtifactItem,
    package: &str,
    version: &str,
    client: &RegistryClient<'_>,
) {
    if artifact.source_ref.is_some() {
        return;
    }
    let Some(repository) = artifact.repository.as_deref() else {
        return;
    };
    let directory = artifact
        .repository_directory
        .clone()
        .or_else(|| github_repo_dir(repository));
    artifact.source_ref =
        github_release_tag(repository, directory.as_deref(), package, version, client).await;
    artifact.source_tag = artifact.source_ref.is_some();
}

/// The release tag of `version` in a GitHub repository, when one exists
/// upstream: `v<version>`, `<version>`, then the shapes a monorepo uses for
/// one package's releases ([`tag_candidates`]). Without a GitHub check, or
/// when the check fails (rate limit, missing repository), there is no tag.
/// A found tag is remembered as a release fact, so a repeated lookup spends
/// no GitHub request; an absence is remembered as long as a registry answer
/// (the tag may be pushed later).
async fn github_release_tag(
    repository: &str,
    directory: Option<&str>,
    package: &str,
    version: &str,
    client: &RegistryClient<'_>,
) -> Option<String> {
    let tags = client.tags?;
    let (owner, repo) = github_slug(repository)?;
    let resource = format!(
        "tag:{}/{}/{}:{package}@{version}",
        owner.to_ascii_lowercase(),
        repo.to_ascii_lowercase(),
        directory.unwrap_or_default()
    );
    if let Some(fact) = client.fact(&resource).await {
        return string(fact.get("tag"));
    }
    let candidates = tag_candidates(&repo, package, directory, version);
    // Two rounds: the plain tags most releases carry, then a monorepo
    // package's own shapes, each round's checks together.
    let (plain, own) = candidates.split_at(candidates.len().min(2));
    for round in [plain, own] {
        let answers = futures_util::future::join_all(
            round.iter().map(|tag| tags.tag_exists(&owner, &repo, tag)),
        )
        .await;
        for (tag, exists) in round.iter().zip(answers) {
            // A failed check is no answer: nothing is remembered.
            if exists? {
                client
                    .remember_fact(&resource, &serde_json::json!({ "tag": tag }))
                    .await;
                return Some(tag.clone());
            }
        }
    }
    client
        .remember_answer(&resource, &serde_json::json!({ "tag": null }))
        .await;
    None
}

/// Tags a package's release may carry, most common first: `v<version>` and
/// `<version>`, then `<stem>-v<version>` and `<stem>@<version>` for each name
/// the package goes by inside its repository: the package name (scoped or
/// not), its directory, and either without the repository name's prefix
/// (`pydantic_core` in `pydantic` is tagged `core-v2.50.1`). A package that
/// is its repository, with no directory of its own, is tagged without a stem.
fn tag_candidates(
    repo: &str,
    package: &str,
    directory: Option<&str>,
    version: &str,
) -> Vec<String> {
    let mut tags = vec![format!("v{version}"), version.to_owned()];
    for stem in tag_stems(repo, package, directory) {
        for tag in [format!("{stem}-v{version}"), format!("{stem}@{version}")] {
            if !tags.contains(&tag) {
                tags.push(tag);
            }
        }
    }
    tags
}

fn tag_stems(repo: &str, package: &str, directory: Option<&str>) -> Vec<String> {
    let key = |name: &str| name.to_ascii_lowercase().replace('_', "-");
    let repo_key = key(repo);
    let unscoped = package.rsplit('/').next().unwrap_or(package);
    let directory = directory
        .and_then(|directory| directory.rsplit('/').next())
        .filter(|leaf| !leaf.is_empty());
    if directory.is_none() && key(unscoped) == repo_key {
        return Vec::new();
    }
    let without_repo = |name: &str| {
        key(name)
            .strip_prefix(&repo_key)
            .and_then(|rest| rest.strip_prefix('-'))
            .filter(|rest| !rest.is_empty())
            .map(str::to_owned)
    };
    let mut stems: Vec<String> = Vec::new();
    for stem in [
        Some(package.to_owned()),
        Some(unscoped.to_owned()),
        directory.map(str::to_owned),
        Some(key(unscoped)),
        without_repo(unscoped),
        directory.and_then(without_repo),
    ]
    .into_iter()
    .flatten()
    {
        if !stem.is_empty() && !stems.contains(&stem) {
            stems.push(stem);
        }
    }
    stems
}

/// Whether GitHub says `repository` has no commit `reference`: `false` when
/// it has one, when the repository is not on GitHub, or when no check could
/// run (no GitHub access, rate limit), so only a definite answer drops a ref.
pub(crate) async fn github_ref_missing(
    repository: &str,
    reference: &str,
    client: &RegistryClient<'_>,
) -> bool {
    let (Some(tags), Some((owner, repo))) = (client.tags, github_slug(repository)) else {
        return false;
    };
    tags.commit_exists(&owner, &repo, reference).await == Some(false)
}

#[cfg(test)]
mod tests {
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
        cache: &'a super::super::ArtifactCache,
        tags: &'a super::super::test_http::KnownTags,
    ) -> RegistryClient<'a> {
        RegistryClient {
            http,
            budget,
            cache: Some(cache),
            tags: Some(tags),
        }
    }

    /// A package with no matching tag is checked once per registry-answer
    /// lifetime: the repeat lookup spends no GitHub request.
    #[tokio::test]
    async fn an_absent_release_tag_is_remembered_for_a_registry_answer_lifetime() {
        let http = super::super::test_http::RouteHttp::new();
        let budget = super::super::test_http::test_budget();
        let cache = super::super::ArtifactCache::new(None);
        let tags = super::super::test_http::KnownTags::always(Some(false));
        let client = tag_client(&http, &budget, &cache, &tags);
        let repository = "https://github.com/acme/monorepo";
        for _ in 0..2 {
            assert_eq!(
                github_release_tag(
                    repository,
                    Some("packages/widget"),
                    "widget",
                    "1.0.0",
                    &client
                )
                .await,
                None
            );
        }
        let candidates = tag_candidates("monorepo", "widget", Some("packages/widget"), "1.0.0");
        assert_eq!(tags.seen().len(), candidates.len(), "{:?}", tags.seen());
        // A failed check is not an absence: it is asked again.
        let failing = super::super::test_http::KnownTags::always(None);
        let client = tag_client(&http, &budget, &cache, &failing);
        for _ in 0..2 {
            assert_eq!(
                github_release_tag(repository, None, "other", "2.0.0", &client).await,
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
        let cache = super::super::ArtifactCache::new(None);
        let plain = super::super::test_http::KnownTags::new(vec!["acme/widget@v1.0.0"]);
        let client = tag_client(&http, &budget, &cache, &plain);
        assert_eq!(
            github_release_tag(
                "https://github.com/acme/widget",
                Some("packages/core"),
                "core",
                "1.0.0",
                &client
            )
            .await
            .as_deref(),
            Some("v1.0.0")
        );
        assert_eq!(plain.seen(), ["acme/widget@v1.0.0", "acme/widget@1.0.0"]);
        let stems = super::super::test_http::KnownTags::new(vec![
            "acme/widget@core@1.0.0",
            "acme/widget@core-v1.0.0",
        ]);
        let cache = super::super::ArtifactCache::new(None);
        let client = tag_client(&http, &budget, &cache, &stems);
        assert_eq!(
            github_release_tag(
                "https://github.com/acme/widget",
                Some("packages/core"),
                "core",
                "1.0.0",
                &client
            )
            .await
            .as_deref(),
            Some("core-v1.0.0")
        );
        assert_eq!(
            stems.seen().len(),
            tag_candidates("widget", "core", Some("packages/core"), "1.0.0").len()
        );
    }
}
