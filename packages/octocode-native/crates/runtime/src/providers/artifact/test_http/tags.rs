//! The GitHub release-tag double artifact tests share.
use super::super::release_ref::{ReleaseTags, TagCommitFuture, TagFuture};
use std::sync::Mutex;

/// GitHub's answer to every release-ref check: the refs that exist
/// (`owner/repo@ref`), or one fixed answer (`None`: the check could not run).
/// Records every check.
pub(crate) struct KnownTags {
    answer: Answer,
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
            seen: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn always(exists: Option<bool>) -> Self {
        Self {
            answer: Answer::Always(exists),
            seen: Mutex::new(Vec::new()),
        }
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

    fn commit_exists<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        commit: &'a str,
    ) -> TagFuture<'a> {
        self.known(owner, repo, commit)
    }
}

impl KnownTags {
    fn known<'a>(&'a self, owner: &'a str, repo: &'a str, reference: &'a str) -> TagFuture<'a> {
        let name = format!("{owner}/{repo}@{reference}");
        let exists = match &self.answer {
            Answer::Known(known) => Some(known.contains(&name.as_str())),
            Answer::Always(exists) => *exists,
        };
        self.seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(name);
        Box::pin(async move { exists })
    }
}
