//! Durable extraction progress and its spill files. Linking is rebuilt from verified facts.
use super::*;
use octocode_engine::fact_scan::GraphFactsView;
use octocode_engine::graph::{GraphFactRustModule, GraphFactsTypedEntry};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

mod artifact;
mod state;
#[cfg(test)]
mod tests;
use artifact::{decode, encode};
pub(in crate::tools::ast_graph) use state::job_path;
use state::{Manifest, Stage};

use super::super::limits::LIMITS;
const JOB_VERSION: u32 = 1;

/// Bytes of artifact text one secret scan reads: within what the scanner
/// admits, which refuses (as a secret) any larger text unread.
const SECRET_SCAN_WINDOW: usize = 4 << 20;
/// Bytes consecutive windows share, longer than any one credential, so a
/// secret across a window boundary lies whole inside one window.
const SECRET_SCAN_OVERLAP: usize = 64 << 10;

/// Whether `text` holds a secret, scanned window by window so an artifact
/// of any admitted size is read rather than refused unread.
fn holds_secret(security: &ContentSecurity, text: &str) -> bool {
    let mut start = 0;
    loop {
        let mut end = (start + SECRET_SCAN_WINDOW).min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        if security.sanitize_text(&text[start..end], None).has_secrets {
            return true;
        }
        if end == text.len() {
            return false;
        }
        start = end - SECRET_SCAN_OVERLAP;
        while !text.is_char_boundary(start) {
            start -= 1;
        }
    }
}

pub(super) struct Job<'a> {
    dir: PathBuf,
    manifest: Manifest,
    resumed: bool,
    state: Mutex<State>,
    paths: &'a PathPolicy,
    security: &'a ContentSecurity,
}

#[derive(Default)]
struct State {
    bytes: u64,
    memory_bytes: u64,
    artifacts: BTreeMap<String, u64>,
    committed: BTreeSet<String>,
    resumed: BTreeSet<String>,
    /// Artifacts [`Job::load`] verified this build, by file, with their
    /// sizes: a reused file keeps its artifact rather than writing it again.
    loaded: BTreeMap<String, u64>,
    memory_files: usize,
}

pub(super) struct Kept {
    pub(super) modules: Option<(String, Vec<GraphFactRustModule>)>,
    digest: String,
    stamp: Option<octocode_engine::stamp::SourceStamp>,
    storage: Storage,
}

enum Storage {
    Disk(PathBuf),
    Memory(Box<[u8]>),
}

impl Job<'_> {
    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn target(&self, relative: &str) -> PathBuf {
        self.dir.join(format!(
            "{}.fact",
            octocode_engine::digest::sha256(relative.as_bytes())
        ))
    }

    fn read(&self, target: &Path) -> Result<Vec<u8>, ToolError> {
        state::checked_path(self.paths, target, false)?;
        crate::tools::source::read_bounded(target, LIMITS.artifact_bytes + 65)
            .map_err(|error| failure(format!("cannot read graph checkpoint: {error:?}")))
    }

    pub(super) fn load(
        &self,
        context: &octocode_engine::fact_scan::GraphFactsReuseContext<'_>,
    ) -> Option<GraphFactsTypedEntry> {
        let bytes = self.read(&self.target(context.relative_path)).ok()?;
        let artifact = decode(
            &bytes,
            &self.manifest.namespace,
            context.relative_path,
            context.content_digest,
        )
        .ok()?;
        let entry = artifact.extraction.into_entry();
        if !context.accepts(&entry) {
            return None;
        }
        let mut state = self.state();
        if self.resumed {
            state.resumed.insert(context.relative_path.into());
        }
        state
            .loaded
            .insert(context.relative_path.into(), bytes.len() as u64);
        Some(entry)
    }

    pub(super) fn keep(
        &self,
        view: &GraphFactsView<'_>,
        source: &capture::CapturedSource,
    ) -> Result<Kept, ToolError> {
        let kept = |storage| Kept {
            modules: (view.facts.language == "rust")
                .then(|| (view.relative_path.into(), view.facts.modules.clone())),
            digest: view.content_digest.into(),
            stamp: view.source_stamp,
            storage,
        };
        // Reused from its own verified artifact: that artifact already holds
        // these facts, so it is kept, not encoded, scanned and written again.
        if view.reused {
            let mut state = self.state();
            if let Some(size) = state.loaded.remove(view.relative_path) {
                if state
                    .artifacts
                    .insert(view.relative_path.into(), size)
                    .is_none()
                {
                    state.bytes = state.bytes.saturating_add(size);
                }
                state.committed.insert(view.relative_path.into());
                return Ok(kept(Storage::Disk(self.target(view.relative_path))));
            }
        }
        let bytes = encode(&self.manifest.namespace, view, source)?;
        let mut state = self.state();
        let storage = if holds_secret(self.security, &String::from_utf8_lossy(&bytes)) {
            let bytes_len = bytes.len() as u64;
            if state.memory_bytes.saturating_add(bytes_len) > LIMITS.memory_only_bytes {
                return Err(limit(
                    "memory-only extraction artifacts",
                    LIMITS.memory_only_bytes,
                ));
            }
            state.memory_bytes += bytes_len;
            state.memory_files += 1;
            if let Some(previous) = state.artifacts.remove(view.relative_path) {
                std::fs::remove_file(self.target(view.relative_path)).map_err(|error| {
                    failure(format!("cannot remove obsolete graph checkpoint: {error}"))
                })?;
                state::sync_dir(&self.dir)?;
                state.bytes = state.bytes.saturating_sub(previous);
            }
            Storage::Memory(bytes.into_boxed_slice())
        } else {
            let target = self.target(view.relative_path);
            state::checked_path(self.paths, &target, true)?;
            let previous = state
                .artifacts
                .get(view.relative_path)
                .copied()
                .unwrap_or(0);
            let next_bytes = state
                .bytes
                .saturating_sub(previous)
                .saturating_add(bytes.len() as u64);
            if next_bytes > LIMITS.spill_bytes {
                return Err(limit("checkpoint disk artifacts", LIMITS.spill_bytes));
            }
            // An artifact is its own commit record: header digest + pinned identity + complete payload.
            // No receipt can become durable before its referenced facts.
            state::durable_write(&target, &bytes)?;
            state.bytes = next_bytes;
            state
                .artifacts
                .insert(view.relative_path.into(), bytes.len() as u64);
            state.committed.insert(view.relative_path.into());
            Storage::Disk(target)
        };
        Ok(kept(storage))
    }

    pub(super) fn restore(
        &self,
        relative: &str,
        kept: Kept,
    ) -> Result<
        (
            GraphFactsTypedEntry,
            super::super::classify::SourceMarks,
            capture::CapturedSource,
        ),
        ToolError,
    > {
        let bytes = match kept.storage {
            Storage::Disk(path) => self.read(&path)?.into_boxed_slice(),
            Storage::Memory(bytes) => bytes,
        };
        let artifact = decode(&bytes, &self.manifest.namespace, relative, &kept.digest)?;
        let (mut entry, marks, source) = artifact.into_parts();
        entry.source_stamp = kept.stamp;
        Ok((entry, marks, source))
    }

    pub(super) fn linking(&self) -> Result<(), ToolError> {
        self.manifest.write(&self.dir, Stage::Linking)
    }

    pub(super) fn progress(&self) -> BuildProgress {
        let state = self.state();
        BuildProgress {
            resumed_files: state.resumed.len(),
            checkpointed_files: state.committed.len(),
            spilled_bytes: state
                .committed
                .iter()
                .filter_map(|relative| state.artifacts.get(relative))
                .sum(),
            memory_only_files: state.memory_files,
            max_spill_bytes: LIMITS.spill_bytes,
            max_memory_only_bytes: LIMITS.memory_only_bytes,
        }
    }
}

/// Removes a job once its graph is published: a completed job is never
/// resumed, so its artifacts would only hold disk until the next build of
/// the same root. Warm rebuilds reuse the disposable fact cache instead.
pub(in crate::tools::ast_graph) fn retire(path: &Path) -> Result<(), ToolError> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(failure(format!("cannot remove graph checkpoint: {error}"))),
    }
}

/// Removes jobs an earlier build marked complete (before jobs were retired
/// on publication); unfinished jobs stay resumable.
pub(in crate::tools::ast_graph) fn remove_completed(jobs: &Path) {
    let Ok(entries) = std::fs::read_dir(jobs) else {
        return;
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        let real_dir = std::fs::symlink_metadata(&dir).is_ok_and(|metadata| metadata.is_dir());
        if real_dir
            && Manifest::read(&dir).is_some_and(|manifest| manifest.stage == Stage::Complete)
        {
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}

fn failure(message: impl Into<String>) -> ToolError {
    ToolError::new(ErrorCode::FileAccessFailed, message)
}

fn limit(what: &str, bytes: u64) -> ToolError {
    ToolError::new(
        ErrorCode::ScopeTooBroad,
        format!(
            "graph {what} exceed the {bytes}-byte limit; narrow the build scope; committed extraction remains resumable"
        ),
    )
}
