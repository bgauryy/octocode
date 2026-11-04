//! The ordered write side of a materialize call: what each entry acquired and the files it skipped.
use super::*;

/// What a materialize page has for one listed entry.
pub(in crate::tools::gh_structure) enum Acquired {
    NotAFile,
    /// Larger than this call may read, by its listed size.
    Oversized,
    Read(Result<octocode_github::ContentResponse, ProviderError>),
}

/// The ordered write side of a materialize call.
pub(in crate::tools::gh_structure) struct MaterializeWriter<'a> {
    pub(in crate::tools::gh_structure) root: &'a Path,
    pub(in crate::tools::gh_structure) home: &'a Path,
    pub(in crate::tools::gh_structure) written: usize,
    pub(in crate::tools::gh_structure) cursor: usize,
    /// Why the walk stopped early, when a provider limit stopped it.
    pub(in crate::tools::gh_structure) stopped: Option<String>,
    pub(in crate::tools::gh_structure) skips: MaterializeSkips,
    /// Files this page wrote completely, for the manifest.
    pub(in crate::tools::gh_structure) recorded: Vec<(String, FileIdentity)>,
}

impl MaterializeWriter<'_> {
    /// Write one listed entry (its read, when it is a file); `false` stops
    /// the walk at this entry, which the continuation resumes.
    pub(in crate::tools::gh_structure) async fn write(
        &mut self,
        entry: &TreeEntry,
        read: Acquired,
    ) -> Result<bool, ProviderError> {
        let acquired = match read {
            Acquired::NotAFile => {
                self.cursor += 1;
                return Ok(true);
            }
            Acquired::Oversized => {
                self.cursor += 1;
                self.skips.oversized.push(entry.path.clone());
                return Ok(true);
            }
            Acquired::Read(Ok(acquired)) => acquired,
            // A read refused by the call's limits ends the walk, not the
            // files already written: the continuation resumes at this entry.
            Acquired::Read(Err(error))
                if self.written > 0
                    && matches!(
                        error.kind,
                        ProviderErrorKind::RateLimited | ProviderErrorKind::Timeout
                    ) =>
            {
                self.stopped = Some(format!(
                    "Stopped at {} after {} file(s): {}.",
                    entry.path,
                    self.written,
                    error.message.trim_end_matches('.')
                ));
                return Ok(false);
            }
            // One unreadable file (binary, unsupported entry) must not abort
            // the batch; only auth/rate/timeout/cancel failures propagate.
            Acquired::Read(Err(error))
                if !access_failure(&error) && !error.is_cancelled_or_timed_out() =>
            {
                self.cursor += 1;
                if error.reason == Some(ProviderErrorReason::BinaryFile) {
                    self.skips.binary.push(entry.path.clone());
                } else {
                    self.skips
                        .unreadable
                        .push((entry.path.clone(), error.message.to_string()));
                }
                return Ok(true);
            }
            Acquired::Read(Err(error)) => return Err(error),
        };
        self.cursor += 1;
        if !crate::tools::gh_shared::is_repo_relative(&entry.path) {
            self.skips.unreadable.push((
                entry.path.clone(),
                "the provider path leaves the materialize root".to_owned(),
            ));
            return Ok(true);
        }
        snapshot::write_file(self.home, self.root, &entry.path, &acquired.bytes).await?;
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

/// Files a materialize call did not write. `location.skipped` names each
/// path once; the warnings carry counts and each distinct read error once.
#[derive(Default)]
pub(in crate::tools::gh_structure) struct MaterializeSkips {
    /// The body limit an oversized file passed.
    pub(in crate::tools::gh_structure) read_limit: usize,
    pub(in crate::tools::gh_structure) oversized: Vec<String>,
    pub(in crate::tools::gh_structure) unreadable: Vec<(String, String)>,
    /// Not text: nothing a local text tool could read is missing.
    pub(in crate::tools::gh_structure) binary: Vec<String>,
}

impl MaterializeSkips {
    pub(in crate::tools::gh_structure) fn new(read_limit: usize) -> Self {
        Self {
            read_limit,
            ..Self::default()
        }
    }

    pub(in crate::tools::gh_structure) fn is_empty(&self) -> bool {
        self.binary.is_empty() && !self.leaves_a_gap()
    }

    /// Whether a skipped file holds text the checkout now lacks.
    pub(in crate::tools::gh_structure) fn leaves_a_gap(&self) -> bool {
        !self.oversized.is_empty() || !self.unreadable.is_empty()
    }

    pub(in crate::tools::gh_structure) fn paths(&self) -> Vec<&str> {
        self.oversized
            .iter()
            .map(String::as_str)
            .chain(self.unreadable.iter().map(|(path, _)| path.as_str()))
            .chain(self.binary.iter().map(String::as_str))
            .collect()
    }

    /// Why `location.skipped` names binaries on a checkout that is complete.
    pub(in crate::tools::gh_structure) fn binary_hint(&self) -> Option<String> {
        (!self.binary.is_empty()).then(|| {
            format!(
                "{} binary file(s) are in location.skipped: not text, so not written.",
                self.binary.len()
            )
        })
    }

    pub(in crate::tools::gh_structure) fn warnings(&self) -> Vec<String> {
        let mut warnings = Vec::new();
        if !self.oversized.is_empty() {
            warnings.push(format!(
                "Skipped {} file(s) larger than this call's {} MiB read limit (location.skipped).",
                self.oversized.len(),
                self.read_limit / (1024 * 1024)
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
