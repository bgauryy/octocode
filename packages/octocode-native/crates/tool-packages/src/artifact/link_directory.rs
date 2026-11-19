//! Where a source link's ref ends and its directory starts.
//!
//! A `/tree/<ref>/<path>` link cannot say: a ref may contain `/`
//! (`release/1.x`, `v2/main`, `users/x`). The split comes from evidence, in
//! order: the registry's own directory (the link is then irrelevant), the ref
//! the row already pins, and the repository confirming the link's first
//! segment as a whole ref. With none of them the row has no directory and says so.
//!
//! A link the registry keeps for the package rather than for the version (a
//! crate's) names where the package lives now, which an older release may not
//! ([`place_at_pinned_ref`]).
use super::http::RegistryClient;
use super::release_ref::{CheckFailure, CommitCheck, commit_check};
use super::types::LinkBelowRef;
use super::util::github_slug;
use super::{ArtifactError, ArtifactItem};
use octocode_contract_types::ErrorCode;
use octocode_io::budget::BudgetStop;

/// Places the directory of a row whose source link is the package's current
/// one, not its version's (crates.io keeps one link per crate), in the
/// repository's tree at the commit the row pins: ripgrep's `ignore` 0.4.7 sits
/// in `ignore/` at its release, where today's link says `crates/ignore`. The
/// directory is the first that holds one of `manifests` there of the link's path
/// below its ref (each split, longest first, since a ref may contain `/`)
/// and the package's name at the repository root (`grep-searcher/` for the
/// link's `crates/searcher`). None holding it leaves the row without a
/// directory and says so; a tree that could not be read, or was cut short
/// before any holds it, says that instead. A row with no pinned ref, or no
/// such link, is left to [`settle`].
pub(crate) async fn place_at_pinned_ref(
    artifact: &mut ArtifactItem,
    manifests: &[&str],
    client: &RegistryClient<'_>,
) {
    if artifact.repository_directory.is_some() {
        return;
    }
    let (Some(reference), Some((owner, repo)), Some(tags)) = (
        artifact.source_ref.clone(),
        artifact.repository.as_deref().and_then(github_slug),
        client.tags,
    ) else {
        return;
    };
    let Some(below) = artifact.link_below_ref.take() else {
        return;
    };
    let candidates = pinned_candidates(&below.segments, &artifact.package_name);
    let package = &artifact.package_name;
    let tried = candidates
        .iter()
        .map(|directory| format!("{directory}/"))
        .collect::<Vec<_>>()
        .join(", ");
    let manifest = manifests.join(" or ");
    match tags.tree_files(&owner, &repo, &reference).await {
        Ok(tree) => {
            let held = candidates.iter().find(|directory| {
                manifests
                    .iter()
                    .any(|manifest| tree.paths.contains(&format!("{directory}/{manifest}")))
            });
            match held {
                Some(directory) => artifact.repository_directory = Some(directory.clone()),
                None if tree.truncated => artifact.disclosures.push(format!(
                    "{package}: the source link {} is the package's current one, and the repository's tree at {reference} was cut short before any of {tried} showed {manifest}, so its directory at this release is not given.",
                    below.link
                )),
                None => artifact.disclosures.push(format!(
                    "{package}: the source link {} is the package's current one, and at {reference} none of {tried} holds {manifest}, so its directory at this release is not given.",
                    below.link
                )),
            }
        }
        Err(failure) => {
            if let Some(cause) = unchecked_cause(failure, client) {
                artifact.disclosures.push(format!(
                    "{package}: the source link {} is the package's current one, and the repository's tree at {reference} could not be read ({cause}), so its directory at this release is not given.",
                    below.link
                ));
            }
        }
    }
}

/// The directories a pinned tree is searched for, in order: the link's path
/// below each possible end of its ref (longest first), then the package's
/// name at the root.
fn pinned_candidates(segments: &[String], package: &str) -> Vec<String> {
    let mut candidates: Vec<String> = (1..segments.len())
        .map(|start| segments[start..].join("/"))
        .collect();
    let named = package.rsplit('/').next().unwrap_or(package).to_owned();
    if !named.is_empty() && !candidates.contains(&named) {
        candidates.push(named);
    }
    candidates
}

/// Places the directory of every row whose source link points below a ref.
/// The pinned ref places what it can; the rest ask the repository once per
/// distinct (repository, first segment) of the page, all together: the GitHub
/// transport's own admission bounds how many run at once. A check that could
/// not run leaves its rows unplaced and says why; only a cancelled call fails.
pub(crate) async fn settle(
    artifacts: &mut [ArtifactItem],
    client: &RegistryClient<'_>,
) -> Result<(), ArtifactError> {
    let mut unplaced: Vec<(usize, String, LinkBelowRef)> = Vec::new();
    for (index, artifact) in artifacts.iter_mut().enumerate() {
        let Some(below) = artifact.link_below_ref.take() else {
            continue;
        };
        if artifact.repository_directory.is_some() {
            continue;
        }
        match below_pinned_ref(artifact.source_ref.as_deref(), &below.segments) {
            Some(directory) => {
                artifact.repository_directory =
                    (!directory.is_empty()).then(|| directory.join("/"));
            }
            None => {
                if let Some(repository) = artifact.repository.clone() {
                    unplaced.push((index, repository, below));
                }
            }
        }
    }
    let mut asked: Vec<(&str, &str)> = Vec::new();
    for (_, repository, below) in &unplaced {
        let key = (repository.as_str(), below.segments[0].as_str());
        if !asked.contains(&key) {
            asked.push(key);
        }
    }
    let answers = futures_util::future::join_all(
        asked
            .iter()
            .map(|(repository, first)| commit_check(repository, first, client)),
    )
    .await;
    for (index, repository, below) in &unplaced {
        let first = below.segments[0].as_str();
        // Every unplaced row's key was asked above.
        let Some(answer) = asked.iter().zip(&answers).find_map(|(key, answer)| {
            (*key == (repository.as_str(), first)).then(|| answer.clone())
        }) else {
            continue;
        };
        let artifact = &mut artifacts[*index];
        match answer {
            Ok(check @ (CommitCheck::Found | CommitCheck::FoundAfterRedirect { .. })) => {
                artifact.repository_directory = Some(below.segments[1..].join("/"));
                artifact.note_repository(check);
            }
            Ok(CommitCheck::RepositoryMissing) => {
                artifact.note_repository(CommitCheck::RepositoryMissing);
            }
            Ok(CommitCheck::Missing) => artifact.disclosures.push(format!(
                "{}: the source link {} points below a ref that may contain a slash, and the repository does not confirm {first} as the whole ref, so its directory is not derived.",
                artifact.package_name, below.link
            )),
            Err(failure) => {
                let Some(cause) = unchecked_cause(failure, client) else {
                    return Err(ArtifactError::new(
                        ErrorCode::Cancelled,
                        "Artifact registry request was cancelled.",
                    ));
                };
                artifact.disclosures.push(format!(
                    "{}: the source link {} points below a ref that may contain a slash, and {first} could not be checked as the whole ref ({cause}), so its directory is not derived.",
                    artifact.package_name, below.link
                ));
            }
        }
    }
    Ok(())
}

/// Why a repository check gave no answer: the call's own end when its
/// budget is spent, else the check's cause; `None` when the call was
/// cancelled, which fails it instead.
fn unchecked_cause(failure: CheckFailure, client: &RegistryClient<'_>) -> Option<String> {
    match client.budget.check() {
        Err(BudgetStop::Cancelled) => None,
        Err(BudgetStop::Deadline) => Some("the call's deadline passed".to_owned()),
        Ok(()) => Some(failure.describe()),
    }
}

/// The segments a link leaves below the ref the row pins, when they start with
/// that ref's own segments (none left: the link ends at the pinned ref);
/// `None` when the pinned ref does not place the link.
fn below_pinned_ref<'a>(source_ref: Option<&str>, segments: &'a [String]) -> Option<&'a [String]> {
    let pinned = source_ref?.split('/');
    let length = pinned.clone().count();
    if segments.len() < length || !pinned.zip(segments).all(|(own, link)| own == link) {
        return None;
    }
    Some(&segments[length..])
}

#[cfg(test)]
mod tests;
