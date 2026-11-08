//! The GitHub release-tag double artifact tests share.
use super::super::release_ref::{
    CheckFailure, CommitCheck, CommitFuture, ReleaseTags, TagCommitFuture, TagNamesFuture,
};
use std::sync::Mutex;

/// GitHub's answer to every release-ref check: the refs that exist
/// (`owner/repo@ref`), or one fixed answer (`None`: the check could not run).
/// A repository's tag list is its known refs (none for a fixed answer).
/// Records every check; a tag-list read as `owner/repo#tags`.
pub(crate) struct KnownTags {
    answer: Answer,
    /// What every commit check answers about the repository, when set.
    repository: Option<CommitCheck>,
    /// Why every commit check fails, when set.
    failure: Option<CheckFailure>,
    /// Why every tag-list read fails, when set.
    listing_failure: Option<CheckFailure>,
    seen: Mutex<Vec<String>>,
}

enum Answer {
    Known(Vec<&'static str>),
    Always(Option<bool>),
}

impl KnownTags {
    pub(crate) fn new(known: Vec<&'static str>) -> Self {
        Self {
            answer: Answer::Known(known),
            repository: None,
            failure: None,
            listing_failure: None,
            seen: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn always(exists: Option<bool>) -> Self {
        Self {
            answer: Answer::Always(exists),
            repository: None,
            failure: None,
            listing_failure: None,
            seen: Mutex::new(Vec::new()),
        }
    }

    /// Every commit check answers `check` whatever the commit.
    pub(crate) fn with_repository(mut self, check: CommitCheck) -> Self {
        self.repository = Some(check);
        self
    }

    /// Why every commit check fails: the set cause, or the one a `None`
    /// always-answer stands for; `None` when checks answer.
    pub(crate) fn commit_check_failure(&self) -> Option<CheckFailure> {
        match (self.failure.clone(), &self.answer) {
            (Some(failure), _) => Some(failure),
            (None, Answer::Always(None)) if self.repository.is_none() => {
                Some(CheckFailure::Request)
            }
            _ => None,
        }
    }

    /// Every commit check fails with `failure`.
    pub(crate) fn with_failure(mut self, failure: CheckFailure) -> Self {
        self.failure = Some(failure);
        self
    }

    /// Every tag-list read fails with `failure`.
    pub(crate) fn with_listing_failure(mut self, failure: CheckFailure) -> Self {
        self.listing_failure = Some(failure);
        self
    }

    fn record(&self, name: String) {
        self.seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(name);
    }

    /// Every check, as `owner/repo@ref`, in order.
    pub(crate) fn seen(&self) -> Vec<String> {
        self.seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl ReleaseTags for KnownTags {
    /// A known tag `a/b@tag` names `commit-of-tag`; `a/b@tag=<sha>` names `<sha>`.
    fn tag_commit<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        tag: &'a str,
    ) -> TagCommitFuture<'a> {
        let name = format!("{owner}/{repo}@{tag}");
        let named = format!("{name}=");
        let plain = format!("commit-of-{tag}");
        let commit = match &self.answer {
            Answer::Known(known) => Ok(known.iter().find_map(|entry| {
                if *entry == name {
                    Some(plain.clone())
                } else {
                    entry.strip_prefix(named.as_str()).map(str::to_owned)
                }
            })),
            Answer::Always(Some(exists)) => Ok(exists.then(|| plain.clone())),
            Answer::Always(None) => Err(CheckFailure::Request),
        };
        self.record(name);
        Box::pin(async move { commit })
    }

    fn tag_names<'a>(&'a self, owner: &'a str, repo: &'a str) -> TagNamesFuture<'a> {
        let prefix = format!("{owner}/{repo}@");
        let names = match (&self.listing_failure, &self.answer) {
            (Some(failure), _) => Err(failure.clone()),
            (None, Answer::Known(known)) => Ok(known
                .iter()
                .filter_map(|entry| entry.strip_prefix(prefix.as_str()))
                .map(|tag| tag.split_once('=').map_or(tag, |(tag, _)| tag).to_owned())
                .collect()),
            (None, Answer::Always(Some(_))) => Ok(Vec::new()),
            (None, Answer::Always(None)) => Err(CheckFailure::Request),
        };
        self.record(format!("{owner}/{repo}#tags"));
        Box::pin(async move { names })
    }

    fn commit_check<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        commit: &'a str,
    ) -> CommitFuture<'a> {
        let name = format!("{owner}/{repo}@{commit}");
        let check = match (self.failure.clone(), self.repository.clone(), &self.answer) {
            (Some(failure), _, _) => Err(failure),
            (None, Some(check), _) => Ok(check),
            (None, None, Answer::Known(known)) => Ok(if known.contains(&name.as_str()) {
                CommitCheck::Found
            } else {
                CommitCheck::Missing
            }),
            (None, None, Answer::Always(Some(true))) => Ok(CommitCheck::Found),
            (None, None, Answer::Always(Some(false))) => Ok(CommitCheck::Missing),
            (None, None, Answer::Always(None)) => Err(CheckFailure::Request),
        };
        self.record(name);
        Box::pin(async move { check })
    }
}
