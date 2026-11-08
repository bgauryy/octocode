//! The release tag of a version from the repository's own tag list: the
//! tags no candidate shape spells (`rel/2.14.1`, `release-1.4.0`), found by
//! the version they end with. What a tag's prefix names decides whose
//! release it is: `<name>@` names one package, this one or another;
//! `<name>-v`, `<name>_v` and `<dir>/<dir>/v` name another package only when
//! the repository holds a package of that name; any other prefix (`rel/`,
//! `release-`, `log4j-`) names none.
use super::{ArtifactItem, CheckFailure, ListedTag, ReleaseTag, ReleaseTags, names_version};
use crate::providers::artifact::TagAbsence;
mod layout;

/// A repository's git tree at one commit: the path of every file in it, and
/// whether GitHub cut the recursive read short (`truncated`: past its
/// limit, so `paths` is only part of the tree).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TreeFiles {
    pub paths: Vec<String>,
    pub truncated: bool,
}

/// A pending read of a repository's tree at one commit.
pub type TreeFilesFuture<'a> =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<TreeFiles, CheckFailure>> + 'a>>;

/// What a release-tag lookup found for one version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TagSearch {
    /// No GitHub check ran: the call has no GitHub access, or the
    /// repository is not on GitHub.
    Unchecked,
    /// The release tag and the commit it names.
    Found(ReleaseTag),
    /// The release tag, taken by its name: its prefix has a package's shape
    /// (`<stem>-v`, a path) whose `stem` is none of this package's names, and
    /// the repository does not show `stem` as another package (`unattested`
    /// says what was checked).
    Named {
        release: ReleaseTag,
        stem: String,
        unattested: String,
    },
    /// The repository's whole tag list was read and no tag is this
    /// package's release: tags that could be (`differing`) name the version
    /// at different commits, so none is the release by its name alone; tags
    /// scoped to other packages (`foreign`) name it under another package's
    /// name. Both empty: no tag names the version.
    Unnamed {
        differing: Vec<ReleaseTag>,
        foreign: Vec<String>,
    },
    /// No tag was found and the lookup was cut short by `cause`: the
    /// candidate shapes were checked (`shapes_checked`) and the tag list
    /// could not be read or resolved, or not even the candidates could be
    /// checked. A tag may still name the version.
    Unlisted {
        cause: CheckFailure,
        shapes_checked: bool,
    },
}

impl TagSearch {
    /// The release tag, when the lookup found one.
    pub(crate) fn found(self) -> Option<ReleaseTag> {
        match self {
            Self::Found(release) | Self::Named { release, .. } => Some(release),
            Self::Unchecked | Self::Unnamed { .. } | Self::Unlisted { .. } => None,
        }
    }

    /// Tags that name the version at different commits: none is pinned.
    pub(super) fn ambiguous(&self) -> bool {
        matches!(self, Self::Unnamed { differing, .. } if !differing.is_empty())
    }

    /// Records on the row what the lookup left open: tags that name
    /// `version` at different commits or under another package's name, a tag
    /// taken by its name alone, or a lookup cut short (the warning then
    /// claims only what was checked).
    pub(super) fn note_on(&self, artifact: &mut ArtifactItem, version: &str) {
        match self {
            Self::Unnamed { differing, .. } if !differing.is_empty() => {
                let tags = differing
                    .iter()
                    .map(|release| format!("{} (commit {})", release.tag, release.commit))
                    .collect::<Vec<_>>()
                    .join(", ");
                artifact.disclosures.push(format!(
                    "{}: tags {tags} each name version {version} at different commits, so none is taken as its release tag.",
                    artifact.package_name
                ));
            }
            // The row's warning names them.
            Self::Unnamed { foreign, .. } if !foreign.is_empty() => {
                artifact.tag_absence = Some(TagAbsence::OtherPackagesOnly(foreign.clone()));
            }
            Self::Named {
                release,
                stem,
                unattested,
            } => artifact.disclosures.push(format!(
                "{}: tag {} is taken as its release tag: {stem} in its prefix is none of this package's names, and {unattested}.",
                artifact.package_name, release.tag
            )),
            Self::Unlisted {
                cause,
                shapes_checked,
            } => {
                let cause = cause.describe();
                artifact.tag_absence = Some(if *shapes_checked {
                    TagAbsence::ListUnread(cause)
                } else {
                    TagAbsence::Unchecked(cause)
                });
            }
            Self::Unchecked | Self::Found(_) | Self::Unnamed { .. } => {}
        }
    }
}

/// The names a package goes by in its repository's tags: the stems its own
/// candidate tags use ([`super::tag_stems`]), or, for a package that is its
/// repository, its own and the repository's name; and its directory.
pub(super) struct OwnNames {
    keys: Vec<String>,
    directory: Option<String>,
}

impl OwnNames {
    pub(super) fn new(repo: &str, package: &str, directory: Option<&str>) -> Self {
        let mut names = super::tag_stems(repo, package, directory);
        if names.is_empty() {
            let unscoped = package.rsplit('/').next().unwrap_or(package);
            names = vec![package.to_owned(), unscoped.to_owned(), repo.to_owned()];
        }
        Self {
            keys: names.iter().map(|name| key(name)).collect(),
            directory: directory
                .map(|directory| directory.trim_matches('/'))
                .filter(|directory| !directory.is_empty())
                .map(key),
        }
    }

    /// Whether a package-scoped prefix's `name` (a package name or a
    /// directory path) is this package's.
    fn owns(&self, name: &str) -> bool {
        let name = key(name);
        let leaf = name.rsplit('/').next().unwrap_or(&name);
        self.keys.iter().any(|own| *own == name || own == leaf)
            || self.directory.as_deref() == Some(name.as_str())
    }
}

fn key(name: &str) -> String {
    name.to_ascii_lowercase().replace('_', "-")
}

/// Whose release a listed tag's `prefix` (the tag before its version) says
/// it is.
#[derive(Clone, Debug, Eq, PartialEq)]
enum Scope {
    /// A package-scoped prefix naming this package.
    Own,
    /// `<name>@` naming another package: the `@` form is package grammar,
    /// no repository-wide release spelling.
    Foreign,
    /// A package's shape (`<stem>-v`, `<stem>_v`, a path of more than one
    /// segment) whose stem is none of this package's names. The shape is
    /// also a repository-wide release spelling (`release-v`, `releases/2021/`),
    /// so it is another package's only when the repository holds a package
    /// `stem` ([`layout::attest_stems`]).
    Stem(String),
    /// A prefix naming no package (`rel/`, `release-`, `v`).
    Generic,
}

/// The package-scoped shapes are those a package's own candidate tags take
/// (`<name>@`, `<name>-v`) and a module path's (`<dir>/<dir>/v`, a path of
/// more than one segment); any other prefix names no package.
fn scope_of(prefix: &str, own: &OwnNames) -> Scope {
    if let Some(name) = prefix.strip_suffix('@').filter(|name| !name.is_empty()) {
        return if own.owns(name) {
            Scope::Own
        } else {
            Scope::Foreign
        };
    }
    let stem = prefix
        .strip_suffix("-v")
        .or_else(|| prefix.strip_suffix("_v"))
        .or_else(|| {
            prefix
                .strip_suffix("/v")
                .or_else(|| prefix.strip_suffix('/'))
                .filter(|path| path.contains('/'))
        })
        .filter(|name| !name.is_empty());
    match stem {
        None => Scope::Generic,
        Some(name) if own.owns(name) => Scope::Own,
        Some(name) => Scope::Stem(name.to_owned()),
    }
}

/// A tag naming the version, resolved to its commit; `stem` when its prefix
/// has a package's shape this package's names do not account for.
struct Candidate {
    release: ReleaseTag,
    stem: Option<String>,
}

/// Each listed tag's commit (a lightweight tag's listed ref already names
/// it; GitHub's request admission bounds the reads), with what it carries.
/// A tag deleted since the list was read is no tag of the repository.
async fn resolve<T>(
    tags: &dyn ReleaseTags,
    owner: &str,
    repo: &str,
    listed: Vec<(ListedTag, T)>,
) -> Result<Vec<(ReleaseTag, T)>, CheckFailure> {
    let answers = futures_util::future::join_all(listed.iter().map(|(tag, _)| async {
        match &tag.commit {
            Some(commit) => Ok(Some(commit.clone())),
            None => tags.tag_commit(owner, repo, &tag.name).await,
        }
    }))
    .await;
    let mut resolved = Vec::new();
    for ((tag, carried), answer) in listed.into_iter().zip(answers) {
        if let Some(commit) = answer? {
            resolved.push((
                ReleaseTag {
                    tag: tag.name,
                    commit,
                },
                carried,
            ));
        }
    }
    Ok(resolved)
}

/// Orders tags as a pin chooses among them: the shortest name, then the
/// first in order.
fn by_pin_order(left: &ReleaseTag, right: &ReleaseTag) -> std::cmp::Ordering {
    left.tag
        .len()
        .cmp(&right.tag.len())
        .then_with(|| left.tag.cmp(&right.tag))
}

/// The release `releases` (in pin order) name: the first, when they all
/// name one commit.
fn one_release<'a>(releases: impl IntoIterator<Item = &'a ReleaseTag>) -> Option<&'a ReleaseTag> {
    let mut releases = releases.into_iter();
    let first = releases.next()?;
    releases
        .all(|release| release.commit.eq_ignore_ascii_case(&first.commit))
        .then_some(first)
}

/// The outcome of tags (in pin order) that may be this package's release.
fn settle(releases: Vec<ReleaseTag>, foreign: Vec<String>) -> TagSearch {
    match one_release(&releases) {
        Some(release) => TagSearch::Found(release.clone()),
        None => TagSearch::Unnamed {
            differing: releases,
            foreign,
        },
    }
}

/// The release tag of `version` among every tag of `owner/repo`: each tag
/// that names the version ([`names_version`]) is sorted by what its prefix
/// names ([`scope_of`]). This package's own tags, else the tags naming no
/// package, are resolved to their commits: when they all name one commit,
/// that is the release (the shortest name, then the first in order, is
/// pinned); when they name different commits, they are listed, never chosen
/// between. A tag at another package's name (`other@`) is never pinned. A
/// tag whose prefix only has a package's shape (`other-v`) is another
/// package's when the repository's tree at its commit holds a package of
/// that name; that tree is read only when it decides the outcome. Otherwise
/// the tag names no package, and a pin of it says so ([`TagSearch::Named`]).
/// `shapes_checked` says whether the candidate shapes were checked before,
/// for a lookup the list cannot settle.
pub(super) async fn listed_release_tag(
    tags: &dyn ReleaseTags,
    owner: &str,
    repo: &str,
    version: &str,
    own: &OwnNames,
    shapes_checked: bool,
) -> TagSearch {
    let unlisted = |cause| TagSearch::Unlisted {
        cause,
        shapes_checked,
    };
    let listed = match tags.tag_refs(owner, repo).await {
        Ok(listed) => listed,
        Err(cause) => return unlisted(cause),
    };
    let (mut own_tags, mut named, mut foreign) = (Vec::new(), Vec::new(), Vec::new());
    for tag in listed {
        let scope = match tag.name.strip_suffix(version) {
            Some(prefix) if names_version(&tag.name, version) => scope_of(prefix, own),
            _ => continue,
        };
        match scope {
            Scope::Own => own_tags.push((tag, ())),
            Scope::Generic => named.push((tag, None)),
            Scope::Stem(stem) => named.push((tag, Some(stem))),
            Scope::Foreign => foreign.push(tag.name),
        }
    }
    if !own_tags.is_empty() {
        return match resolve(tags, owner, repo, own_tags).await {
            Ok(resolved) => {
                let mut releases: Vec<ReleaseTag> =
                    resolved.into_iter().map(|(release, ())| release).collect();
                releases.sort_by(by_pin_order);
                settle(releases, sorted(foreign))
            }
            Err(cause) => unlisted(cause),
        };
    }
    let mut candidates: Vec<Candidate> = match resolve(tags, owner, repo, named).await {
        Ok(resolved) => resolved
            .into_iter()
            .map(|(release, stem)| Candidate { release, stem })
            .collect(),
        Err(cause) => return unlisted(cause),
    };
    candidates.sort_by(|left, right| by_pin_order(&left.release, &right.release));
    // The stems decide nothing when the tags naming no package pin the same
    // release with them as without them: no tree is read.
    let unscoped = one_release(
        candidates
            .iter()
            .filter(|candidate| candidate.stem.is_none())
            .map(|candidate| &candidate.release),
    );
    let with_stems = one_release(candidates.iter().map(|candidate| &candidate.release));
    let decided = unscoped.is_some() && unscoped == with_stems;
    if decided || candidates.iter().all(|candidate| candidate.stem.is_none()) {
        let releases = candidates
            .into_iter()
            .map(|candidate| candidate.release)
            .collect();
        return settle(releases, sorted(foreign));
    }
    let (releases, unattested) =
        layout::attest_stems(tags, owner, repo, candidates, &mut foreign).await;
    match settle(releases, sorted(foreign)) {
        TagSearch::Found(release) => {
            match unattested.into_iter().find(|stem| stem.tag == release.tag) {
                Some(layout::Unattested { stem, checked, .. }) => TagSearch::Named {
                    release,
                    stem,
                    unattested: checked,
                },
                None => TagSearch::Found(release),
            }
        }
        other => other,
    }
}

fn sorted(mut names: Vec<String>) -> Vec<String> {
    names.sort();
    names.dedup();
    names
}
