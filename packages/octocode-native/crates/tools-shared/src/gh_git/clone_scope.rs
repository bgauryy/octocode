//! What a ghCloneRepo checkout holds, as the checkout records it: the
//! repository, commit, history depth and sparse scope, in one file of the
//! checkout's git administration directory (never its work tree, so the
//! checkout stays clean). ghCloneRepo writes it ([`write_record`]); the
//! local tools read it ([`clone_scope`]) to say that a missing path lies
//! outside a sparse checkout and to name the clone that checks it out.
//!
//! A checkout without a record (made before it existed, or not by
//! ghCloneRepo) has an unknown scope: [`clone_scope`] answers `None`, never
//! "the whole repository".
use octocode_contract_types::GhCloneRepoQuery;
use octocode_runtime_base::response::result::Continuation;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Component, Path, PathBuf};

/// The record's file name in the checkout's git administration directory.
const RECORD_FILE: &str = "octocode-clone.json";

/// What one checkout holds, as ghCloneRepo served it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneRecord {
    pub owner: String,
    pub repo: String,
    pub commit_sha: String,
    /// The sparse scope on disk (the requested paths that exist at the
    /// commit); `None` for a full checkout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paths: Option<Vec<String>>,
    /// Whether the files directly at the repository root are on disk beside
    /// a sparse scope (cone mode); always true for a full checkout.
    pub root_files: bool,
    pub history_depth: u64,
}

/// Write `record` for the checkout whose git administration directory is
/// `git_dir`.
pub fn write_record(git_dir: &Path, record: &CloneRecord) -> std::io::Result<()> {
    let text = serde_json::to_vec(record).map_err(std::io::Error::other)?;
    std::fs::write(git_dir.join(RECORD_FILE), text)
}

/// The record of the checkout whose git administration directory is
/// `git_dir`; `None` when it has none or it does not parse.
pub fn read_record(git_dir: &Path) -> Option<CloneRecord> {
    let text = std::fs::read(git_dir.join(RECORD_FILE)).ok()?;
    serde_json::from_slice(&text).ok()
}

/// A sparse ghCloneRepo checkout and the scope it holds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloneScope {
    root: PathBuf,
    record: CloneRecord,
    paths: Vec<String>,
}

/// The sparse ghCloneRepo checkout that holds `path` (the checkout root or
/// any path under it, existing or not): the nearest checkout above it, when
/// that checkout records a sparse scope. `None` outside any checkout, for a
/// full checkout, and for a checkout whose record is missing or unreadable
/// (its scope is unknown). Cost: one metadata read per ancestor up to the
/// nearest repository, then one small file read.
pub fn clone_scope(path: &Path) -> Option<CloneScope> {
    let (root, git_dir) = path.ancestors().find_map(|dir| {
        octocode_engine::search::walk::checkout_git_dir(dir).map(|git| (dir.to_path_buf(), git))
    })?;
    CloneScope::of(root, read_record(&git_dir)?)
}

impl CloneScope {
    /// The scope of the checkout at `root` that `record` describes; `None`
    /// for a full checkout.
    pub fn of(root: PathBuf, record: CloneRecord) -> Option<Self> {
        let paths = record
            .paths
            .as_ref()?
            .iter()
            .map(|path| path.trim_end_matches('/').to_owned())
            .collect();
        Some(Self {
            root,
            record,
            paths,
        })
    }

    /// The checkout directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The repo-relative paths on disk, as the clone row's
    /// `location.paths` names them.
    pub fn paths(&self) -> &[String] {
        &self.paths
    }

    /// Whether the checkout leaves out a `path` (under [`Self::root`]) that
    /// is not on disk: were it at the commit, it would lie under no scope
    /// path and on no directory leading to one. A file beside a cone scope
    /// (at the root or directly in a directory leading to it) would be on
    /// disk, so a missing name there can only be a directory, which the
    /// checkout leaves out too. A missing path inside the scope is missing at
    /// the commit itself: not excluded.
    pub fn excludes(&self, path: &Path) -> bool {
        let Some(relative) = self.relative(path).filter(|relative| !relative.is_empty()) else {
            return false;
        };
        let inside = |scope: &str| {
            relative == scope
                || relative
                    .strip_prefix(scope)
                    .is_some_and(|rest| rest.starts_with('/'))
        };
        let on_the_way = |scope: &str| {
            scope
                .strip_prefix(relative.as_str())
                .is_some_and(|rest| rest.starts_with('/'))
        };
        !self
            .paths
            .iter()
            .any(|scope| inside(scope) || on_the_way(scope))
    }

    /// The related ghCloneRepo call that checks out `path` (under
    /// [`Self::root`]) beside this checkout's scope: same repository,
    /// commit and history depth. `None` when `path` is not under the root.
    pub fn reclone(&self, path: &Path) -> Option<Value> {
        let relative = self
            .relative(path)
            .filter(|relative| !relative.is_empty())?;
        let mut paths = self.paths.clone();
        if !paths.contains(&relative) {
            paths.push(relative);
        }
        let query: GhCloneRepoQuery = serde_json::from_value(json!({
            "owner": self.record.owner,
            "repo": self.record.repo,
            "ref": self.record.commit_sha,
            "paths": paths,
            "historyDepth": self.record.history_depth,
        }))
        .ok()?;
        Some(Continuation::related_query(query).build())
    }

    /// The error of a missing `display` path this checkout excludes
    /// ([`Self::excludes`]): it names the scope and the related[] clone
    /// ([`Self::reclone`]) that checks the path out.
    pub fn outside_message(&self, display: &str) -> String {
        format!(
            "Path does not exist: {display}. It is outside this checkout's sparse paths ({}): the related[] ghCloneRepo call checks it out with them, or read it with ghGetFileContent.",
            self.listed()
        )
    }

    /// The coverage note of a row that reads this checkout: what is on disk,
    /// so a miss is no absence from the repository.
    pub fn coverage_warning(&self) -> String {
        let root = if self.record.root_files {
            " and the root's files"
        } else {
            ""
        };
        format!(
            "This checkout is sparse: only {}{root} are on disk; a miss here says nothing of other paths.",
            self.listed()
        )
    }

    fn listed(&self) -> String {
        self.paths.join(", ")
    }

    /// `path` relative to the root, `/`-separated; `None` when it is not
    /// under the root or climbs out of it.
    fn relative(&self, path: &Path) -> Option<String> {
        let rest = path.strip_prefix(&self.root).ok()?;
        let mut segments = Vec::new();
        for component in rest.components() {
            match component {
                Component::Normal(segment) => segments.push(segment.to_str()?.to_owned()),
                Component::CurDir => {}
                _ => return None,
            }
        }
        Some(segments.join("/"))
    }
}

#[cfg(test)]
mod tests;
