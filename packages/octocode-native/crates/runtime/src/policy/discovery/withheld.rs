use std::path::Path;

use super::{is_sensitive_path, sensitive_directory};

/// What a walk's path policy withheld: each security-policy directory name
/// with how many directories it matched, how many credential files, how
/// many `.git` pointer files, how many symlinks it did not follow, and how
/// many other denied entries (outside the allowed roots). Names come from
/// the policy lists, never the walked paths.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Withheld {
    pub dirs: std::collections::BTreeMap<&'static str, usize>,
    pub files: usize,
    pub(crate) git_files: usize,
    pub symlinks: usize,
    pub other: usize,
}

impl Withheld {
    /// Count one entry the path policy denied.
    pub fn record(&mut self, path: &Path, is_dir: bool) {
        if !is_sensitive_path(path) {
            if path.is_symlink() {
                self.symlinks += 1;
            } else {
                self.other += 1;
            }
            return;
        }
        let aliases = octocode_engine::text::slash_path(path);
        let own = aliases.rsplit('/').next().unwrap_or_default();
        match sensitive_directory(&aliases) {
            Some(".git") if !is_dir && own == ".git" => self.git_files += 1,
            Some(dir) if is_dir || own != dir => *self.dirs.entry(dir).or_default() += 1,
            _ => self.files += 1,
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
