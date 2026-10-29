//! Whether a snapshot still matches its sources: stamps, digests, inputs and
//! the change list.
use super::*;

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
pub(super) fn reuse_current(
    home: &Path,
    root: &Path,
    options: &IngestOptions,
    max_files: u32,
    paths: &PathPolicy,
    registry_identity: &str,
) -> Option<Value> {
    let root_text = root.to_string_lossy();
    let (id, manifest) = list_snapshots(home).into_iter().rev().find_map(|id| {
        let manifest = read_manifest(&home.join(&id))?;
        (manifest["root"] == root_text.as_ref()).then_some((id, manifest))
    })?;
    let same_options = manifest["formatVersion"] == format::FORMAT_VERSION
        && analysis_current(&manifest)
        && manifest["registryIdentity"] == registry_identity
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
        "diagnostics": manifest["diagnostics"]["byCode"],
        "buildMs": 0,
        "bytes": manifest["graph"]["bytes"],
        "sha256": manifest["graph"]["sha256"],
        "counts": manifest["counts"],
        "calls": manifest["calls"],
        "argumentCoverage": manifest["argumentCoverage"],
        "extraction": {"parsedFiles":0,"reusedFiles":manifest["scan"]["filesScanned"],"resumedFiles":0},
        "linkage": manifest["linkage"],
        "scan": manifest["scan"],
        "next": format!("octocode graph query stats --graph {id} --workspace {}", octocode_io::shell::quote(&home.parent()?.parent()?.to_string_lossy())),
    }))
}

/// The linking configs a build of the snapshot's files read (see
/// [`super::super::freshness::config_inputs`]), recorded in its manifest.
pub(super) fn snapshot_inputs(
    paths: &PathPolicy,
    root: &Path,
    tables: &format::GraphTables,
) -> BTreeMap<String, String> {
    let files = tables
        .nodes
        .iter()
        .filter(|node| node.kind == format::NodeKind::File)
        .map(|node| tables.str(node.key));
    super::super::freshness::config_inputs(
        paths,
        root,
        &super::super::freshness::scope_dirs(root, files),
    )
}

/// The source files a re-ingest of `root` would scan, root-relative: the
/// ingest's exclusions and `.gitignore`, graph-fact extensions and the files
/// its language globs name.
pub(super) fn scope_files(
    root: &Path,
    exclude_dir: &[String],
    language_globs: &BTreeMap<String, Vec<String>>,
    extensions: &[String],
) -> BTreeSet<String> {
    let globs = language_globs
        .values()
        .flatten()
        .filter_map(|pattern| {
            octocode_engine::search::glob::set(
                root,
                [pattern.as_str()].into_iter(),
                "languageGlobs",
            )
            .ok()
            .flatten()
        })
        .collect::<Vec<_>>();
    let excluded = INGEST_EXCLUDES
        .iter()
        .map(|name| (*name).to_owned())
        .chain(exclude_dir.iter().cloned());
    let mut current = BTreeSet::new();
    for entry in crate::tools::pruned_walk(root, excluded).build().flatten() {
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let path = entry.path();
        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase());
        if !ext.is_some_and(|ext| extensions.contains(&ext))
            && !globs
                .iter()
                .any(|matcher| octocode_engine::search::glob::matches(matcher, path, false))
        {
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
        let extensions = manifest["registryExtensions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let current = scope_files(root, &excludes, &globs, &extensions);
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
    for (file, status) in super::super::freshness::config_changes(&recorded, &current) {
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
