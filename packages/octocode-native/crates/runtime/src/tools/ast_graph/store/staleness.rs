//! Whether a snapshot still matches its sources: stamps, digests, inputs and
//! the change list.
use super::*;
use octocode_engine::stamp::SourceStamp;

/// Whether the file at `path` still holds the bytes `digest` names, `None`
/// when it cannot be read. A stamp recorded at ingest that still equals the
/// file's size and change times proves it without a read (the stamp was
/// taken only for a settled file, see [`octocode_engine::stamp::SourceStamp`]);
/// any difference, or no stamp, falls back to hashing the content.
pub(crate) fn source_matches(
    path: &Path,
    digest: &str,
    stamp: Option<&SourceStamp>,
) -> Option<bool> {
    // Ingest never reads a source beyond this bound. A different size cannot
    // have the recorded digest, and a file grown past the parser bound must
    // prove the snapshot stale without allocating it in full.
    const MAX_INGEST_SOURCE_BYTES: usize =
        octocode_engine::fact_scan::MAX_GRAPH_SOURCE_BYTES as usize;
    let meta = std::fs::metadata(path).ok()?;
    let current_size = meta.len();
    if current_size > MAX_INGEST_SOURCE_BYTES as u64
        || stamp.is_some_and(|recorded| recorded.size != current_size)
    {
        return Some(false);
    }
    if stamp.is_some() && SourceStamp::of(&meta).as_ref() == stamp {
        return Some(true);
    }
    let content = crate::tools::source::read_bounded(path, MAX_INGEST_SOURCE_BYTES).ok()?;
    Some(octocode_engine::digest::sha256(&content) == digest)
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

/// A digest of the stamps behind a snapshot's config inputs: every directory
/// a build lists for configs (`scope_dirs`, keyed as it keys them) and every
/// config in `keys`. A later equal digest proves the same names and bytes
/// without listing or hashing: a listing's added or removed entry moves its
/// directory's stamp, an edit moves the config's, on the terms of
/// [`source_matches`]. `None` when a stamp is refused or the path policy
/// refuses a config.
fn input_stamps<'a>(
    paths: &PathPolicy,
    root: &Path,
    tables: &format::GraphTables,
    keys: impl Iterator<Item = &'a str>,
    stamp_of: impl Fn(&std::fs::Metadata) -> Option<SourceStamp>,
) -> Option<String> {
    let mut proof = octocode_engine::digest::HashWriter::default();
    let mut stamped = |path: &Path, name: &[u8]| {
        let stamp = stamp_of(&std::fs::metadata(path).ok()?)?;
        proof.update(name);
        proof.update([0]);
        for part in [stamp.size, stamp.modified_ns, stamp.changed_ns, stamp.inode] {
            proof.update(part.to_le_bytes());
        }
        Some(())
    };
    // `scope_dirs` of no files is the root and its ancestors inside the
    // repository; the directories of the files, as `&str`, cost far less to
    // collect than its paths, on every query.
    for dir in super::super::freshness::scope_dirs(root, []) {
        stamped(&dir, dir.as_os_str().as_encoded_bytes())?;
    }
    let mut dirs = BTreeSet::new();
    for node in &tables.nodes {
        if node.kind != format::NodeKind::File {
            continue;
        }
        let mut rest = tables.str(node.key);
        while let Some((dir, _)) = rest.rsplit_once('/') {
            if !dirs.insert(dir) {
                break;
            }
            rest = dir;
        }
    }
    for dir in dirs {
        stamped(&root.join(dir), dir.as_bytes())?;
    }
    for key in keys {
        let (mut dir, mut name) = (root, key);
        while let Some(rest) = name.strip_prefix("../") {
            dir = dir.parent()?;
            name = rest;
        }
        let valid = paths.validate_read(dir.join(name)).ok()?;
        stamped(&valid.canonical, key.as_bytes())?;
    }
    Some(hex::encode(proof.finalize()))
}

/// Settled stamps of the inputs a build read, taken before it hashes them
/// ([`snapshot_inputs`]), so a later write moves a stamp past what was hashed.
/// Unix only: elsewhere a directory's stamp is its modification time alone,
/// which not every filesystem moves when an entry is added.
pub(super) fn settled_input_stamps(
    paths: &PathPolicy,
    root: &Path,
    tables: &format::GraphTables,
    keys: &BTreeMap<String, String>,
) -> Option<String> {
    if !cfg!(unix) {
        return None;
    }
    let now = octocode_engine::stamp::settle_clock();
    input_stamps(
        paths,
        root,
        tables,
        keys.keys().map(String::as_str),
        |meta| SourceStamp::settled(meta, now),
    )
}

/// Whether the scope's directories list configs other than `inputs`: one the
/// path policy withheld is no input now and is one once admitted, with no
/// stamp recorded to show it, so such a snapshot takes no proof.
pub(super) fn withholds_configs(
    root: &Path,
    tables: &format::GraphTables,
    inputs: &BTreeMap<String, String>,
) -> bool {
    let files = tables
        .nodes
        .iter()
        .filter(|node| node.kind == format::NodeKind::File)
        .map(|node| tables.str(node.key));
    let listed = super::super::freshness::scope_dirs(root, files)
        .iter()
        .filter_map(|dir| std::fs::read_dir(dir).ok())
        .flatten()
        .flatten()
        .filter(|entry| {
            super::super::freshness::is_global_config(&entry.file_name().to_string_lossy())
                && entry.file_type().is_ok_and(|kind| kind.is_file())
        })
        .count();
    listed != inputs.len()
}

/// Whether the snapshot's recorded input proof still holds, so its configs
/// need neither a listing nor a hash.
fn inputs_proven(
    paths: &PathPolicy,
    root: &Path,
    tables: &format::GraphTables,
    manifest: &Value,
) -> bool {
    let Some(recorded) = manifest["inputProof"].as_str() else {
        return false;
    };
    let keys = manifest["inputs"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(key, _)| key.as_str());
    input_stamps(paths, root, tables, keys, SourceStamp::of).as_deref() == Some(recorded)
}

/// The source files a re-ingest of `root` would scan, root-relative: the one
/// default prune, the caller's exclusions and `.gitignore`, graph-fact
/// extensions and the files its language globs name.
pub(super) fn scope_files(
    root: &Path,
    exclude: &[String],
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
    let skipped =
        octocode_engine::search::glob::set(root, exclude.iter().map(String::as_str), "exclude")
            .ok()
            .flatten();
    let mut current = BTreeSet::new();
    for entry in crate::tools::pruned_walk(root, []).build().flatten() {
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let path = entry.path();
        if skipped
            .as_ref()
            .is_some_and(|matcher| octocode_engine::search::glob::matches(matcher, path, false))
        {
            continue;
        }
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
}

/// Every way the tree under `root` differs from the snapshot `tables`
/// (`manifest` its manifest): a source edited or removed (digests, a settled
/// stamp skipping the hash), a source added (a rescan with the ingest's
/// walk), and a linking config edited, removed or added. The one
/// freshness check behind query freshness and `query stale`; `first_only`
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
            return Freshness { changes };
        }
    }
    for (file, status) in scope_changes(root, tables, manifest, &changes) {
        changes.push(Change {
            file,
            status,
            config: false,
        });
        if done(&changes) {
            return Freshness { changes };
        }
    }
    if inputs_proven(paths, root, tables, manifest) {
        return Freshness { changes };
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
    Freshness { changes }
}

/// Sources a re-ingest would scan differently: files added under the
/// snapshot's scope (a rescan with the ingest's exclusions), and recorded
/// files it would no longer scan (gone, or now ignored or excluded) that
/// `changes` does not already report.
fn scope_changes(
    root: &Path,
    tables: &format::GraphTables,
    manifest: &Value,
    changes: &[Change],
) -> Vec<(String, &'static str)> {
    let recorded = tables
        .nodes
        .iter()
        .filter(|node| node.kind == format::NodeKind::File)
        .map(|node| tables.str(node.key))
        .collect::<BTreeSet<_>>();
    let strings = |value: &Value| {
        value
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let globs = serde_json::from_value::<BTreeMap<String, Vec<String>>>(
        manifest["scan"]["languageGlobs"].clone(),
    )
    .unwrap_or_default();
    let current = scope_files(
        root,
        &strings(&manifest["scan"]["exclude"]),
        &globs,
        &strings(&manifest["registryExtensions"]),
    );
    let reported = changes
        .iter()
        .map(|change| change.file.as_str())
        .collect::<BTreeSet<_>>();
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
    added.chain(excluded).collect()
}
