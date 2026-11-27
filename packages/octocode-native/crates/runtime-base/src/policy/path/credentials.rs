//! `security.credentialFiles`: what the policy withholds under each setting.
use super::PathPolicy;
use crate::policy::discovery::{
    EntryKind, folded_path, is_in_sensitive_directory, is_sensitive_path,
    is_sensitive_repository_file, repository_path,
};
use std::path::Path;

impl PathPolicy {
    /// This policy with credential file names withheld whatever the setting,
    /// for what leaves a local read: a graph snapshot on disk, or evidence sent
    /// to a classification provider.
    pub fn withholding_credentials(&self) -> Self {
        Self {
            allow_credential_files: false,
            ..self.clone()
        }
    }

    pub(crate) fn allows_credential_files(&self) -> bool {
        self.allow_credential_files
    }

    /// Whether this policy withholds `path`, a `kind` entry: the discovery
    /// rules, with credential file names lifted when the user allows them, except
    /// Octocode's own home and files directly in the user's home.
    pub fn withholds(&self, path: &Path, kind: EntryKind) -> bool {
        if !self.allow_credential_files || self.keeps_credentials_withheld(path) {
            return is_sensitive_path(path, kind);
        }
        is_in_sensitive_directory(path)
    }

    /// Whether this policy withholds a file a GitHub repository names by
    /// `path` from a read that returns its text: the local rule, with
    /// credential file names lifted when the user allows them (the output gate
    /// then keeps the read in memory only); a sensitive directory never opens.
    /// Saved copies on disk (materialize, clone) withhold them whatever the
    /// setting.
    pub fn withholds_repository_file(&self, path: &str) -> bool {
        if self.allow_credential_files {
            return is_in_sensitive_directory(&repository_path(path));
        }
        is_sensitive_repository_file(path)
    }

    /// Whether this policy withholds the text of a repository file by any of
    /// the names that identify it: its own `path`, the `previous` path of a
    /// rename, and the `link_target` a symlink resolves to. This is the one
    /// decision for every place file text enters a response or the disk (a
    /// read, a search row, a patch, a link target, a saved answer).
    pub fn withheld_text(
        &self,
        path: &str,
        previous: Option<&str>,
        link_target: Option<&str>,
    ) -> bool {
        withheld_names(path, previous, link_target, |name| {
            self.withholds_repository_file(name)
        })
    }

    /// What `allow` never opens: Octocode's home; files directly in the user's
    /// home; and the user's home dot directories (`~/.claude`, `~/.codex`), which
    /// hold tool logins, not project sources. Compared as the filesystem compares
    /// names, so a symlinked or differently cased home cannot step around it.
    fn keeps_credentials_withheld(&self, path: &Path) -> bool {
        let path = folded_path(path);
        self.octocode_home
            .iter()
            .any(|home| path.starts_with(folded_path(home)))
            || self.home_forms.iter().any(|home| {
                let home = folded_path(home);
                path.strip_prefix(&home).is_ok_and(|inside| {
                    let mut parts = inside.components();
                    let first = parts.next();
                    parts.next().is_none()
                        || first
                            .is_some_and(|part| part.as_os_str().to_string_lossy().starts_with('.'))
                })
            })
    }
}

/// [`PathPolicy::withheld_text`] under the default policy (credential file
/// names withheld whatever the setting): for text that stays on disk or
/// outlives one request, and for a caller the runtime gave no policy.
pub fn withheld_text_by_default(
    path: &str,
    previous: Option<&str>,
    link_target: Option<&str>,
) -> bool {
    withheld_names(path, previous, link_target, is_sensitive_repository_file)
}

fn withheld_names(
    path: &str,
    previous: Option<&str>,
    link_target: Option<&str>,
    withholds: impl Fn(&str) -> bool,
) -> bool {
    std::iter::once(path)
        .chain(previous)
        .chain(link_target)
        .any(withholds)
}

#[cfg(test)]
mod tests;
