//! The GitHub release-tag double artifact tests share.
use super::super::release_ref::{CommitCheck, CommitFuture, ReleaseTags, TagCommitFuture};
use std::sync::Mutex;

/// GitHub's answer to every release-ref check: the refs that exist
/// (`owner/repo@ref`), or one fixed answer (`None`: the check could not run).
/// Records every check.
pub(crate) struct KnownTags {
    answer: Answer,
    /// What every commit check answers about the repository, when set.
    repository: Option<CommitCheck>,
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
            seen: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn always(exists: Option<bool>) -> Self {
        Self {
            answer: Answer::Always(exists),
            repository: None,
            seen: Mutex::new(Vec::new()),
        }
    }

    /// Every commit check answers `check` whatever the commit.
    pub(crate) fn with_repository(mut self, check: CommitCheck) -> Self {
        self.repository = Some(check);
        self
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
            Answer::Known(known) => Some(known.iter().find_map(|entry| {
                if *entry == name {
                    Some(plain.clone())
                } else {
                    entry.strip_prefix(named.as_str()).map(str::to_owned)
                }
            })),
            Answer::Always(exists) => exists.map(|exists| exists.then(|| plain.clone())),
        };
        self.seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(name);
        Box::pin(async move { commit })
    }

    fn commit_check<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        commit: &'a str,
    ) -> CommitFuture<'a> {
        let name = format!("{owner}/{repo}@{commit}");
        let check = self.repository.or_else(|| match &self.answer {
            Answer::Known(known) => Some(if known.contains(&name.as_str()) {
                CommitCheck::Found
            } else {
                CommitCheck::Missing
            }),
            Answer::Always(exists) => exists.map(|exists| {
                if exists {
                    CommitCheck::Found
                } else {
                    CommitCheck::Missing
                }
            }),
        });
        self.seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(name);
        Box::pin(async move { check })
    }
}
