//! The release tag of a version from the repository's own tag list: the
//! tags no candidate shape spells (`rel/2.14.1`, `release-1.4.0`), found by
//! the version they end with, whatever prefix the project's tags carry.
use super::{ArtifactItem, CheckFailure, ReleaseTag, ReleaseTags, names_version};

/// What a release-tag lookup found for one version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TagSearch {
    /// No GitHub check ran: the call has no GitHub access, or the
    /// repository is not on GitHub.
    Unchecked,
    /// The release tag and the commit it names.
    Found(ReleaseTag),
    /// The repository's whole tag list was read and no tag is the release:
    /// none names the version (empty), or the tags listed here name it at
    /// different commits, so none is the release by its name alone.
    Unnamed(Vec<ReleaseTag>),
    /// No candidate shape names the version, and the tag list could not be
    /// read or its tags resolved, for this reason: a tag may still name it.
    Unlisted(CheckFailure),
}

impl TagSearch {
    /// The release tag, when the lookup found one.
    pub(crate) fn found(self) -> Option<ReleaseTag> {
        match self {
            Self::Found(release) => Some(release),
            Self::Unchecked | Self::Unnamed(_) | Self::Unlisted(_) => None,
        }
    }

    /// Records on the row what the lookup left open: tags that name
    /// `version` at different commits, or a tag list that could not be read
    /// (the warning then scopes its "no tag" claim to the shapes checked).
    pub(super) fn note_on(&self, artifact: &mut ArtifactItem, version: &str) {
        match self {
            Self::Unnamed(named) if !named.is_empty() => {
                let tags = named
                    .iter()
                    .map(|release| format!("{} (commit {})", release.tag, release.commit))
                    .collect::<Vec<_>>()
                    .join(", ");
                artifact.disclosures.push(format!(
                    "{}: tags {tags} each name version {version} at different commits, so none is taken as its release tag.",
                    artifact.package_name
                ));
            }
            Self::Unlisted(failure) => artifact.tags_unlisted = Some(failure.describe()),
            Self::Unchecked | Self::Found(_) | Self::Unnamed(_) => {}
        }
    }
}

/// The release tag of `version` among every tag of `owner/repo`: each tag
/// that names the version ([`names_version`]) is resolved to its commit.
/// Tags that all name one commit are that release (the shortest name, then
/// the first in order, is the one pinned); tags naming different commits
/// are listed, never chosen between.
pub(super) async fn listed_release_tag(
    tags: &dyn ReleaseTags,
    owner: &str,
    repo: &str,
    version: &str,
) -> TagSearch {
    let names = match tags.tag_names(owner, repo).await {
        Ok(names) => names,
        Err(failure) => return TagSearch::Unlisted(failure),
    };
    let mut named = names
        .into_iter()
        .filter(|name| names_version(name, version))
        .collect::<Vec<_>>();
    named.sort_by(|left, right| left.len().cmp(&right.len()).then_with(|| left.cmp(right)));
    named.dedup();
    let answers =
        futures_util::future::join_all(named.iter().map(|tag| tags.tag_commit(owner, repo, tag)))
            .await;
    let mut releases = Vec::new();
    for (tag, answer) in named.into_iter().zip(answers) {
        match answer {
            Ok(Some(commit)) => releases.push(ReleaseTag { tag, commit }),
            // Deleted since the list was read: not a tag of the repository.
            Ok(None) => {}
            Err(failure) => return TagSearch::Unlisted(failure),
        }
    }
    let one_commit = releases.first().is_some_and(|first| {
        releases
            .iter()
            .all(|release| release.commit.eq_ignore_ascii_case(&first.commit))
    });
    if one_commit {
        TagSearch::Found(releases.swap_remove(0))
    } else {
        TagSearch::Unnamed(releases)
    }
}
