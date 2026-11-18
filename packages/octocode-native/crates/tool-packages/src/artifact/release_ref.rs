//! Release refs for registries whose metadata names no commit. Each is a
//! lead source only: any failure leaves the lookup without a release ref.
use super::util::github_slug;
use super::{ArtifactItem, RepositoryStatus, http::RegistryClient};
mod listing;
pub(crate) use listing::TagSearch;
pub use listing::{TreeFiles, TreeFilesFuture};

/// What GitHub says of a commit (or `HEAD`) of a repository.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommitCheck {
    /// The commit exists.
    Found,
    /// The commit exists, but GitHub reached it through a redirect: the
    /// repository was renamed or transferred, so the registry's link is
    /// stale. `to` is the `owner/repo` GitHub redirects to, when it names it.
    FoundAfterRedirect { to: Option<String> },
    /// The repository exists and has no such commit.
    Missing,
    /// The repository itself does not resolve (deleted, private, or never
    /// there): GitHub answers 404 and no redirect.
    RepositoryMissing,
}

/// Why a commit check gave no answer about the commit or the repository.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CheckFailure {
    /// The repository link names no GitHub repository.
    NotGitHub,
    /// This call has no GitHub access.
    Unavailable,
    /// No GitHub credential or request could be prepared.
    Request,
    /// GitHub answered with this failure (a rate limit, an authorization or
    /// a network failure). `resets` says when a rate limit lifts, as GitHub
    /// stated it ("at <ISO time>" or "in N seconds"), so the row says when
    /// to ask again ([`Self::provider`]).
    Provider {
        kind: octocode_github::ProviderErrorKind,
        resets: Option<String>,
    },
    /// The GitHub services could not be built (a malformed API URL, a bad
    /// setting): every check of the call fails the same way, with this message.
    Services(String),
}

impl CheckFailure {
    /// The failure GitHub answered with: its kind and, for a rate limit,
    /// when it lifts (its reset, else its `retry-after`).
    pub fn provider(error: &octocode_github::ProviderError) -> Self {
        let resets = error.rate_limit.as_deref().and_then(|limit| {
            limit
                .reset_epoch_seconds
                .and_then(|secs| i64::try_from(secs).ok())
                .map(|secs| {
                    format!(
                        "at {}",
                        octocode_runtime_base::civil_date::iso8601_secs(secs)
                    )
                })
                .or_else(|| {
                    limit
                        .retry_after_seconds
                        .map(|secs| format!("in {secs} seconds"))
                })
        });
        Self::Provider {
            kind: error.kind,
            resets,
        }
    }

    /// The cause, as a clause a disclosure can carry.
    pub fn describe(&self) -> String {
        use octocode_github::ProviderErrorKind as Kind;
        match self {
            Self::Services(message) => format!(
                "GitHub could not be set up: {}",
                message.trim_end_matches('.')
            ),
            Self::NotGitHub => "the repository is not on GitHub".to_owned(),
            Self::Unavailable => "GitHub is not available to this call".to_owned(),
            Self::Request => "no GitHub request could be prepared (credentials)".to_owned(),
            Self::Provider {
                kind: Kind::RateLimited,
                resets,
            } => match resets {
                Some(resets) => format!("GitHub's rate limit stopped it; it resets {resets}"),
                None => "GitHub's rate limit stopped it".to_owned(),
            },
            Self::Provider {
                kind: Kind::Authentication | Kind::Permission,
                ..
            } => "GitHub refused the credential".to_owned(),
            Self::Provider { kind, .. } => format!("GitHub failed it ({})", kind.code()),
        }
    }
}

/// Release-tag checks that all fail with one cause: what a call uses when
/// its GitHub services cannot be built, so each check states why instead of
/// being skipped.
pub struct FailedTags(pub CheckFailure);

impl ReleaseTags for FailedTags {
    fn tag_commit<'a>(&'a self, _: &'a str, _: &'a str, _: &'a str) -> TagCommitFuture<'a> {
        let failure = self.0.clone();
        Box::pin(async move { Err(failure) })
    }

    fn tag_refs<'a>(&'a self, _: &'a str, _: &'a str) -> TagRefsFuture<'a> {
        let failure = self.0.clone();
        Box::pin(async move { Err(failure) })
    }

    fn tag_commits<'a>(
        &'a self,
        _: &'a str,
        _: &'a str,
        _: &'a [octocode_github::TagRef],
    ) -> TagCommitsFuture<'a> {
        let failure = self.0.clone();
        Box::pin(async move { Err(failure) })
    }

    fn commit_check<'a>(&'a self, _: &'a str, _: &'a str, _: &'a str) -> CommitFuture<'a> {
        let failure = self.0.clone();
        Box::pin(async move { Err(failure) })
    }

    fn tree_files<'a>(&'a self, _: &'a str, _: &'a str, _: &'a str) -> TreeFilesFuture<'a> {
        let failure = self.0.clone();
        Box::pin(async move { Err(failure) })
    }
}

/// What a commit check found, or why it could not tell: a missing commit and
/// a failed check are different answers.
pub(super) type CommitResult = Result<CommitCheck, CheckFailure>;

/// A pending commit check.
pub type CommitFuture<'a> = std::pin::Pin<Box<dyn std::future::Future<Output = CommitResult> + 'a>>;
/// A pending look-up of the commit a release tag names: `Ok(None)` when the
/// repository has no such tag.
pub type TagCommitFuture<'a> =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<String>, CheckFailure>> + 'a>>;
/// A pending read of every tag of a repository.
pub type TagRefsFuture<'a> = std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<Vec<octocode_github::TagRef>, CheckFailure>> + 'a>,
>;
/// A pending look-up of the commit each listed tag names, in order (`None`:
/// no longer a tag of the repository).
pub type TagCommitsFuture<'a> = std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<Vec<Option<String>>, CheckFailure>> + 'a>,
>;

/// Checks that a release tag or a recorded commit exists in a GitHub
/// repository, through the configured GitHub API (its URL, the caller's
/// credential and request budget).
pub trait ReleaseTags: Send + Sync {
    /// The commit `tag` of `owner/repo` names (a branch of that name is not
    /// a tag), `Ok(None)` when the repository has no such tag.
    fn tag_commit<'a>(&'a self, owner: &'a str, repo: &'a str, tag: &'a str)
    -> TagCommitFuture<'a>;
    /// Every tag of `owner/repo`, read whole.
    fn tag_refs<'a>(&'a self, owner: &'a str, repo: &'a str) -> TagRefsFuture<'a>;
    /// The commit each of `tags` (listed by [`Self::tag_refs`]) names, in
    /// order: a lightweight tag's as listed, every annotated tag's target
    /// read together.
    fn tag_commits<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        tags: &'a [octocode_github::TagRef],
    ) -> TagCommitsFuture<'a>;
    /// What GitHub says of `commit` (a SHA a registry recorded, or `HEAD`) in
    /// `owner/repo`.
    fn commit_check<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        commit: &'a str,
    ) -> CommitFuture<'a>;
    /// The files of `owner/repo`'s git tree at `commit`, read whole (one
    /// recursive read): the layout that says which directories are packages.
    fn tree_files<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        commit: &'a str,
    ) -> TreeFilesFuture<'a>;
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
    let search = github_release_tag(
        &repository,
        artifact.repository_directory.as_deref(),
        package,
        version,
        declared,
        client,
    )
    .await;
    let ambiguous = search.ambiguous();
    search.note_on(artifact, version);
    let mut found = search.found();
    // No tag names the release: a declared tag of another release (a release
    // candidate's) that exists is still nearer the release than the default
    // branch. It is pinned as the registry's claim (`registryRef`), never as
    // a checked release tag, and the response names it. Tags that name the
    // release at different commits are stated instead: a release
    // candidate's tag is no nearer than they are.
    let mut declared_ref = false;
    if found.is_none()
        && !ambiguous
        && let Some(tag) = declared
            .map(str::trim)
            .filter(|tag| !names_version(tag, bare_version(version)))
        && let (Some(tags), Some((owner, repo))) = (client.tags, github_slug(&repository))
    {
        found = first_existing_tag(tags, &owner, &repo, &[&[tag.to_owned()]])
            .await
            .ok()
            .flatten();
        declared_ref = found.is_some();
    }
    settle_pin(artifact, &repository, found, client).await;
    if declared_ref {
        // The declared tag is the registry's claim: read at its name, never
        // stated as this release's tag.
        artifact.source_ref = artifact.release_tag.take();
        artifact.source_tag = false;
        artifact.declared_ref = true;
    }
}

/// A version written with its `v` (`v2.2.8`) names the same release as without.
fn bare_version(version: &str) -> &str {
    version
        .strip_prefix('v')
        .filter(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
        .unwrap_or(version)
}

/// [`pin_release_tag`] for a Go module, whose tags follow the go command's own
/// rule rather than a registry's habits: `v<version>`, with the module's
/// directory in its repository as a prefix (`service/s3/v1.2.3`) when it is
/// not the root. `directory` is that directory, when known. Only the one tag a
/// module of this repository can carry is asked, never a spelling built from
/// the module path (a vanity path is not a name its repository's tags use).
/// `spelled` is that tag as the module path spells it: when the tag could not
/// be checked (no GitHub, a failed check), it stays the ref, as the
/// registry's claim, rather than no ref at all.
pub(crate) async fn pin_go_release_tag(
    artifact: &mut ArtifactItem,
    directory: Option<&str>,
    version: &str,
    spelled: Option<String>,
    client: &RegistryClient<'_>,
) {
    let Some(repository) = unpinned_repository(artifact) else {
        return;
    };
    match go_release_tag(&repository, directory, version, client).await {
        Ok(found) => settle_pin(artifact, &repository, found, client).await,
        Err(_) if spelled.is_some() => artifact.source_ref = spelled,
        Err(_) => settle_pin(artifact, &repository, None, client).await,
    }
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

/// Records the outcome of a tag lookup on the row: the source is read at the
/// commit the tag names, and the tag is named apart (`releaseTag`).
async fn settle_pin(
    artifact: &mut ArtifactItem,
    repository: &str,
    found: Option<ReleaseTag>,
    client: &RegistryClient<'_>,
) {
    artifact.source_tag = found.is_some();
    artifact.source_ref = found.as_ref().map(|release| release.commit.clone());
    artifact.release_tag = found.map(|release| release.tag);
    // No tag to pin: the source reads the default branch of the repository
    // the registry names, so first learn whether that repository is there.
    if artifact.source_ref.is_none() {
        match commit_check(repository, "HEAD", client).await {
            Ok(check) => artifact.note_repository(check),
            Err(failure) => artifact.note_unchecked("the repository", failure),
        }
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
    let Some(search) = release_tag_search(artifact, package, version, client).await else {
        return;
    };
    settle_commit_against_tag(artifact, version, commit, &search);
}

/// The release tag lookup [`check_commit_against_tag`] settles, for a row
/// whose repository is known: a caller that has the commit to check may run
/// it beside that commit's own existence check instead of after it.
pub(crate) async fn release_tag_search(
    artifact: &ArtifactItem,
    package: &str,
    version: &str,
    client: &RegistryClient<'_>,
) -> Option<TagSearch> {
    let repository = artifact.repository.as_deref()?;
    Some(
        github_release_tag(
            repository,
            artifact.repository_directory.as_deref(),
            package,
            version,
            None,
            client,
        )
        .await,
    )
}

/// [`check_commit_against_tag`] with its lookup already made.
pub(crate) fn settle_commit_against_tag(
    artifact: &mut ArtifactItem,
    version: &str,
    commit: &str,
    search: &TagSearch,
) {
    match search {
        TagSearch::Found(release) | TagSearch::Named { release, .. } => {
            if release.commit.eq_ignore_ascii_case(commit) {
                artifact.source_tag = true;
                artifact.release_tag = Some(release.tag.clone());
            } else {
                artifact.tag_differs = Some(release.clone());
            }
            search.note_on(artifact, version);
        }
        // Tags naming the version at different commits: the one naming the
        // registry's commit proves it.
        TagSearch::Unnamed { differing, .. }
            if differing
                .iter()
                .any(|release| release.commit.eq_ignore_ascii_case(commit)) =>
        {
            artifact.source_tag = true;
            artifact.release_tag = differing
                .iter()
                .find(|release| release.commit.eq_ignore_ascii_case(commit))
                .map(|release| release.tag.clone());
        }
        TagSearch::Unnamed { .. } => search.note_on(artifact, version),
        // The registry's commit stays its claim (`registryRef`), which says
        // no tag checked it: no absence is claimed to scope.
        TagSearch::Unchecked | TagSearch::Unlisted { .. } => {}
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
/// the same release, and a tag the package's metadata declares is asked
/// first. When no candidate names it (or a check fails), the repository's
/// own tag list decides by what each tag's prefix names
/// ([`listing::listed_release_tag`]); a lookup cut short is
/// [`TagSearch::Unlisted`] with its cause and whether the candidates were
/// checked at all. Each lookup checks current tags; neither matches nor
/// absences are retained.
async fn github_release_tag(
    repository: &str,
    directory: Option<&str>,
    package: &str,
    version: &str,
    declared: Option<&str>,
    client: &RegistryClient<'_>,
) -> TagSearch {
    let (Some(tags), Some((owner, repo))) = (client.tags, github_slug(repository)) else {
        return TagSearch::Unchecked;
    };
    let bare = bare_version(version);
    let mut candidates = tag_candidates(&repo, package, directory, bare);
    let named = declared
        .and_then(|tag| declared_candidate(tag, bare))
        .filter(|tag| !candidates.contains(tag))
        .map(|tag| candidates.insert(0, tag))
        .is_some();
    // Two rounds: the plain tags most releases carry (and the one the
    // package declares), then a monorepo package's own shapes, each round's
    // checks together.
    let (plain, own) = candidates.split_at(candidates.len().min(2 + usize::from(named)));
    // A failed candidate check is kept: a claim about the shapes checked
    // holds only when they were.
    let shapes_checked = match first_existing_tag(tags, &owner, &repo, &[plain, own]).await {
        Ok(Some(release)) => return TagSearch::Found(release),
        Ok(None) => true,
        Err(_) => false,
    };
    let own = listing::OwnNames::new(&repo, package, directory);
    listing::listed_release_tag(tags, &owner, &repo, bare, &own, shapes_checked).await
}

/// Whether `tag` names release `version`: it ends with the version, and the
/// version starts the token (no digit or `.` before it: `rel/2.14.1`,
/// `log4j-2.14.1` and `v2.14.1` name 2.14.1; `12.14.1` and `2.14.1-rc1` do not).
fn names_version(tag: &str, version: &str) -> bool {
    tag.strip_suffix(version).is_some_and(|before| {
        !version.is_empty() && !before.ends_with(|c: char| c.is_ascii_digit() || c == '.')
    })
}

/// The candidate a declared tag (a Maven `<scm><tag>`) gives for `version`. A
/// declared tag is evidence of its shape, not of the release: one that names
/// this version is asked as is; one naming another release or a release
/// candidate (`rel/2.25.3`, `log4j-2.14.1-rc1`) lends only its prefix before
/// its version (`rel/`, `log4j-`), asked with this version. A tag with no
/// version token (`HEAD`) gives none.
fn declared_candidate(tag: &str, version: &str) -> Option<String> {
    let tag = tag.trim();
    if names_version(tag, version) {
        return Some(tag.to_owned());
    }
    // The first `<digits>.<digit>` that starts a token: not inside a word
    // (`log4j`), only after a separator, a lone `v`, or the start.
    let bytes = tag.as_bytes();
    let starts = (0..bytes.len()).find(|&at| {
        let token = bytes[at].is_ascii_digit()
            && match at.checked_sub(1).map(|before| bytes[before]) {
                None => true,
                Some(b'v' | b'V') => at < 2 || !bytes[at - 2].is_ascii_alphanumeric(),
                Some(before) => !before.is_ascii_alphanumeric() && before != b'.',
            };
        let rest = &bytes[at..];
        let digits = rest.iter().take_while(|byte| byte.is_ascii_digit()).count();
        token
            && rest.get(digits) == Some(&b'.')
            && rest.get(digits + 1).is_some_and(u8::is_ascii_digit)
    })?;
    Some(format!("{}{version}", tag.get(..starts)?))
}

/// The release tag of a Go module version in a GitHub repository: the go
/// command's spelling, `v<version>` under the module's `directory` when it
/// has one. A leading `v` and `+incompatible` are not part of the release.
async fn go_release_tag(
    repository: &str,
    directory: Option<&str>,
    version: &str,
    client: &RegistryClient<'_>,
) -> Result<Option<ReleaseTag>, CheckFailure> {
    let tags = client.tags.ok_or(CheckFailure::Unavailable)?;
    let (owner, repo) = github_slug(repository).ok_or(CheckFailure::NotGitHub)?;
    let release = version.strip_suffix("+incompatible").unwrap_or(version);
    let tag = match directory.filter(|directory| !directory.is_empty()) {
        Some(directory) => format!("{directory}/{release}"),
        None => release.to_owned(),
    };
    first_existing_tag(tags, &owner, &repo, &[&[tag]]).await
}

/// The first of `candidates` (rounds in order, each round's checks together,
/// a round's candidates in order) the repository has as a tag. A failed check
/// is no answer: nothing is remembered, and the lookup fails with its cause.
async fn first_existing_tag(
    tags: &dyn ReleaseTags,
    owner: &str,
    repo: &str,
    rounds: &[&[String]],
) -> Result<Option<ReleaseTag>, CheckFailure> {
    for round in rounds {
        let answers = futures_util::future::join_all(
            round.iter().map(|tag| tags.tag_commit(owner, repo, tag)),
        )
        .await;
        for (tag, commit) in round.iter().zip(answers) {
            if let Some(commit) = commit? {
                return Ok(Some(ReleaseTag {
                    tag: tag.clone(),
                    commit,
                }));
            }
        }
    }
    Ok(None)
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
/// `repository`: found, missing, or the [`CheckFailure`] that left it
/// unknown, so only a definite answer changes a row and a failure is stated.
pub(crate) async fn commit_check(
    repository: &str,
    reference: &str,
    client: &RegistryClient<'_>,
) -> CommitResult {
    let tags = client.tags.ok_or(CheckFailure::Unavailable)?;
    let (owner, repo) = github_slug(repository).ok_or(CheckFailure::NotGitHub)?;
    tags.commit_check(&owner, &repo, reference).await
}

#[cfg(test)]
mod tests;
