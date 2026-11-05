//! `security.credentialFiles`: what the policy withholds under each setting.
use super::PathPolicy;
use crate::policy::discovery::{
    EntryKind, folded_path, is_in_sensitive_directory, is_sensitive_path,
};
use std::path::Path;

impl PathPolicy {
    /// This policy with credential file names withheld whatever the setting,
    /// for what leaves a local read: a graph snapshot on disk, or evidence sent
    /// to a classification provider.
    pub(crate) fn withholding_credentials(&self) -> Self {
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
    pub(crate) fn withholds(&self, path: &Path, kind: EntryKind) -> bool {
        if !self.allow_credential_files || self.keeps_credentials_withheld(path) {
            return is_sensitive_path(path, kind);
        }
        is_in_sensitive_directory(path)
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
