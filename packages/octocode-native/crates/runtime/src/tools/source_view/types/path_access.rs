//! Path validation for a read: what a validated read names, and why a
//! path failed.
use octocode_contract_types::ErrorCode;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedRead {
    pub canonical: PathBuf,
    pub display: String,
    /// What a row names the read file by: the canonical path relative to
    /// the workspace when inside it, else the canonical path; a path read
    /// through a symlink keeps the requested name.
    pub named: String,
    /// The file a path read through a symlink resolved to, named as a
    /// canonical path is.
    pub target: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PathFailure {
    /// The row's `errorCode`: the policy's shared code, else the access code.
    pub code: ErrorCode,
    pub message: String,
    pub safe_path: Option<String>,
    pub resource_missing: bool,
    /// The missing path lies in a sparse git checkout, so it may exist
    /// upstream outside the checked-out paths.
    pub sparse_checkout: bool,
    /// The path names something other than a regular file.
    pub not_a_file: Option<NotAFile>,
    /// Where a missing file's listing searches: the workspace root (`.`)
    /// for a path inside it, else the allowed root that holds the path,
    /// else its closest admitted ancestor directory.
    pub search_root: Option<String>,
    /// A missing path's `viewTree` lead ([`missing_path_lead`]): the tree
    /// of its nearest existing admitted parent, where a mistyped name's
    /// siblings show.
    ///
    /// [`missing_path_lead`]: crate::response::result::missing_path_lead
    pub view_tree: Option<serde_json::Value>,
    /// The path a missing file's name was likely mistyped for, written as
    /// the request wrote its directory: a readable file in the same
    /// directory ([`close_name`]).
    ///
    /// [`close_name`]: crate::tools::leads::find_file::close_name
    pub corrected: Option<String>,
}
/// What a path names instead of a regular file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotAFile {
    Directory,
    /// A FIFO, socket or device: never readable file text.
    Special,
}

pub trait PathAccess {
    fn validate_read(&self, path: &Path) -> Result<ValidatedRead, Box<PathFailure>>;
}

impl PathAccess for crate::policy::path::PathPolicy {
    fn validate_read(&self, path: &Path) -> Result<ValidatedRead, Box<PathFailure>> {
        crate::policy::path::PathPolicy::validate_read(self, path)
            .map(|validated| {
                let resolved = self
                    .workspace_relative(&validated.canonical)
                    .unwrap_or_else(|| validated.canonical.to_string_lossy().into_owned());
                // The request is a symlink, or names its file through a
                // directory link inside the workspace (`link/a.rs`): a
                // component of the path, as `.` and `..` fold it, is one.
                // Links above the workspace (`/tmp`) rename nothing, and a
                // name that differs from the file's only in letter case (a
                // case-insensitive file system) is no link.
                let lexical = self.expand_and_resolve(path);
                let is_link = |at: &Path| {
                    fs::symlink_metadata(at).is_ok_and(|meta| meta.file_type().is_symlink())
                };
                let through_link = is_link(&lexical)
                    || lexical
                        .ancestors()
                        .skip(1)
                        .take_while(|dir| {
                            self.workspace_relative(dir).is_some_and(|name| name != ".")
                        })
                        .any(is_link);
                let (named, target) = if through_link {
                    (path.to_string_lossy().into_owned(), Some(resolved))
                } else {
                    (resolved, None)
                };
                ValidatedRead {
                    named,
                    target,
                    canonical: validated.canonical,
                    display: validated.display,
                }
            })
            .map_err(|error| {
                let missing = error.code == crate::policy::PolicyErrorCode::NotFound;
                let resolved = self.expand_and_resolve(path);
                Box::new(PathFailure {
                    code: error.shared_code(),
                    message: error.message,
                    safe_path: error.safe_path,
                    resource_missing: missing,
                    sparse_checkout: missing && in_sparse_checkout(&resolved),
                    not_a_file: (error.code == crate::policy::PolicyErrorCode::NotRegular).then(
                        || match resolved.is_dir() {
                            true => NotAFile::Directory,
                            false => NotAFile::Special,
                        },
                    ),
                    search_root: missing
                        .then(|| missing_search_root(self, path, &resolved))
                        .flatten(),
                    view_tree: missing
                        .then(|| {
                            crate::response::result::missing_path_lead(
                                self,
                                &path.to_string_lossy(),
                            )
                        })
                        .flatten()
                        .and_then(|mut next| next.get_mut("viewTree").map(serde_json::Value::take)),
                    corrected: missing
                        .then(|| corrected_path(self, path, &resolved))
                        .flatten(),
                })
            })
    }
}

/// The readable file a missing file's name was likely mistyped for: the
/// one close name in its admitted parent directory ([`close_name`]), in the
/// requested path's own directory spelling. Only a path the policy admits
/// reading is named, so a recovery never reveals a withheld file.
///
/// [`close_name`]: crate::tools::leads::find_file::close_name
fn corrected_path(
    policy: &crate::policy::path::PathPolicy,
    requested: &Path,
    resolved: &Path,
) -> Option<String> {
    requested.file_name()?;
    let parent = policy.validate(resolved.parent()?).ok()?;
    let name = resolved.file_name()?.to_string_lossy();
    let close = crate::tools::leads::find_file::close_name(&parent.canonical, &name)?;
    let corrected = requested.with_file_name(close);
    crate::policy::path::PathPolicy::validate_read(policy, &corrected).ok()?;
    Some(corrected.to_string_lossy().into_owned())
}

/// The admitted directory a missing file's listing searches from: the
/// workspace root, or the most specific allowed root holding `resolved`,
/// so a file moved to another tree is found; the closest admitted ancestor
/// when no root holds it.
fn missing_search_root(
    policy: &crate::policy::path::PathPolicy,
    requested: &Path,
    resolved: &Path,
) -> Option<String> {
    if policy.workspace_relative(resolved).is_some() {
        return Some(".".to_owned());
    }
    policy
        .allowed_roots()
        .into_iter()
        .filter(|root| resolved.starts_with(root) && root.is_dir())
        .max_by_key(|root| root.components().count())
        .map(|root| root.to_string_lossy().into_owned())
        .or_else(|| policy.nearest_existing_dir(&requested.to_string_lossy()))
}

/// Whether `path` lies in a git worktree with sparse checkout enabled (as a
/// `ghCloneRepo` clone with `path` is).
fn in_sparse_checkout(path: &Path) -> bool {
    path.ancestors()
        .skip(1)
        .map(|dir| dir.join(".git"))
        .find(|git| git.is_dir())
        .is_some_and(|git| {
            git.join("info/sparse-checkout").is_file()
                && std::fs::read_to_string(git.join("config")).is_ok_and(|config| {
                    config.lines().any(|line| {
                        line.split_whitespace()
                            .collect::<String>()
                            .eq_ignore_ascii_case("sparseCheckout=true")
                    })
                })
        })
}
