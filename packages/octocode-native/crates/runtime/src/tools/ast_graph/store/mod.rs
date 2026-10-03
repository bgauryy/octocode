//! Persisted code graphs: `octocode graph ingest` builds a snapshot once;
//! `octocode graph query` answers bounded questions from it without
//! rebuilding.
//!
//! ```text
//! <workspace>/.octocode/graph/
//!   latest                                   # id of the newest snapshot
//!   20260928T101500Z-packages-app/
//!     manifest.json                          # scope, counts, digests, gaps
//!     graph.bin                              # see `format.rs`
//! ```
//!
//! Snapshots are immutable. A build is written into a hidden temp directory
//! and renamed into place, so a reader never observes a partial snapshot.
mod classify;
mod detect;
mod format;
mod impact;
mod query;
mod tables;
mod workspace;

use super::graph::{BuildExtras, build_graph_with};
use super::types::AstTopologyQuery;
use crate::{
    policy::path::PathPolicy, security::ContentSecurity, tools::cancel::CancellationCheck,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

pub use query::{OPS, QueryOptions, query};

pub(crate) const GRAPH_FILE: &str = "graph.bin";
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
    "__pycache__",
    "site-packages",
    "Pods",
    "DerivedData",
    "storybook-static",
    "bower_components",
    "jspm_packages",
    "obj",
];

/// A command result: the JSON printed to stdout and the process exit code.
#[derive(Debug)]
pub struct GraphOutput {
    pub value: Value,
    pub exit: u8,
}

impl GraphOutput {
    pub(crate) fn ok(value: Value) -> Self {
        Self { value, exit: 0 }
    }
    pub(crate) fn error(exit: u8, code: &str, message: impl Into<String>) -> Self {
        Self {
            value: json!({"error": message.into(), "errorCode": code}),
            exit,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct IngestOptions {
    /// Directory to scan.
    pub path: PathBuf,
    /// Workspace that owns `.octocode/graph`; defaults to the nearest
    /// ancestor of the current directory holding `.git`, else the current
    /// directory.
    pub workspace: Option<PathBuf>,
    pub exclude_dir: Vec<String>,
    pub max_files: Option<u32>,
    /// Snapshots of the same scope to retain, newest first (default 3).
    pub keep: Option<usize>,
    /// Rebuild even when the latest snapshot of this scope is current.
    pub force: bool,
}

/// `<workspace>/.octocode/graph`, where the workspace is explicit or the
/// nearest `.git` ancestor of `cwd`.
pub(crate) fn graph_home(workspace: Option<&Path>) -> Result<PathBuf, String> {
    let workspace = match workspace {
        Some(path) => path.to_path_buf(),
        None => {
            let cwd = std::env::current_dir()
                .map_err(|error| format!("cannot read the current directory: {error}"))?;
            cwd.ancestors()
                .find(|dir| dir.join(".git").exists())
                .map(Path::to_path_buf)
                .unwrap_or(cwd)
        }
    };
    Ok(workspace.join(".octocode").join("graph"))
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
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let (year, month, day) = crate::civil_date::civil_from_days(secs.div_euclid(86_400));
    let rem = secs.rem_euclid(86_400);
    let (h, m, s) = (rem / 3600, rem % 3600 / 60, rem % 60);
    (
        format!("{year:04}{month:02}{day:02}T{h:02}{m:02}{s:02}Z"),
        format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z"),
    )
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
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

/// Removes older snapshots of `slug` beyond `keep` and abandoned temp dirs.
fn prune(home: &Path, slug: &str, keep: usize) -> Vec<String> {
    let mut removed = Vec::new();
    let same_scope = list_snapshots(home)
        .into_iter()
        .filter(|id| read_manifest(&home.join(id)).is_some_and(|m| m["scope"] == slug))
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
pub fn ingest(
    options: &IngestOptions,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
) -> GraphOutput {
    let started = Instant::now();
    let home = match graph_home(options.workspace.as_deref()) {
        Ok(home) => home,
        Err(message) => return GraphOutput::error(5, "graph.workspace", message),
    };
    let home = match paths.validate_output(&home) {
        Ok(valid) => valid.canonical,
        Err(error) => return GraphOutput::error(2, "graph.outputDenied", error.message),
    };
    let workspace = home
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let max_files = options.max_files.unwrap_or(DEFAULT_MAX_FILES);
    if !options.force
        && let Ok(root) = paths.validate(&options.path).map(|valid| valid.canonical)
        && let Some(receipt) = reuse_current(&home, &root, options, max_files)
    {
        let mut receipt = receipt;
        receipt["totalMs"] = json!(started.elapsed().as_millis() as u64);
        return GraphOutput::ok(receipt);
    }
    let mut request = json!({
        "analysis": "cycles",
        "path": options.path.to_string_lossy(),
        "maxFiles": max_files,
        "excludeDir": options.exclude_dir,
    });
    // Cargo metadata links `crate::`/workspace-crate imports; syntax-only
    // Rust linking would leave them unresolved.
    if options.path.join("Cargo.toml").is_file() {
        request["rustWorkspace"] = json!("cargo");
    }
    let query: AstTopologyQuery = match serde_json::from_value(request) {
        Ok(query) => query,
        Err(error) => return GraphOutput::error(2, "graph.input", error.to_string()),
    };
    let built = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        build_graph_with(
            &query,
            paths,
            security,
            cancel,
            &BuildExtras {
                respect_gitignore: true,
                extra_excludes: INGEST_EXCLUDES.iter().map(|x| (*x).to_owned()).collect(),
            },
        )
    })) {
        Ok(Ok(built)) => built,
        Ok(Err(error)) => {
            let exit = if error.code.contains("path") { 3 } else { 5 };
            return GraphOutput {
                value: json!({"error": error.message, "errorCode": error.code, "hints": error.hints}),
                exit,
            };
        }
        Err(_) => return GraphOutput::error(5, "graph.internal", "graph build panicked"),
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
    let project = workspace::Workspace::discover(&built.root, &files, &mains);
    let projection = tables::project(&built, &project);
    let (bytes, digest) = format::encode(&projection.tables);

    let slug = scope_slug(&built.root, &workspace);
    let (stamp, created_at) = utc_now();
    let mut id = format!("{stamp}-{slug}");
    let mut attempt = 1;
    while home.join(&id).exists() {
        attempt += 1;
        id = format!("{stamp}-{slug}-{attempt}");
    }
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
        build_ms,
    });
    let publish = || -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(&home)?;
        let tmp = home.join(format!(".tmp-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&tmp)?;
        std::fs::write(tmp.join(GRAPH_FILE), &bytes)?;
        let text = serde_json::to_vec_pretty(&manifest).unwrap_or_default();
        std::fs::write(tmp.join(MANIFEST_FILE), text)?;
        let target = home.join(&id);
        std::fs::rename(&tmp, &target)?;
        write_atomic(&home.join(LATEST_FILE), id.as_bytes())?;
        Ok(target)
    };
    let dir = match publish() {
        Ok(dir) => dir,
        Err(error) => {
            return GraphOutput::error(5, "graph.write", format!("cannot publish graph: {error}"));
        }
    };
    let pruned = prune(&home, &slug, options.keep.unwrap_or(DEFAULT_KEEP));
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

/// The latest snapshot of `root` when nothing it covers changed: same scan
/// options, same octocode and format version, the same file set (with the
/// ingest's exclusions and `.gitignore`), and identical content digests.
/// Hashing is far cheaper than parsing, so an unchanged tree returns in a
/// fraction of the ingest time.
fn reuse_current(
    home: &Path,
    root: &Path,
    options: &IngestOptions,
    max_files: u32,
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
        && manifest["scan"]["truncated"] == false;
    if !same_options {
        return None;
    }
    let dir = home.join(&id);
    let bytes = std::fs::read(dir.join(GRAPH_FILE)).ok()?;
    let (tables, digest) = format::decode(&bytes).ok()?;
    if manifest["graph"]["sha256"].as_str() != Some(digest.as_str()) {
        return None;
    }
    let recorded = tables
        .nodes
        .iter()
        .filter(|node| node.kind == format::NodeKind::File)
        .map(|node| tables.str(node.key))
        .collect::<std::collections::BTreeSet<_>>();
    let extensions = octocode_engine::signatures::graph_facts::graph_fact_extensions();
    let requested = INGEST_EXCLUDES
        .iter()
        .map(|name| (*name).to_owned())
        .chain(options.exclude_dir.iter().cloned())
        .collect::<Vec<_>>();
    let excluded = crate::policy::prune::PruneMode::SyntaxVisible
        .directories(&requested, true)
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    let walker = ignore::WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .parents(true)
        .filter_entry(move |entry| {
            !entry.file_type().is_some_and(|kind| kind.is_dir())
                || !excluded.contains(entry.file_name().to_string_lossy().as_ref())
        })
        .build();
    let mut current = std::collections::BTreeSet::new();
    for entry in walker.flatten() {
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let path = entry.path();
        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase());
        if !ext.is_some_and(|ext| extensions.contains(&ext)) {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .ok()?
            .to_string_lossy()
            .replace('\\', "/");
        current.insert(rel);
    }
    if current.len() != recorded.len()
        || !current.iter().all(|file| recorded.contains(file.as_str()))
    {
        return None;
    }
    for (node, digest) in &tables.digests {
        let file = tables.str(tables.nodes[*node as usize].key);
        let content = std::fs::read(root.join(file)).ok()?;
        if octocode_engine::index::content_digest(&content) != tables.str(*digest) {
            return None;
        }
    }
    write_atomic(&home.join(LATEST_FILE), id.as_bytes()).ok()?;
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
    build_ms: u64,
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
    json!({
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
            "truncated": input.built.truncated,
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
            "resolved": imports[0],
            "external": imports[1],
            "unresolvedInternal": imports[2],
            "unsupported": imports[3],
            "nonCode": imports[4],
        },
        "calls": input.projection.calls,
        "callInternalRecall": (input.projection.calls.internal_recall() * 1000.0).round() / 1000.0,
        "callRecallByLanguage": input.projection.calls.recall_by_language(),
        "languages": input.built.languages.iter().map(|(language, files, linking)| {
            json!({"language": language, "files": files, "linking": linking})
        }).collect::<Vec<_>>(),
        "diagnostics": {"total": tables.diagnostics.len(), "byCode": by_code},
        "buildMs": input.build_ms,
    })
}

#[cfg(test)]
mod tests;
