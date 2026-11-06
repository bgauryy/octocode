//! Release refs for registries whose metadata names no commit. Each is a
//! lead source only: any failure leaves the lookup without a release ref.
use super::util::github_slug;
use super::{ArtifactItem, RepositoryStatus, http::RegistryClient};

/// What GitHub says of a commit (or `HEAD`) of a repository.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitCheck {
    /// The commit exists.
    Found,
    /// The commit exists, but GitHub reached it through a redirect: the
    /// repository was renamed or transferred, so the registry's link is stale.
    FoundAfterRedirect,
    /// The repository exists and has no such commit.
    Missing,
    /// The repository itself does not resolve (deleted, private, or never
    /// there): GitHub answers 404 and no redirect.
    RepositoryMissing,
}

/// A pending commit check; `None` means the check could not run.
pub type CommitFuture<'a> =
    std::pin::Pin<Box<dyn std::future::Future<Output = Option<CommitCheck>> + 'a>>;
/// A pending look-up of the commit a release tag names.
pub type TagCommitFuture<'a> =
    std::pin::Pin<Box<dyn std::future::Future<Output = Option<Option<String>>> + 'a>>;

/// Checks that a release tag or a recorded commit exists in a GitHub
/// repository, through the configured GitHub API (its URL, the caller's
/// credential and request budget). `None` means the check failed.
pub trait ReleaseTags: Send + Sync {
    /// The commit `tag` of `owner/repo` names (a branch of that name is not
    /// a tag), `Some(None)` when the repository has no such tag.
    fn tag_commit<'a>(&'a self, owner: &'a str, repo: &'a str, tag: &'a str)
    -> TagCommitFuture<'a>;
    /// What GitHub says of `commit` (a SHA a registry recorded, or `HEAD`) in
    /// `owner/repo`.
    fn commit_check<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        commit: &'a str,
    ) -> CommitFuture<'a>;
}

/// Pins `artifact`'s source to the upstream release tag of `version` when
/// its registry named no commit: the one fallback every registry without a
/// recorded release commit (or whose commit is unknown) takes. `package` is
/// the name the package's tags go by (Maven's artifactId, not its
/// coordinate). `declared` is a release tag the package's own metadata names
/// (a Maven `<scm><tag>`): the first candidate, still checked to exist.
pub(crate) async fn pin_release_tag(
    artifact: &mut ArtifactItem,
    package: &str,
    version: &str,
    declared: Option<&str>,
    client: &RegistryClient<'_>,
) {
    let Some(repository) = unpinned_repository(artifact) else {
        return;
    };
    let found = github_release_tag(
        &repository,
        artifact.repository_directory.as_deref(),
        package,
        version,
        declared,
        client,
    )
    .await;
    settle_pin(artifact, &repository, found, client).await;
}

/// [`pin_release_tag`] for a Go module, whose tags follow the go command's own
/// rule rather than a registry's habits: `v<version>`, with the module's
/// directory in its repository as a prefix (`service/s3/v1.2.3`) when it is
/// not the root. `directory` is that directory, when known. Only the one tag a
/// module of this repository can carry is asked, never a spelling built from
/// the module path (a vanity path is not a name its repository's tags use).
pub(crate) async fn pin_go_release_tag(
    artifact: &mut ArtifactItem,
    directory: Option<&str>,
    version: &str,
    client: &RegistryClient<'_>,
) {
    let Some(repository) = unpinned_repository(artifact) else {
        return;
    };
    let found = go_release_tag(&repository, directory, version, client).await;
    settle_pin(artifact, &repository, found, client).await;
}

/// The repository a row still has to pin a release to: none when the row has a
/// ref already, names no repository, or its repository is gone.
fn unpinned_repository(artifact: &ArtifactItem) -> Option<String> {
    if artifact.source_ref.is_some()
        || artifact.repository_status == Some(RepositoryStatus::Missing)
    {
        return None;
    }
    artifact.repository.clone()
}

/// Records the outcome of a tag lookup on the row.
async fn settle_pin(
    artifact: &mut ArtifactItem,
    repository: &str,
    found: Option<ReleaseTag>,
    client: &RegistryClient<'_>,
) {
    artifact.source_ref = found.map(|release| release.tag);
    artifact.source_tag = artifact.source_ref.is_some();
    // No tag to pin: the source reads the default branch of the repository
    // the registry names, so first learn whether that repository is there.
    if artifact.source_ref.is_none()
        && let Some(check) = commit_check(repository, "HEAD", client).await
    {
        artifact.note_repository(check);
    }
}

/// Ties a registry's unchecked release commit to the upstream release tag of
/// `version`, when one exists: the check a reader would otherwise make by
/// hand. The tag naming that commit proves it (`verification: tag`); a tag
/// naming another commit is said on the row, since a package published from
/// a commit other than its tag may differ from it. No tag, or a failed check,
/// leaves the commit as the registry's claim.
pub(crate) async fn check_commit_against_tag(
    artifact: &mut ArtifactItem,
    package: &str,
    version: &str,
    commit: &str,
    client: &RegistryClient<'_>,
) {
    let Some(repository) = artifact.repository.as_deref() else {
        return;
    };
    let Some(release) = github_release_tag(
        repository,
        artifact.repository_directory.as_deref(),
        package,
        version,
        None,
        client,
    )
    .await
    else {
        return;
    };
    if release.commit.eq_ignore_ascii_case(commit) {
        artifact.source_tag = true;
    } else {
        artifact.tag_differs = Some(release);
    }
}

/// An upstream release tag and the commit it names.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseTag {
    pub tag: String,
    pub commit: String,
}

/// The release tag of `version` in a GitHub repository, when one exists
/// upstream: `v<version>`, `<version>`, then the shapes a monorepo uses for
/// one package's releases ([`tag_candidates`]); a leading `v` of `version` is
/// the same release, and a tag the package's metadata declares is asked first. Without a GitHub check, or
/// when the check fails (rate limit, missing repository), there is no tag.
/// Each lookup checks current tags; neither matches nor absences are retained.
async fn github_release_tag(
    repository: &str,
    directory: Option<&str>,
    package: &str,
    version: &str,
    declared: Option<&str>,
    client: &RegistryClient<'_>,
) -> Option<ReleaseTag> {
    let tags = client.tags?;
    let (owner, repo) = github_slug(repository)?;
    // A version written with its `v` (`v2.2.8`) names the same release as without.
    let bare = version
        .strip_prefix('v')
        .filter(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
        .unwrap_or(version);
    let mut candidates = tag_candidates(&repo, package, directory, bare);
    let named = declared
        .filter(|tag| !tag.is_empty() && !candidates.iter().any(|known| known == tag))
        .map(|tag| candidates.insert(0, tag.to_owned()))
        .is_some();
    // Two rounds: the plain tags most releases carry (and the one the
    // package declares), then a monorepo package's own shapes, each round's
    // checks together.
    let (plain, own) = candidates.split_at(candidates.len().min(2 + usize::from(named)));
    first_existing_tag(tags, &owner, &repo, &[plain, own]).await
}

/// The release tag of a Go module version in a GitHub repository: the go
/// command's spelling, `v<version>` under the module's `directory` when it
/// has one. A leading `v` and `+incompatible` are not part of the release.
async fn go_release_tag(
    repository: &str,
    directory: Option<&str>,
    version: &str,
    client: &RegistryClient<'_>,
) -> Option<ReleaseTag> {
    let tags = client.tags?;
    let (owner, repo) = github_slug(repository)?;
    let release = version.strip_suffix("+incompatible").unwrap_or(version);
    let tag = match directory.filter(|directory| !directory.is_empty()) {
        Some(directory) => format!("{directory}/{release}"),
        None => release.to_owned(),
    };
    first_existing_tag(tags, &owner, &repo, &[&[tag]]).await
}

/// The first of `candidates` (rounds in order, each round's checks together,
/// a round's candidates in order) the repository has as a tag. A failed check
/// is no answer: nothing is remembered, and there is no tag.
async fn first_existing_tag(
    tags: &dyn ReleaseTags,
    owner: &str,
    repo: &str,
    rounds: &[&[String]],
) -> Option<ReleaseTag> {
    for round in rounds {
        let answers = futures_util::future::join_all(
            round.iter().map(|tag| tags.tag_commit(owner, repo, tag)),
        )
        .await;
        for (tag, commit) in round.iter().zip(answers) {
            if let Some(commit) = commit? {
                return Some(ReleaseTag {
                    tag: tag.clone(),
                    commit,
                });
            }
        }
    }
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

/// What GitHub says of `reference` (a recorded commit, or `HEAD`) in
/// `repository`; `None` when the repository is not on GitHub or no check
/// could run (no GitHub access, rate limit), so only a definite answer
/// changes a row.
pub(crate) async fn commit_check(
    repository: &str,
    reference: &str,
    client: &RegistryClient<'_>,
) -> Option<CommitCheck> {
    let (tags, (owner, repo)) = (client.tags?, github_slug(repository)?);
    tags.commit_check(&owner, &repo, reference).await
}

#[cfg(test)]
mod tests;
