use std::path::Path;

use super::{EntryKind, folded, is_sensitive_path, sensitive_directory};

/// What a walk's path policy withheld: each security-policy directory name
/// with how many directories it matched, how many credential files, how
/// many `.git` pointer files, how many symlinks it did not follow, and how
/// many other denied entries (outside the allowed roots). Names come from
/// the policy lists, never the walked paths; the one path kept is the
/// directory that directly holds withheld entries ([`Self::holders`]), never
/// an entry's own name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Withheld {
    pub dirs: std::collections::BTreeMap<&'static str, usize>,
    pub files: usize,
    pub(crate) git_files: usize,
    pub symlinks: usize,
    pub other: usize,
    holders: std::collections::BTreeMap<(WithheldKind, String), usize>,
}

/// Which count a withheld entry is in, and the reason it is disclosed with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum WithheldKind {
    /// A security-policy directory, by its policy name (`.ssh`, `.config/gh`).
    PolicyDir(&'static str),
    CredentialFile,
    GitFile,
    Symlink,
    Other,
}

impl WithheldKind {
    /// The rule that withheld it, as `skippedPaths` names it.
    pub fn reason(self) -> std::borrow::Cow<'static, str> {
        match self {
            Self::PolicyDir(name) => {
                format!("security-policy dir {}/", name.trim_end_matches('/')).into()
            }
            Self::CredentialFile => "credential file".into(),
            Self::GitFile => "`.git` file (a submodule or worktree pointer)".into(),
            Self::Symlink => "symlink not followed".into(),
            Self::Other => "outside the allowed roots or unresolvable".into(),
        }
    }
}

/// The directory a walk meets a policy directory in: the parent of the
/// shortest ancestor of `path` the policy withholds (`a` for `a/.ssh/key`,
/// `a/.config` for `a/.config/gh`).
fn policy_dir_holder(path: &Path) -> String {
    let ancestors = path.ancestors().collect::<Vec<_>>();
    ancestors
        .iter()
        .rev()
        .find(|at| sensitive_directory(&octocode_engine::text::slash_path(at)).is_some())
        .and_then(|at| at.parent())
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

impl Withheld {
    /// Count one entry the path policy denied. One stat of `path` decides
    /// both whether the policy withholds it and which bucket it counts in.
    pub fn record(&mut self, path: &Path) {
        let kind = EntryKind::stat(path);
        let is_dir = kind == EntryKind::Dir;
        let named = if !is_sensitive_path(path, kind) {
            if path.is_symlink() {
                self.symlinks += 1;
                WithheldKind::Symlink
            } else {
                self.other += 1;
                WithheldKind::Other
            }
        } else {
            let aliases = octocode_engine::text::slash_path(path);
            let own = folded(aliases.rsplit('/').next().unwrap_or_default());
            let own = own.as_ref();
            match sensitive_directory(&aliases) {
                Some(".git") if !is_dir && own == ".git" => {
                    self.git_files += 1;
                    WithheldKind::GitFile
                }
                Some(dir) if is_dir || own != dir => {
                    *self.dirs.entry(dir).or_default() += 1;
                    let holder = policy_dir_holder(path);
                    *self
                        .holders
                        .entry((WithheldKind::PolicyDir(dir), holder))
                        .or_default() += 1;
                    return;
                }
                _ => {
                    self.files += 1;
                    WithheldKind::CredentialFile
                }
            }
        };
        let holder = path.parent().unwrap_or(path).to_string_lossy().into_owned();
        *self.holders.entry((named, holder)).or_default() += 1;
    }

    /// The directories that directly hold withheld entries, policy
    /// directories included: each with the kind and how many it holds. The
    /// held entries are never named.
    pub fn holders(&self) -> impl Iterator<Item = (WithheldKind, &str, usize)> {
        self.holders
            .iter()
            .map(|((kind, holder), count)| (*kind, holder.as_str(), *count))
    }

    /// What this walk withheld beyond `stated`, an earlier count of the same
    /// root (a re-run that lifted walk rules, beyond the page's own walk):
    /// each count less the stated one, so no entry is disclosed twice.
    #[must_use]
    pub fn beyond(&self, stated: &Self) -> Self {
        let less = |mine: usize, theirs: usize| mine.saturating_sub(theirs);
        Self {
            dirs: self
                .dirs
                .iter()
                .map(|(name, count)| {
                    (
                        *name,
                        less(*count, stated.dirs.get(name).copied().unwrap_or(0)),
                    )
                })
                .filter(|(_, count)| *count > 0)
                .collect(),
            files: less(self.files, stated.files),
            git_files: less(self.git_files, stated.git_files),
            symlinks: less(self.symlinks, stated.symlinks),
            other: less(self.other, stated.other),
            holders: self
                .holders
                .iter()
                .map(|(key, count)| {
                    (
                        key.clone(),
                        less(*count, stated.holders.get(key).copied().unwrap_or(0)),
                    )
                })
                .filter(|(_, count)| *count > 0)
                .collect(),
        }
    }

    pub fn total(&self) -> usize {
        self.dirs.values().sum::<usize>() + self.files + self.git_files + self.symlinks + self.other
    }

    /// One disclosure: the count, which security-policy directories, that
    /// no flag or config setting lifts the policy, and that absence there
    /// is unproven.
    pub fn notice(&self) -> Option<String> {
        let total = self.total();
        if total == 0 {
            return None;
        }
        let plural = |count: usize, one: &str, many: &str| {
            format!("{count} {}", if count == 1 { one } else { many })
        };
        let mut parts = Vec::new();
        if !self.dirs.is_empty() {
            let names = self
                .dirs
                .iter()
                .map(|(name, count)| {
                    let name = name.trim_end_matches('/');
                    if *count > 1 {
                        format!("{name}/ ×{count}")
                    } else {
                        format!("{name}/")
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            parts.push(format!("security-policy dirs {names}"));
        }
        if self.files > 0 {
            parts.push(plural(self.files, "credential file", "credential files"));
        }
        if self.git_files > 0 {
            parts.push(format!(
                "{} (submodule or worktree pointers)",
                plural(self.git_files, "`.git` file", "`.git` files")
            ));
        }
        if self.symlinks > 0 {
            parts.push(format!(
                "{} not followed",
                plural(self.symlinks, "symlink", "symlinks")
            ));
        }
        if self.other > 0 {
            parts.push(format!(
                "{} outside the allowed roots or unresolvable",
                plural(self.other, "entry", "entries")
            ));
        }
        Some(format!(
            "{} withheld by path policy: {}.",
            plural(total, "entry", "entries"),
            parts.join("; ")
        ))
    }
}
