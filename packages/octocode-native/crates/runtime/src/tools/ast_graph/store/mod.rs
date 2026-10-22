//! Persisted code graphs: `octocode graph ingest` builds a snapshot once;
//! `octocode graph query` answers bounded questions from it without
//! rebuilding.
//!
//! ```text
//! <workspace>/.octocode/graph/
//!   latest                                   # id of the newest snapshot
//!   20260928T101500Z-packages-app-1a2b3c/     # time, scope, root tag
//!     manifest.json                          # scope, counts, digests, gaps
//!     graph.bin                              # see `format.rs`
//! ```
//!
//! Snapshots are immutable. A build is written into a hidden temp directory
//! and renamed into place, so a reader never observes a partial snapshot.
pub(super) mod classify;
mod detect;
mod drift;
mod format;
mod impact;
mod query;
pub(super) mod tables;
mod workspace;

use super::graph::{GraphScan, build_graph};
use super::types::ImportOutcome;
use crate::response::exit::ExitClass;
use crate::{
    execution::cancel::CancellationCheck, policy::path::PathPolicy, security::ContentSecurity,
};
use octocode_contract_types::ErrorCode;
use octocode_io::atomic::write_atomic;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    time::Instant,
};

pub use query::{OPS, QueryOptions, query};

pub(crate) const GRAPH_FILE: &str = "graph.bin";
/// Published snapshots beyond this size require a narrower ingest scope.
/// Refuse them before decode can multiply allocations from untrusted lengths.
const MAX_GRAPH_BYTES: usize = 512 * 1024 * 1024;

fn read_graph_file(path: &Path) -> Result<Vec<u8>, String> {
    crate::tools::source::read_bounded(path, MAX_GRAPH_BYTES).map_err(|error| {
        format!(
            "cannot read {} within the {}-byte snapshot limit: {error:?}; narrow the graph ingest scope",
            path.display(),
            MAX_GRAPH_BYTES
        )
    })
}
pub(crate) const MANIFEST_FILE: &str = "manifest.json";
const LATEST_FILE: &str = "latest";
const MANIFEST_KIND: &str = "octocode.graph";
const DEFAULT_MAX_FILES: u32 = 50_000;
const DEFAULT_KEEP: usize = 3;
/// Environment and tool-output directories the ingest prunes on top of the
/// shared syntax-visible policy and `.gitignore` (hidden directories are never
/// scanned).
const INGEST_EXCLUDES: &[&str] = &[
    "venv",
    "site-packages",
    "Pods",
    "storybook-static",
    "bower_components",
    "jspm_packages",
    "obj",
];

/// A command result: the JSON printed to stdout and how the command ended.
#[derive(Debug)]
pub struct GraphOutput {
    pub value: Value,
    pub class: ExitClass,
}

impl GraphOutput {
    pub(crate) fn ok(value: Value) -> Self {
        Self {
            value,
            class: ExitClass::Success,
        }
    }
    pub(crate) fn empty(value: Value) -> Self {
        Self {
            value,
            class: ExitClass::Empty,
        }
    }
    /// A failure with a declared `errorCodes` entry; its class decides the exit.
    pub fn error(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            value: json!({"error": message.into(), "errorCode": code}),
            class: ExitClass::of(code),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct IngestOptions {
    /// Directory to scan.
    pub path: PathBuf,
    /// Workspace that owns `.octocode/graph`. The runtime fills it when
    /// unset ([`default_workspace`] of its working directory).
    pub workspace: Option<PathBuf>,
    pub exclude_dir: Vec<String>,
    pub max_files: Option<u32>,
    /// Snapshots of the same root to retain, newest first (default 3).
    pub keep: Option<usize>,
    /// Rebuild even when the latest snapshot of this scope is current.
    pub force: bool,
    /// AST parsers by root-relative glob (`{"cpp": ["include/**/*.h"]}`).
    pub language_globs: BTreeMap<String, Vec<String>>,
}

/// `<workspace>/.octocode/graph`. The workspace is the caller's: the
/// runtime names one ([`default_workspace`] of its own working directory)
/// when the call does not, so the process directory never decides it.
pub(crate) fn graph_home(workspace: Option<&Path>) -> Result<PathBuf, String> {
    let workspace =
        workspace.ok_or("no workspace: name the directory that owns .octocode/graph")?;
    Ok(workspace.join(".octocode").join("graph"))
}

/// The nearest repository root at or above `cwd`
/// ([`octocode_engine::search::repository`]) that `paths` allows (else `cwd`),
/// so a run from a repository subdirectory stays inside the allowed roots.
pub(crate) fn default_workspace(cwd: PathBuf, paths: &PathPolicy) -> PathBuf {
    cwd.ancestors()
        .find(|dir| {
            octocode_engine::search::repository::is_root(dir) && paths.validate(dir).is_ok()
        })
        .map(Path::to_path_buf)
        .unwrap_or(cwd)
}

/// Filesystem-safe scope name: the scanned root relative to the workspace,
/// or its last two components when it lies outside.
fn scope_slug(root: &Path, workspace: &Path) -> String {
    let relative = root
        .strip_prefix(workspace)
        .ok()
        .map(|rel| rel.to_string_lossy().into_owned())
        .filter(|rel| !rel.is_empty());
    let raw = relative.unwrap_or_else(|| {
        let parts = root
            .components()
            .rev()
            .take(2)
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        parts.into_iter().rev().collect::<Vec<_>>().join("-")
    });
    let mut slug = String::new();
    for ch in raw.chars() {
        let ch = if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_') {
            ch
        } else {
            '-'
        };
        if ch != '-' || !slug.ends_with('-') {
            slug.push(ch);
        }
    }
    let slug = slug
        .trim_matches(['-', '.'])
        .chars()
        .take(64)
        .collect::<String>();
    if slug.is_empty() { "root".into() } else { slug }
}

/// `(compact, iso)` UTC timestamps: `20260928T101500Z`, `2026-09-28T10:15:00Z`.
fn utc_now() -> (String, String) {
    let iso = crate::civil_date::iso8601_secs(crate::civil_date::unix_secs());
    (iso.replace(['-', ':'], ""), iso)
}

/// Whether `dir` is a published snapshot (has a manifest of our kind).
fn read_manifest(dir: &Path) -> Option<Value> {
    std::fs::read(dir.join(MANIFEST_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .filter(|manifest| manifest["kind"] == MANIFEST_KIND)
}

fn is_snapshot(dir: &Path) -> bool {
    read_manifest(dir).is_some()
}

/// Published snapshot ids, oldest first (ids start with a sortable time).
pub(crate) fn list_snapshots(home: &Path) -> Vec<String> {
    let mut ids = std::fs::read_dir(home)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| !name.starts_with('.') && is_snapshot(&home.join(name)))
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

/// Removes older snapshots of `root` beyond `keep` and abandoned temp dirs.
/// Snapshots are grouped by the root they scanned, never by their scope
/// name: two roots that share a name keep their own snapshots.
fn prune(home: &Path, root: &Path, keep: usize) -> Vec<String> {
    let mut removed = Vec::new();
    let root = root.to_string_lossy();
    let same_scope = list_snapshots(home)
        .into_iter()
        .filter(|id| read_manifest(&home.join(id)).is_some_and(|m| m["root"] == root.as_ref()))
        .collect::<Vec<_>>();
    let excess = same_scope.len().saturating_sub(keep.max(1));
    for id in same_scope.into_iter().take(excess) {
        if std::fs::remove_dir_all(home.join(&id)).is_ok() {
            removed.push(id);
        }
    }
    let stale_after = std::time::Duration::from_secs(3600);
    for entry in std::fs::read_dir(home).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let old = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|time| time.elapsed().ok())
            .is_some_and(|age| age > stale_after);
        if name.starts_with(".tmp-") && old {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
    removed
}

/// Builds the graph for `options.path` and publishes a new snapshot.
/// `cargo` is the configured `OCTOCODE_CARGO` for Rust workspace linking.
pub fn ingest(
    options: &IngestOptions,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
    cargo: Option<&str>,
) -> GraphOutput {
    let started = Instant::now();
    let home = match graph_home(options.workspace.as_deref()) {
        Ok(home) => home,
        Err(message) => return GraphOutput::error(ErrorCode::WorkspaceRootInvalid, message),
    };
    let home = match paths.validate_output(&home) {
        Ok(valid) => valid.canonical,
        Err(error) => {
            return GraphOutput::error(
                error.local_error_code(ErrorCode::PathPolicyDenied),
                error.message,
            );
        }
    };
    let workspace = home
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let max_files = options.max_files.unwrap_or(DEFAULT_MAX_FILES);
    if !options.force
        && let Ok(root) = paths.validate(&options.path).map(|valid| valid.canonical)
        && let Some(receipt) = reuse_current(&home, &root, options, max_files, paths)
    {
        let mut receipt = receipt;
        receipt["totalMs"] = json!(started.elapsed().as_millis() as u64);
        return GraphOutput::ok(receipt);
    }
    let built = match build_for_ingest(options, paths, security, cancel, max_files, cargo) {
        Ok(built) => built,
        Err(output) => return output,
    };
    let build_ms = started.elapsed().as_millis() as u64;
    let files = built.nodes.keys().map(String::as_str).collect::<Vec<_>>();
    let mains = built
        .facts
        .iter()
        .filter(|(_, facts)| {
            facts
                .declarations
                .iter()
                .any(|d| matches!(d.name.as_str(), "main" | "Main") && d.kind != "module")
        })
        .map(|(file, _)| file.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let marks = |file: &str| {
        built
            .facts
            .get(file)
            .map(|facts| facts.marks)
            .unwrap_or_default()
    };
    let project = workspace::Workspace::discover(&built.root, &files, &mains, &marks, &|path| {
        super::aliases::read_config_text(paths, security, path)
    });
    let mut projection = tables::project(&built, &project);
    redact_secrets(&mut projection.tables, security);
    let (bytes, digest) = format::encode(&projection.tables);
    if bytes.len() > MAX_GRAPH_BYTES {
        return GraphOutput::error(
            ErrorCode::ScopeTooBroad,
            format!(
                "encoded graph exceeds the {MAX_GRAPH_BYTES}-byte snapshot limit; ingest a narrower scope"
            ),
        );
    }

    let slug = scope_slug(&built.root, &workspace);
    let (stamp, created_at) = utc_now();
    // Roots whose names map to one slug (`a-b`, `a/b`) still get distinct ids.
    let root_tag = &octocode_engine::digest::sha256(built.root.to_string_lossy().as_bytes())[..6];
    let mut id = format!("{stamp}-{slug}-{root_tag}");
    let mut attempt = 1;
    while home.join(&id).exists() {
        attempt += 1;
        id = format!("{stamp}-{slug}-{root_tag}-{attempt}");
    }
    let inputs = snapshot_inputs(paths, &built.root, &projection.tables);
    let manifest = manifest(&ManifestInput {
        id: &id,
        scope: &slug,
        created_at: &created_at,
        workspace: &workspace,
        built: &built,
        projection: &projection,
        bytes: bytes.len(),
        digest: &digest,
        max_files,
        exclude_dir: &options.exclude_dir,
        language_globs: &options.language_globs,
        inputs: &inputs,
        build_ms,
    });
    let dir = match publish(&home, &id, &stamp, &bytes, &manifest) {
        Ok(dir) => dir,
        Err(error) => {
            return GraphOutput::error(
                ErrorCode::FileAccessFailed,
                format!("cannot publish graph: {error}"),
            );
        }
    };
    let pruned = prune(&home, &built.root, options.keep.unwrap_or(DEFAULT_KEEP));
    let mut receipt = json!({
        "id": id,
        "dir": dir.to_string_lossy(),
        "root": built.root.to_string_lossy(),
        "bytes": bytes.len(),
        "sha256": digest,
        "counts": manifest["counts"],
        "calls": manifest["calls"],
        "callInternalRecall": manifest["callInternalRecall"],
        "scan": manifest["scan"],
        "buildMs": manifest["buildMs"],
        "totalMs": started.elapsed().as_millis() as u64,
        "next": format!("octocode graph query stats --graph {id}"),
    });
    if manifest["diagnostics"]["total"].as_u64().unwrap_or(0) > 0 {
        receipt["diagnostics"] = manifest["diagnostics"]["byCode"].clone();
    }
    if !pruned.is_empty() {
        receipt["pruned"] = json!(pruned);
    }
    GraphOutput::ok(receipt)
}

/// The linked graph of `options.path` under the ingest's exclusions and
/// `.gitignore`.
fn build_for_ingest(
    options: &IngestOptions,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
    max_files: u32,
    cargo: Option<&str>,
) -> Result<super::types::BuiltGraph, GraphOutput> {
    let scan = GraphScan {
        root: options.path.clone(),
        exclude_dir: INGEST_EXCLUDES
            .iter()
            .map(|x| (*x).to_owned())
            .chain(options.exclude_dir.iter().cloned())
            .collect(),
        max_files,
        language_globs: options.language_globs.clone(),
        // Cargo metadata links `crate::`/workspace-crate imports; syntax-only
        // Rust linking would leave them unresolved.
        cargo_metadata: options.path.join("Cargo.toml").is_file(),
        cargo: cargo.map(str::to_owned),
    };
    // The builder upholds map-index invariants with `expect()`; a corrupt or
    // adversarial input tripping one is an error, not a torn-down host.
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        build_graph(&scan, paths, security, cancel)
    })) {
        Ok(Ok(built)) => Ok(built),
        Ok(Err(error)) => Err(GraphOutput {
            class: ExitClass::of(error.code),
            value: json!({"error": error.message, "errorCode": error.code, "hints": error.hints}),
        }),
        Err(_) => Err(GraphOutput::error(
            ErrorCode::ExecutionFailed,
            "graph build panicked",
        )),
    }
}

/// Bytes of joined strings one secret scan reads. The scanner takes bounded
/// text, and most snapshots hold no secret, so a clean batch is cleared in
/// one scan.
const SECRET_SCAN_BATCH: usize = 1 << 20;

/// Redacts every string of `tables` the secret scanner flags (a token used as
/// an import specifier is a package name; a diagnostic can quote source), so
/// no secret of the source reaches the disk, then re-sorts the lookups the
/// new strings reorder. Digest strings are hashes the staleness check
/// compares, never source text, so they are kept as they are.
fn redact_secrets(tables: &mut format::GraphTables, security: &ContentSecurity) {
    let digests = tables
        .digests
        .iter()
        .map(|(_, string)| *string as usize)
        .collect::<std::collections::HashSet<_>>();
    let strings = &tables.strings;
    let mut batches = Vec::new();
    let mut start = 0;
    while start < strings.len() {
        let mut end = start;
        let mut size = 0;
        while end < strings.len() && (end == start || size < SECRET_SCAN_BATCH) {
            size += strings[end].len() + 1;
            end += 1;
        }
        batches.push(start..end);
        start = end;
    }
    // Batches are independent: scan them on every core.
    let next = std::sync::atomic::AtomicUsize::new(0);
    let redacted = std::thread::scope(|scope| {
        let workers = (0..crate::tools::workers::workers(None, batches.len(), None))
            .map(|_| {
                scope.spawn(|| {
                    let mut redacted = Vec::new();
                    while let Some(batch) =
                        batches.get(next.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
                    {
                        let scanned = batch.clone().filter(|index| !digests.contains(index));
                        let text = scanned
                            .clone()
                            .map(|index| strings[index].as_str())
                            .collect::<Vec<_>>()
                            .join("\n");
                        if security.sanitize_text(&text, None).has_secrets {
                            redacted.extend(scanned.filter_map(|index| {
                                let result = security.sanitize_text(&strings[index], None);
                                result.has_secrets.then_some((index, result.content))
                            }));
                        }
                    }
                    redacted
                })
            })
            .collect::<Vec<_>>();
        workers
            .into_iter()
            .flat_map(|worker| {
                // A failed scan must not publish unredacted strings.
                worker
                    .join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
            })
            .collect::<Vec<_>>()
    });
    if redacted.is_empty() {
        return;
    }
    for (index, content) in redacted {
        tables.strings[index] = content;
    }
    tables.index();
}

/// Writes the snapshot under a temp dir, renames it into place and points
/// the latest marker at it.
fn publish(
    home: &Path,
    id: &str,
    stamp: &str,
    bytes: &[u8],
    manifest: &Value,
) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(home)?;
    let tmp = home.join(format!(".tmp-{}-{stamp}", std::process::id()));
    std::fs::create_dir_all(&tmp)?;
    std::fs::write(tmp.join(GRAPH_FILE), bytes)?;
    let text = serde_json::to_vec_pretty(manifest).unwrap_or_default();
    std::fs::write(tmp.join(MANIFEST_FILE), text)?;
    let target = home.join(id);
    std::fs::rename(&tmp, &target)?;
    write_atomic(&home.join(LATEST_FILE), id.as_bytes(), false)?;
    Ok(target)
}

/// Whether the file at `path` still holds the bytes `digest` names, `None`
/// when it cannot be read. A stamp recorded at ingest that still equals the
/// file's size and change times proves it without a read (the stamp was
/// taken only for a settled file, see [`octocode_engine::stamp::SourceStamp`]);
/// any difference, or no stamp, falls back to hashing the content.
pub(crate) fn source_matches(
    path: &Path,
    digest: &str,
    stamp: Option<&octocode_engine::stamp::SourceStamp>,
) -> Option<bool> {
    // Ingest never reads a source beyond this bound. A different size cannot
    // have the recorded digest, and a file grown past the parser bound must
    // invalidate reuse without allocating it in full.
    const MAX_INGEST_SOURCE_BYTES: usize =
        octocode_engine::fact_scan::MAX_GRAPH_SOURCE_BYTES as usize;
    let meta = std::fs::metadata(path).ok()?;
    let current_size = meta.len();
    if current_size > MAX_INGEST_SOURCE_BYTES as u64
        || stamp.is_some_and(|recorded| recorded.size != current_size)
    {
        return Some(false);
    }
    if stamp.is_some() && octocode_engine::stamp::SourceStamp::of(&meta).as_ref() == stamp {
        return Some(true);
    }
    let content = crate::tools::source::read_bounded(path, MAX_INGEST_SOURCE_BYTES).ok()?;
    Some(octocode_engine::digest::sha256(&content) == digest)
}

/// The latest snapshot of `root` when nothing it covers changed: same scan
/// options, same octocode and format version, and no [`snapshot_changes`].
fn reuse_current(
    home: &Path,
    root: &Path,
    options: &IngestOptions,
    max_files: u32,
    paths: &PathPolicy,
) -> Option<Value> {
    let root_text = root.to_string_lossy();
    let (id, manifest) = list_snapshots(home).into_iter().rev().find_map(|id| {
        let manifest = read_manifest(&home.join(&id))?;
        (manifest["root"] == root_text.as_ref()).then_some((id, manifest))
    })?;
    let same_options = manifest["formatVersion"] == format::FORMAT_VERSION
        && manifest["octocodeVersion"] == env!("CARGO_PKG_VERSION")
        && manifest["scan"]["maxFiles"] == max_files
        && manifest["scan"]["excludeDir"] == json!(options.exclude_dir)
        && manifest["scan"]["languageGlobs"] == language_globs_json(&options.language_globs)
        && manifest["scan"]["truncated"] == false;
    if !same_options {
        return None;
    }
    let dir = home.join(&id);
    let bytes = read_graph_file(&dir.join(GRAPH_FILE)).ok()?;
    let (tables, digest) = format::decode_files(&bytes).ok()?;
    if manifest["graph"]["sha256"].as_str() != Some(digest.as_str()) {
        return None;
    }
    let freshness = snapshot_changes(paths, root, &tables, &manifest, true);
    if !freshness.changes.is_empty() || !freshness.added_checked {
        return None;
    }
    write_atomic(&home.join(LATEST_FILE), id.as_bytes(), false).ok()?;
    Some(json!({
        "id": id,
        "dir": dir.to_string_lossy(),
        "root": manifest["root"],
        "reused": true,
        "bytes": manifest["graph"]["bytes"],
        "sha256": manifest["graph"]["sha256"],
        "counts": manifest["counts"],
        "calls": manifest["calls"],
        "callInternalRecall": manifest["callInternalRecall"],
        "scan": manifest["scan"],
        "next": format!("octocode graph query stats --graph {id}"),
    }))
}

/// The linking configs a build of the snapshot's files read (see
/// [`super::freshness::config_inputs`]), recorded in its manifest.
fn snapshot_inputs(
    paths: &PathPolicy,
    root: &Path,
    tables: &format::GraphTables,
) -> BTreeMap<String, String> {
    let files = tables
        .nodes
        .iter()
        .filter(|node| node.kind == format::NodeKind::File)
        .map(|node| tables.str(node.key));
    super::freshness::config_inputs(paths, root, &super::freshness::scope_dirs(root, files))
}

/// The source files a re-ingest of `root` would scan, root-relative: the
/// ingest's exclusions and `.gitignore`, graph-fact extensions and the files
/// its language globs name.
fn scope_files(
    root: &Path,
    exclude_dir: &[String],
    language_globs: &BTreeMap<String, Vec<String>>,
) -> BTreeSet<String> {
    let extensions = octocode_engine::signatures::graph_facts::graph_fact_extensions();
    let globs = crate::tools::ast_rule::GlobParsers::compile(
        root,
        (!language_globs.is_empty()).then_some(language_globs),
    )
    .unwrap_or_default();
    let excluded = crate::tools::syntax_prune(
        INGEST_EXCLUDES
            .iter()
            .map(|name| (*name).to_owned())
            .chain(exclude_dir.iter().cloned()),
    );
    let mut current = BTreeSet::new();
    for entry in crate::tools::pruned_walk(root, excluded).build().flatten() {
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let path = entry.path();
        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase());
        if !ext.is_some_and(|ext| extensions.contains(&ext)) && !globs.names(path) {
            continue;
        }
        if let Ok(rel) = path.strip_prefix(root) {
            current.insert(octocode_engine::text::slash_path(rel));
        }
    }
    current
}

/// One way the tree differs from a snapshot of it.
pub(crate) struct Change {
    pub file: String,
    /// `changed`, `missing`, `added`, or `excluded` (still present, but a
    /// re-ingest would no longer scan it).
    pub status: &'static str,
    /// A linking config (manifest, tsconfig, ...), not a parsed source.
    pub config: bool,
}

pub(crate) struct Freshness {
    pub changes: Vec<Change>,
    /// Files added since ingest were looked for; a truncated scan cannot
    /// tell an added file from one past `maxFiles`.
    pub added_checked: bool,
}

/// Every way the tree under `root` differs from the snapshot `tables`
/// (`manifest` its manifest): a source edited or removed (digests, a settled
/// stamp skipping the hash), a source added (a rescan with the ingest's
/// exclusions), and a linking config edited, removed or added. The one
/// freshness check behind ingest reuse and `query stale`; `first_only`
/// stops at the first difference.
pub(crate) fn snapshot_changes(
    paths: &PathPolicy,
    root: &Path,
    tables: &format::GraphTables,
    manifest: &Value,
    first_only: bool,
) -> Freshness {
    let mut changes = Vec::new();
    let done = |changes: &Vec<Change>| first_only && !changes.is_empty();
    let added_checked = manifest["scan"]["truncated"] == false;
    let stamps = tables.stamps_by_node();
    for (node, digest) in &tables.digests {
        let file = tables.str(tables.nodes[*node as usize].key);
        let status = match source_matches(&root.join(file), tables.str(*digest), stamps.get(node)) {
            None => "missing",
            Some(true) => continue,
            Some(false) => "changed",
        };
        changes.push(Change {
            file: file.to_owned(),
            status,
            config: false,
        });
        if done(&changes) {
            return Freshness {
                changes,
                added_checked,
            };
        }
    }
    if added_checked {
        let recorded = tables
            .nodes
            .iter()
            .filter(|node| node.kind == format::NodeKind::File)
            .map(|node| tables.str(node.key))
            .collect::<BTreeSet<_>>();
        let excludes = manifest["scan"]["excludeDir"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let globs = serde_json::from_value::<BTreeMap<String, Vec<String>>>(
            manifest["scan"]["languageGlobs"].clone(),
        )
        .unwrap_or_default();
        let current = scope_files(root, &excludes, &globs);
        let reported = changes
            .iter()
            .map(|change| change.file.clone())
            .collect::<BTreeSet<_>>();
        // Recorded files a re-ingest would no longer scan: gone, or now
        // ignored or excluded.
        let excluded = recorded
            .iter()
            .filter(|file| !current.contains(**file) && !reported.contains(**file))
            .map(|file| {
                let status = if root.join(file).exists() {
                    "excluded"
                } else {
                    "missing"
                };
                ((*file).to_owned(), status)
            });
        let added = current
            .iter()
            .filter(|file| !recorded.contains(file.as_str()))
            .map(|file| (file.clone(), "added"));
        for (file, status) in added.chain(excluded).collect::<Vec<_>>() {
            changes.push(Change {
                file,
                status,
                config: false,
            });
            if done(&changes) {
                return Freshness {
                    changes,
                    added_checked,
                };
            }
        }
    }
    let recorded = manifest["inputs"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(key, digest)| Some((key.clone(), digest.as_str()?.to_owned())))
        .collect::<BTreeMap<_, _>>();
    let current = snapshot_inputs(paths, root, tables);
    for (file, status) in super::freshness::config_changes(&recorded, &current) {
        changes.push(Change {
            file,
            status,
            config: true,
        });
        if done(&changes) {
            break;
        }
    }
    Freshness {
        changes,
        added_checked,
    }
}

struct ManifestInput<'a> {
    id: &'a str,
    scope: &'a str,
    created_at: &'a str,
    workspace: &'a Path,
    built: &'a super::types::BuiltGraph,
    projection: &'a tables::Projection,
    bytes: usize,
    digest: &'a str,
    max_files: u32,
    exclude_dir: &'a [String],
    language_globs: &'a BTreeMap<String, Vec<String>>,
    inputs: &'a BTreeMap<String, String>,
    build_ms: u64,
}

/// The manifest's `scan.languageGlobs`: absent when no glob was given.
fn language_globs_json(globs: &BTreeMap<String, Vec<String>>) -> Value {
    if globs.is_empty() {
        Value::Null
    } else {
        json!(globs)
    }
}

fn manifest(input: &ManifestInput) -> Value {
    let tables = &input.projection.tables;
    let mut by_node = BTreeMap::<&str, u64>::new();
    for node in &tables.nodes {
        *by_node.entry(node.kind.as_str()).or_default() += 1;
    }
    let mut by_edge = BTreeMap::<&str, u64>::new();
    for edge in &tables.edges {
        *by_edge.entry(edge.kind.as_str()).or_default() += 1;
    }
    let mut by_code = BTreeMap::<&str, u64>::new();
    for diag in &tables.diagnostics {
        *by_code.entry(tables.str(diag.code)).or_default() += 1;
    }
    let imports = input.built.imports;
    let mut manifest = json!({
        "kind": MANIFEST_KIND,
        "formatVersion": format::FORMAT_VERSION,
        "id": input.id,
        "scope": input.scope,
        "createdAt": input.created_at,
        "octocodeVersion": env!("CARGO_PKG_VERSION"),
        "root": input.built.root.to_string_lossy(),
        "workspace": input.workspace.to_string_lossy(),
        "graph": {"file": GRAPH_FILE, "bytes": input.bytes, "sha256": input.digest},
        "evidence": "syntax: imports are linked by module resolution; calls are name-resolved candidates tagged with resolution and confidence — confirm identity with lspSearch",
        "scan": {
            "filesScanned": input.built.facts.len(),
            "filesSkipped": input.built.files_skipped,
            // The file scan stopped at `maxFiles`; the edge cap is apart:
            // more files cannot lift it, and it leaves the scan complete.
            "truncated": input.built.truncated,
            "edgesCapped": input.built.edges_capped,
            "maxFiles": input.max_files,
            "excludeDir": input.exclude_dir,
        },
        "counts": {
            "nodes": tables.nodes.len(),
            "edges": tables.edges.len(),
            "nodesByKind": by_node,
            "edgesByKind": by_edge,
        },
        "imports": {
            "resolved": imports[ImportOutcome::Resolved as usize],
            "external": imports[ImportOutcome::External as usize],
            "unresolvedInternal": imports[ImportOutcome::UnresolvedInternal as usize],
            "unsupported": imports[ImportOutcome::Unsupported as usize],
            "nonCode": imports[ImportOutcome::NonCode as usize],
        },
        "calls": input.projection.calls,
        "callInternalRecall": (input.projection.calls.internal_recall() * 1000.0).round() / 1000.0,
        "callRecallByLanguage": input.projection.calls.recall_by_language(),
        "languages": input.built.languages.iter().map(|(language, files, linking)| {
            json!({"language": language, "files": files, "linking": linking})
        }).collect::<Vec<_>>(),
        "diagnostics": {"total": tables.diagnostics.len(), "byCode": by_code},
        "inputs": input.inputs,
        "buildMs": input.build_ms,
    });
    if !input.language_globs.is_empty() {
        manifest["scan"]["languageGlobs"] = json!(input.language_globs);
    }
    manifest
}

#[cfg(test)]
mod tests;
