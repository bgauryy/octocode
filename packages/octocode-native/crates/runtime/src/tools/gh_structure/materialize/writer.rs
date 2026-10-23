//! The ordered write side of a materialize page: what each entry acquired, its caps and the files it skipped.
use super::*;

/// What a materialize page has for one listed entry.
pub(in crate::tools::gh_structure) enum Acquired {
    NotAFile,
    /// Larger than [`MATERIALIZE_FILE_BYTES`] by its listed size.
    Oversized,
    /// Already on disk, as the manifest recorded it.
    Reused,
    Read(Result<octocode_github::ContentResponse, ProviderError>),
}

/// The ordered write side of a materialize page and its caps.
pub(in crate::tools::gh_structure) struct MaterializeWriter<'a> {
    pub(in crate::tools::gh_structure) root: &'a Path,
    pub(in crate::tools::gh_structure) home: &'a Path,
    pub(in crate::tools::gh_structure) written: usize,
    pub(in crate::tools::gh_structure) total_bytes: usize,
    pub(in crate::tools::gh_structure) cursor: usize,
    pub(in crate::tools::gh_structure) skips: MaterializeSkips,
    /// Files this page wrote completely, for the manifest.
    pub(in crate::tools::gh_structure) recorded: Vec<(String, FileIdentity)>,
}

impl MaterializeWriter<'_> {
    /// Write one listed entry (its read, when it is a file); `false` stops
    /// the page at this entry, which the continuation resumes.
    pub(in crate::tools::gh_structure) async fn write(
        &mut self,
        entry: &TreeEntry,
        read: Acquired,
    ) -> Result<bool, ProviderError> {
        if self.written >= MATERIALIZE_FILE_CAP {
            return Ok(false);
        }
        if self.total_bytes >= MATERIALIZE_TOTAL_BYTES {
            return Ok(false);
        }
        self.cursor += 1;
        let acquired = match read {
            Acquired::NotAFile | Acquired::Reused => return Ok(true),
            Acquired::Oversized => {
                self.skips.oversized.push(entry.path.clone());
                return Ok(true);
            }
            Acquired::Read(Ok(acquired)) => acquired,
            // One unreadable file (binary, unsupported entry) must not abort
            // the batch; only auth/rate/timeout/cancel failures propagate.
            Acquired::Read(Err(error))
                if !access_failure(&error) && !error.is_cancelled_or_timed_out() =>
            {
                self.skips
                    .unreadable
                    .push((entry.path.clone(), error.message.to_string()));
                return Ok(true);
            }
            Acquired::Read(Err(error)) => return Err(error),
        };
        if acquired.bytes.len() > MATERIALIZE_FILE_BYTES {
            self.skips.oversized.push(entry.path.clone());
            return Ok(true);
        }
        if self.total_bytes.saturating_add(acquired.bytes.len()) > MATERIALIZE_TOTAL_BYTES {
            self.cursor -= 1;
            return Ok(false);
        }
        if !crate::tools::gh_shared::is_repo_relative(&entry.path) {
            self.skips.unreadable.push((
                entry.path.clone(),
                "the provider path leaves the materialize root".to_owned(),
            ));
            return Ok(true);
        }
        snapshot::write_file(self.home, self.root, &entry.path, &acquired.bytes).await?;
        self.total_bytes += acquired.bytes.len();
        self.written += 1;
        self.recorded
            .push((entry.path.clone(), FileIdentity::of(&acquired.bytes)));
        Ok(true)
    }
}

pub(in crate::tools::gh_structure) fn write_error(
    action: &str,
    error: &std::io::Error,
) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::Configuration,
        format!("failed to {action}: {error}"),
    )
}

/// Files a materialize page did not write. `location.skipped` names each
/// path once; the warnings carry counts and each distinct read error once.
#[derive(Default)]
pub(in crate::tools::gh_structure) struct MaterializeSkips {
    pub(in crate::tools::gh_structure) oversized: Vec<String>,
    pub(in crate::tools::gh_structure) unreadable: Vec<(String, String)>,
}

impl MaterializeSkips {
    pub(in crate::tools::gh_structure) fn is_empty(&self) -> bool {
        self.oversized.is_empty() && self.unreadable.is_empty()
    }

    pub(in crate::tools::gh_structure) fn paths(&self) -> Vec<&str> {
        self.oversized
            .iter()
            .map(String::as_str)
            .chain(self.unreadable.iter().map(|(path, _)| path.as_str()))
            .collect()
    }

    pub(in crate::tools::gh_structure) fn warnings(&self) -> Vec<String> {
        let mut warnings = Vec::new();
        if !self.oversized.is_empty() {
            warnings.push(format!(
                "Skipped {} file(s) larger than the {} KiB materialize per-file limit (location.skipped).",
                self.oversized.len(),
                MATERIALIZE_FILE_BYTES / 1024
            ));
        }
        if !self.unreadable.is_empty() {
            let mut errors: Vec<&str> = Vec::new();
            for (_, message) in &self.unreadable {
                let message = message.trim_end_matches('.');
                if !errors.contains(&message) {
                    errors.push(message);
                }
            }
            warnings.push(format!(
                "Skipped {} unreadable file(s) (location.skipped): {}.",
                self.unreadable.len(),
                errors.join("; ")
            ));
        }
        warnings
    }
}
