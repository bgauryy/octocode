//! Durable extraction progress and its spill files. Linking is rebuilt from verified facts.
use super::*;
use octocode_engine::fact_scan::GraphFactsView;
use octocode_engine::graph::{GraphFactRustModule, GraphFactsTypedEntry};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

mod artifact;
pub(in crate::tools::ast_graph) mod lock;
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
    /// Held for the job's life: one build at a time writes a scope's job.
    _lock: std::fs::File,
    manifest: Manifest,
    resumed: bool,
    state: Mutex<State>,
    paths: &'a PathPolicy,
    security: &'a ContentSecurity,
    limits: super::super::limits::Limits,
}

#[derive(Default)]
struct State {
    bytes: u64,
    memory_bytes: u64,
    /// Encoded bytes of the artifacts kept decoded for linking.
    retained_bytes: u64,
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

    /// The verified facts of the file's own artifact, decoded once: its
    /// source correspondence is captured again from the current source, so
    /// only the extraction is decoded here.
    pub(super) fn load(
        &self,
        context: &octocode_engine::fact_scan::GraphFactsReuseContext<'_>,
    ) -> Option<GraphFactsTypedEntry> {
        let bytes = self.read(&self.target(context.relative_path)).ok()?;
        let extraction = artifact::decode_extraction(
            &bytes,
            &self.manifest.namespace,
            context.relative_path,
            context.content_digest,
        )
        .ok()?;
        let entry = extraction.into_entry();
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

    /// Commits the file's artifact and keeps its facts for linking: decoded
    /// while the retention budget allows, else on disk (or, holding a
    /// secret, encoded in memory) to decode at [`Job::restore`]. Encoding,
    /// the secret scan and the write run outside the job's lock, which only
    /// guards its counters. An artifact is its own commit record (header
    /// digest + pinned identity + complete payload), so it is written
    /// without a sync: a torn or lost write fails verification on the next
    /// build and is extracted again; only stage transitions are durable.
    pub(super) fn keep(
        &self,
        view: &GraphFactsView<'_>,
        source: capture::CapturedSource,
    ) -> Result<super::scan::Kept, ToolError> {
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
                drop(state);
                let storage = Storage::Disk(self.target(view.relative_path));
                return Ok(self.retain(view, source, size, storage));
            }
        }
        let bytes = encode(&self.manifest.namespace, view, &source)?;
        let size = bytes.len() as u64;
        let storage = if holds_secret(self.security, &String::from_utf8_lossy(&bytes)) {
            let previous = {
                let mut state = self.state();
                if state.memory_bytes.saturating_add(size) > self.limits.memory_only_bytes {
                    return Err(limit(
                        "memory-only extraction artifacts",
                        self.limits.memory_only_bytes,
                    ));
                }
                state.memory_bytes += size;
                state.memory_files += 1;
                let previous = state.artifacts.remove(view.relative_path);
                if let Some(previous) = previous {
                    state.bytes = state.bytes.saturating_sub(previous);
                }
                previous
            };
            if previous.is_some() {
                std::fs::remove_file(self.target(view.relative_path)).map_err(|error| {
                    failure(format!("cannot remove obsolete graph checkpoint: {error}"))
                })?;
            }
            Storage::Memory(bytes.into_boxed_slice())
        } else {
            let target = self.target(view.relative_path);
            state::checked_path(self.paths, &target, true)?;
            // Reserve the quota first, so concurrent writers never pass it together.
            let previous = {
                let mut state = self.state();
                let previous = state.artifacts.get(view.relative_path).copied();
                let next_bytes = state
                    .bytes
                    .saturating_sub(previous.unwrap_or(0))
                    .saturating_add(size);
                if next_bytes > self.limits.spill_bytes {
                    return Err(limit("checkpoint disk artifacts", self.limits.spill_bytes));
                }
                state.bytes = next_bytes;
                state.artifacts.insert(view.relative_path.into(), size);
                previous
            };
            let written = octocode_io::atomic::write_atomic(&target, &bytes, false);
            let mut state = self.state();
            if let Err(error) = written {
                state.bytes = state.bytes.saturating_sub(size);
                match previous {
                    Some(previous) => {
                        state.bytes = state.bytes.saturating_add(previous);
                        state.artifacts.insert(view.relative_path.into(), previous);
                    }
                    None => {
                        state.artifacts.remove(view.relative_path);
                    }
                }
                return Err(failure(format!("cannot commit graph checkpoint: {error}")));
            }
            state.committed.insert(view.relative_path.into());
            Storage::Disk(target)
        };
        Ok(self.retain(view, source, size, storage))
    }

    /// The file's facts kept decoded when `size` more encoded bytes fit the
    /// retention budget, else `storage` to decode at [`Job::restore`].
    fn retain(
        &self,
        view: &GraphFactsView<'_>,
        source: capture::CapturedSource,
        size: u64,
        storage: Storage,
    ) -> super::scan::Kept {
        let fits = {
            let mut state = self.state();
            let next = state.retained_bytes.saturating_add(size);
            let fits = next <= self.limits.retained_bytes;
            if fits {
                state.retained_bytes = next;
            }
            fits
        };
        if fits {
            let marks = super::super::classify::SourceMarks::of(view.relative_path, view.content);
            return super::scan::Kept::Memory(Box::new((view.to_entry(), marks, source)));
        }
        super::scan::Kept::Spill(Kept {
            modules: (view.facts.language == "rust")
                .then(|| (view.relative_path.into(), view.facts.modules.clone())),
            digest: view.content_digest.into(),
            stamp: view.source_stamp,
            storage,
        })
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
            max_spill_bytes: self.limits.spill_bytes,
            max_memory_only_bytes: self.limits.memory_only_bytes,
        }
    }
}

/// Marks a published job complete, retaining its verified artifacts for reuse.
/// A job another build holds is that build's to finish: it is left as is.
pub(in crate::tools::ast_graph) fn complete(path: &Path) -> Result<(), ToolError> {
    let held = lock::open(&lock::job_lock_path(path))
        .map_err(|error| failure(format!("cannot lock graph checkpoint: {error}")))?;
    match held.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => return Ok(()),
        Err(std::fs::TryLockError::Error(error)) => {
            return Err(failure(format!("cannot lock graph checkpoint: {error}")));
        }
    }
    let manifest = Manifest::read(path)
        .ok_or_else(|| failure("cannot read graph checkpoint manifest for completion"))?;
    manifest.write(path, Stage::Complete)
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
