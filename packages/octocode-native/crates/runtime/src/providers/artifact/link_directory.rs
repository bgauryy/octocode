//! Where a source link's ref ends and its directory starts.
//!
//! A `/tree/<ref>/<path>` link cannot say: a ref may contain `/`
//! (`release/1.x`, `v2/main`, `users/x`). The split comes from evidence, in
//! order: the registry's own directory (the link is then irrelevant), the ref
//! the row already pins, and the repository confirming the link's first
//! segment as a whole ref. With none of them the row has no directory and says so.
use super::http::RegistryClient;
use super::release_ref::{CommitCheck, commit_check};
use super::types::LinkBelowRef;
use super::{ArtifactError, ArtifactItem};
use octocode_contract_types::ErrorCode;
use octocode_io::budget::BudgetStop;

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
    let unchecked = if answers.iter().any(Option::is_none) {
        Some(unchecked_reason(client)?)
    } else {
        None
    };
    for (index, repository, below) in &unplaced {
        let first = below.segments[0].as_str();
        let position = asked
            .iter()
            .position(|key| *key == (repository.as_str(), first));
        let answer = position.and_then(|position| answers[position]);
        let artifact = &mut artifacts[*index];
        match answer {
            Some(check @ (CommitCheck::Found | CommitCheck::FoundAfterRedirect)) => {
                artifact.repository_directory = Some(below.segments[1..].join("/"));
                artifact.note_repository(check);
            }
            Some(CommitCheck::RepositoryMissing) => {
                artifact.note_repository(CommitCheck::RepositoryMissing);
            }
            Some(CommitCheck::Missing) => artifact.disclosures.push(format!(
                "{}: the source link {} points below a ref that may contain a slash, and the repository does not confirm {first} as the whole ref, so its directory is not derived.",
                artifact.package_name, below.link
            )),
            None => artifact.disclosures.push(format!(
                "{}: the source link {} points below a ref that may contain a slash, and {first} could not be checked as the whole ref ({}), so its directory is not derived.",
                artifact.package_name,
                below.link,
                unchecked.as_deref().unwrap_or("no check ran")
            )),
        }
    }
    Ok(())
}

/// Why a repository check gave no answer. A cancelled call fails instead.
fn unchecked_reason(client: &RegistryClient<'_>) -> Result<String, ArtifactError> {
    if client.tags.is_none() {
        return Ok("GitHub is not available to this call".to_owned());
    }
    match client.budget.check() {
        Err(BudgetStop::Cancelled) => Err(ArtifactError::new(
            ErrorCode::Cancelled,
            "Artifact registry request was cancelled.",
        )),
        Err(BudgetStop::Deadline) => Ok("the call's deadline passed".to_owned()),
        Ok(()) => Ok(
            "GitHub gave no answer: a rate limit, an authorization or a network failure".to_owned(),
        ),
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
