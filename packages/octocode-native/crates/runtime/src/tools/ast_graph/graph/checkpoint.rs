//! Durable extraction progress and its spill files. Linking is rebuilt from verified facts.
use super::*;
use octocode_engine::fact_scan::GraphFactsView;
use octocode_engine::graph::{GraphFactRustModule, GraphFactsTypedEntry};
use octocode_engine::num::saturating_usize;
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
const JOB_VERSION: u32 = 4;

/// The file's source judged whole at its path
/// ([`crate::security::JudgedText`]): the one redaction decision a read of
/// the file shows, and so the one its graph facts are stored under
/// ([`judged_facts`]). The scan reads the raw text once, not its
/// JSON-escaped facts, at any length (a text past the whole-source size is
/// judged in one pass over its spans). `Err` when the scan panicked, even if
/// the sanitizer recovered: what the source holds is then unknown.
pub(super) fn judged_source(
    security: &ContentSecurity,
    source: &str,
    path: &Path,
) -> Result<crate::security::JudgedText, octocode_engine::contain::Contained> {
    octocode_engine::contain::contain(octocode_engine::contain::Phase::SecretScan, || {
        #[cfg(test)]
        secret_scan_fault(source);
        crate::security::JudgedText::judge(source, path, security)
    })
}

/// `facts` as the judged source shows them: a fact is text of the source
/// with no place a cut could be taken at (a name, a string-literal receiver,
/// an import specifier), so every value the whole pass replaced is replaced
/// in it ([`crate::security::JudgedText::scrubbed`]). `None` when the pass
/// replaced nothing; `Err` when the facts could not be rewritten.
pub(super) fn judged_facts(
    facts: &octocode_engine::graph::GraphFactsDocument,
    judged: &crate::security::JudgedText,
) -> Result<Option<octocode_engine::graph::GraphFactsDocument>, serde_json::Error> {
    if !judged.is_redacted() {
        return Ok(None);
    }
    fn scrub(value: &mut serde_json::Value, judged: &crate::security::JudgedText) -> bool {
        match value {
            serde_json::Value::String(text) => match judged.scrubbed(text) {
                Some(clean) => {
                    *text = clean;
                    true
                }
                None => false,
            },
            serde_json::Value::Array(items) => items
                .iter_mut()
                .fold(false, |changed, item| scrub(item, judged) | changed),
            serde_json::Value::Object(fields) => fields
                .values_mut()
                .fold(false, |changed, field| scrub(field, judged) | changed),
            _ => false,
        }
    }
    let mut value = serde_json::to_value(facts)?;
    if !scrub(&mut value, judged) {
        return Ok(None);
    }
    serde_json::from_value(value).map(Some)
}

/// Sources holding one of these markers panic their secret scan (tests).
#[cfg(test)]
static SECRET_SCAN_FAULTS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Makes the secret scan of every source holding `marker` panic.
#[cfg(test)]
pub(in crate::tools::ast_graph) fn inject_secret_scan_fault(marker: &str) {
    SECRET_SCAN_FAULTS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(marker.to_owned());
}

#[cfg(test)]
fn secret_scan_fault(source: &str) {
    let faulted = SECRET_SCAN_FAULTS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .any(|marker| source.contains(marker.as_str()));
    assert!(!faulted, "injected secret scan fault");
}

pub(super) struct Job<'a> {
    dir: PathBuf,
    /// Held for the job's life: one build at a time writes a scope's job.
    _lock: octocode_io::lock::LockedFile,
    manifest: Manifest,
    resumed: bool,
    state: Mutex<State>,
    paths: &'a PathPolicy,
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

/// A file's facts left encoded until linking: its marks and source
/// correspondence, which linking keeps whole, stay decoded here.
pub(super) struct Kept {
    pub(super) modules: Option<(String, Vec<GraphFactRustModule>)>,
    pub(super) go_file: Option<(String, Option<String>, bool)>,
    digest: String,
    stamp: Option<octocode_engine::stamp::SourceStamp>,
    storage: Storage,
    marks: super::super::classify::SourceMarks,
    source: capture::CapturedSource,
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
        state::checked_path(self.paths, target)?;
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
        let extraction = artifact::decode(
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
    /// while the retention budget allows, else on disk (or, its source
    /// holding a secret, encoded in memory) to decode at [`Job::restore`].
    /// Encoding, the secret scan and the write run outside the job's lock,
    /// which only guards its counters. An artifact is its own commit record (header
    /// digest + pinned identity + complete payload), so it is written
    /// without a sync: a torn or lost write fails verification on the next
    /// build and is extracted again; only stage transitions are durable.
    /// Returns the kept facts and their artifact's encoded bytes, or the
    /// panic of the file's secret scan: then nothing of the file is kept or
    /// written (it fails closed).
    pub(super) fn keep(
        &self,
        view: &GraphFactsView<'_>,
        source: capture::CapturedSource,
        holds_secret: bool,
    ) -> Result<(super::scan::Kept, usize), ToolError> {
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
                return Ok((
                    self.retain(view, source, size, storage, holds_secret),
                    saturating_usize(size),
                ));
            }
        }
        // Past one artifact's bound the file is linked from its decoded
        // facts, as past the disk and memory-only budgets: only its
        // resumption is lost, never the build.
        let Some(bytes) = encode(&self.manifest.namespace, view, self.limits.artifact_bytes) else {
            let fact_bytes = octocode_engine::json_size::bytes_or(view.facts, usize::MAX);
            return Ok((decoded(view, source, holds_secret), fact_bytes));
        };
        let size = bytes.len() as u64;
        let storage = if holds_secret {
            let previous = {
                let mut state = self.state();
                if state.memory_bytes.saturating_add(size) > self.limits.memory_only_bytes {
                    // Past the budget for encoded copies, the file is linked
                    // from its decoded facts like an uncheckpointed build's:
                    // nothing is refused, only this file's resumption.
                    drop(state);
                    return Ok((decoded(view, source, holds_secret), bytes.len()));
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
            state::checked_path(self.paths, &target)?;
            // Reserve the quota first, so concurrent writers never pass it together.
            let previous = {
                let mut state = self.state();
                let previous = state.artifacts.get(view.relative_path).copied();
                let next_bytes = state
                    .bytes
                    .saturating_sub(previous.unwrap_or(0))
                    .saturating_add(size);
                if next_bytes > self.limits.spill_bytes {
                    // Past the disk budget the file is linked from its
                    // decoded facts, as past the memory-only budget.
                    drop(state);
                    return Ok((decoded(view, source, holds_secret), bytes.len()));
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
        Ok((
            self.retain(view, source, size, storage, holds_secret),
            saturating_usize(size),
        ))
    }

    /// The file's facts kept decoded when `size` more encoded bytes fit the
    /// retention budget, else `storage` to decode at [`Job::restore`].
    /// `hidden`: a read hides a value of the file ([`super::super::classify::ROLE_HIDDEN_VALUE`]).
    fn retain(
        &self,
        view: &GraphFactsView<'_>,
        source: capture::CapturedSource,
        size: u64,
        storage: Storage,
        hidden: bool,
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
        let marks = super::super::classify::SourceMarks::of(view.relative_path, view.content)
            .hiding(hidden);
        if fits {
            return super::scan::Kept::Memory(Box::new((view.to_entry(), marks, source)));
        }
        super::scan::Kept::Spill(Box::new(Kept {
            modules: (view.facts.language == "rust")
                .then(|| (view.relative_path.into(), view.facts.modules.clone())),
            go_file: (view.facts.language == "go").then(|| {
                (
                    view.relative_path.to_owned(),
                    view.facts.go_package.clone(),
                    view.facts.build_excluded,
                )
            }),
            digest: view.content_digest.into(),
            stamp: view.source_stamp,
            storage,
            marks,
            source,
        }))
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
        let mut entry = artifact.into_entry();
        entry.source_stamp = kept.stamp;
        Ok((entry, kept.marks, kept.source))
    }

    pub(super) fn linking(&self) -> Result<(), ToolError> {
        self.manifest.write(&self.dir, Stage::Linking)
    }

    pub(super) fn progress(&self) -> BuildProgress {
        let state = self.state();
        BuildProgress {
            checkpoint_generation: Some(self.manifest.generation.clone()),
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

/// Releases a published build's extraction artifacts and their lock file.
/// A held or newer job belongs to its own build, even if the older build
/// published first.
pub(in crate::tools::ast_graph) fn complete(
    path: &Path,
    generation: &str,
) -> Result<(), ToolError> {
    let lock_path = lock::job_lock_path(path);
    let Some(held) = lock::try_job(&lock_path)
        .map_err(|error| failure(format!("cannot lock graph checkpoint: {error}")))?
    else {
        return Ok(());
    };
    if !path.exists() {
        let _ = held.remove_held(&lock_path);
        return Ok(());
    }
    let manifest = Manifest::read(path)
        .ok_or_else(|| failure("cannot read graph checkpoint manifest for completion"))?;
    if manifest.generation != generation {
        return Ok(());
    }
    // A crash during removal must never turn completed work back into an
    // eligible checkpoint on the next open.
    manifest.write(path, Stage::Complete)?;
    std::fs::remove_dir_all(path)
        .map_err(|error| failure(format!("cannot remove completed graph checkpoint: {error}")))?;
    let _ = held.remove_held(&lock_path);
    if let Some(parent) = path.parent() {
        state::sync_dir(parent)?;
    }
    Ok(())
}

/// Removes, under `base` (a graph home's `.facts`), every extraction job no
/// build holds that no build can resume: one in a form this build cannot
/// read, one whose scope root is gone, or one `superseded` says a snapshot
/// of its root published after its last write replaced; and every lock file
/// whose job is gone. A job a build holds is never touched. Returns the
/// bytes of the unfinished jobs left on disk.
pub(in crate::tools::ast_graph) fn prune_jobs(
    base: &Path,
    superseded: &dyn Fn(&Path, std::time::SystemTime) -> bool,
) -> u64 {
    let Ok(entries) = std::fs::read_dir(base.join("jobs")) else {
        return 0;
    };
    let mut kept = 0_u64;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            kept = kept.saturating_add(prune_job(&path, superseded));
        } else if kind.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension == "lock")
        {
            let job = path.with_extension("");
            let _ = octocode_io::lock::remove_unheld(&path, || !job.exists());
        }
    }
    kept
}

/// [`prune_jobs`] for the job at `dir`: the bytes it keeps, 0 once removed.
fn prune_job(dir: &Path, superseded: &dyn Fn(&Path, std::time::SystemTime) -> bool) -> u64 {
    let bytes = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.metadata().ok())
        .map(|metadata| metadata.len())
        .sum::<u64>();
    let lock_path = lock::job_lock_path(dir);
    let Ok(Some(held)) = lock::try_job(&lock_path) else {
        return bytes;
    };
    let stale = match Manifest::read(dir) {
        Some(manifest) if manifest.version == JOB_VERSION => {
            let root = Path::new(&manifest.root);
            let written = std::fs::metadata(dir.join("job.json")).and_then(|meta| meta.modified());
            !root.exists() || written.is_ok_and(|time| superseded(root, time))
        }
        // A newer build's job is that build's to resume, as a newer
        // snapshot format is ([`super::super::store`] retention); an older
        // one, or one that never finished opening, no build resumes.
        _ => state::Manifest::version_of(dir).is_none_or(|version| version < JOB_VERSION),
    };
    if !stale || std::fs::remove_dir_all(dir).is_err() {
        return bytes;
    }
    let _ = held.remove_held(&lock_path);
    0
}

/// The file's facts kept decoded, as an uncheckpointed build keeps them;
/// `hidden` as [`Job::retain`]'s.
fn decoded(
    view: &GraphFactsView<'_>,
    source: capture::CapturedSource,
    hidden: bool,
) -> super::scan::Kept {
    let marks =
        super::super::classify::SourceMarks::of(view.relative_path, view.content).hiding(hidden);
    super::scan::Kept::Memory(Box::new((view.to_entry(), marks, source)))
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
