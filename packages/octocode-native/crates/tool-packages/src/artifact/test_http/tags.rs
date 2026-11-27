//! The GitHub release-tag double artifact tests share.
use super::super::release_ref::{
    CheckFailure, CommitCheck, CommitFuture, ReleaseTags, TagCommitFuture, TagCommitsFuture,
    TagRefsFuture, TreeFiles, TreeFilesFuture,
};
use octocode_github::{TagObject, TagRef};
use std::sync::Mutex;

/// GitHub's answer to every release-ref check: the refs that exist
/// (`owner/repo@ref`), or one fixed answer (`None`: the check could not run).
/// A repository's tag list is its known refs (none for a fixed answer):
/// `tag=<sha>` lists as a lightweight tag naming `<sha>`, a bare `tag` as an
/// annotated one, whose commit `tag_commits` answers as `tag_commit` does.
/// A tree read at a commit lists the files set for it (`with_tree`), none
/// otherwise. Records every check; a tag-list read as `owner/repo#tags`, a
/// tree read as `owner/repo^tree@<commit>`.
pub struct KnownTags {
    answer: Answer,
    /// What every commit check answers about the repository, when set.
    repository: Option<CommitCheck>,
    /// Why every commit check fails, when set.
    failure: Option<CheckFailure>,
    /// Why every tag-list read fails, when set.
    listing_failure: Option<CheckFailure>,
    /// The files of the tree at each commit set.
    trees: Vec<(&'static str, Vec<&'static str>)>,
    seen: Mutex<Vec<String>>,
}

enum Answer {
    Known(Vec<&'static str>),
    Always(Option<bool>),
}

impl KnownTags {
    pub fn new(known: Vec<&'static str>) -> Self {
        Self {
            answer: Answer::Known(known),
            repository: None,
            failure: None,
            listing_failure: None,
            trees: Vec::new(),
            seen: Mutex::new(Vec::new()),
        }
    }

    pub fn always(exists: Option<bool>) -> Self {
        Self {
            answer: Answer::Always(exists),
            repository: None,
            failure: None,
            listing_failure: None,
            trees: Vec::new(),
            seen: Mutex::new(Vec::new()),
        }
    }

    /// Every commit check answers `check` whatever the commit.
    pub fn with_repository(mut self, check: CommitCheck) -> Self {
        self.repository = Some(check);
        self
    }

    /// Why every commit check fails: the set cause, or the one a `None`
    /// always-answer stands for; `None` when checks answer.
    pub fn commit_check_failure(&self) -> Option<CheckFailure> {
        match (self.failure.clone(), &self.answer) {
            (Some(failure), _) => Some(failure),
            (None, Answer::Always(None)) if self.repository.is_none() => {
                Some(CheckFailure::Request)
            }
            _ => None,
        }
    }

    /// Every commit check fails with `failure`.
    pub fn with_failure(mut self, failure: CheckFailure) -> Self {
        self.failure = Some(failure);
        self
    }

    /// Every tag-list read fails with `failure`.
    pub fn with_listing_failure(mut self, failure: CheckFailure) -> Self {
        self.listing_failure = Some(failure);
        self
    }

    /// The tree at `commit` holds `files`.
    pub fn with_tree(mut self, commit: &'static str, files: Vec<&'static str>) -> Self {
        self.trees.push((commit, files));
        self
    }

    fn record(&self, name: String) {
        self.seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(name);
    }

    /// Every check, as `owner/repo@ref`, in order.
    pub fn seen(&self) -> Vec<String> {
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

    fn tag_refs<'a>(&'a self, owner: &'a str, repo: &'a str) -> TagRefsFuture<'a> {
        let prefix = format!("{owner}/{repo}@");
        let names = match (&self.listing_failure, &self.answer) {
            (Some(failure), _) => Err(failure.clone()),
            (None, Answer::Known(known)) => Ok(known
                .iter()
                .filter_map(|entry| entry.strip_prefix(prefix.as_str()))
                .map(|tag| match tag.split_once('=') {
                    Some((name, commit)) => TagRef {
                        name: name.to_owned(),
                        object: Some(TagObject::Tagged(commit.to_owned())),
                    },
                    None => TagRef {
                        name: tag.to_owned(),
                        object: Some(TagObject::Annotated(format!("object-of-{tag}"))),
                    },
                })
                .collect()),
            (None, Answer::Always(Some(_))) => Ok(Vec::new()),
            (None, Answer::Always(None)) => Err(CheckFailure::Request),
        };
        self.record(format!("{owner}/{repo}#tags"));
        Box::pin(async move { names })
    }

    fn tag_commits<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        tags: &'a [TagRef],
    ) -> TagCommitsFuture<'a> {
        Box::pin(async move {
            let mut commits = Vec::new();
            for tag in tags {
                commits.push(match &tag.object {
                    Some(TagObject::Tagged(commit)) => Some(commit.clone()),
                    _ => self.tag_commit(owner, repo, &tag.name).await?,
                });
            }
            Ok(commits)
        })
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

    fn tree_files<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        commit: &'a str,
    ) -> TreeFilesFuture<'a> {
        let files = match &self.answer {
            Answer::Always(None) => Err(CheckFailure::Request),
            _ => Ok(TreeFiles {
                paths: self
                    .trees
                    .iter()
                    .filter(|(at, _)| *at == commit)
                    .flat_map(|(_, files)| files.iter().map(|file| (*file).to_owned()))
                    .collect(),
                truncated: false,
            }),
        };
        self.record(format!("{owner}/{repo}^tree@{commit}"));
        Box::pin(async move { files })
    }
}
