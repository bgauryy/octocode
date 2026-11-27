//! Whether a snapshot still matches its sources: stamps, digests, inputs and
//! the change list.
use super::*;
use crate::ast_graph::walk_scope::build_admission;
use octocode_engine::stamp::SourceStamp;

#[cfg(test)]
mod policy_tests;
mod scope_proof;
pub(super) use scope_proof::settled_scope_proof;
use scope_proof::{file_dirs, scope_proven};

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
    // A different size cannot have the recorded digest.
    let meta = std::fs::metadata(path).ok()?;
    if stamp.is_some_and(|recorded| recorded.size != meta.len()) {
        return Some(false);
    }
    if stamp.is_some() && SourceStamp::of(&meta).as_ref() == stamp {
        return Some(true);
    }
    // Hashed as it streams: proving a large source holds nothing of it.
    let mut hash = octocode_engine::digest::HashWriter::default();
    std::io::copy(&mut std::fs::File::open(path).ok()?, &mut hash).ok()?;
    Some(hex::encode(hash.finalize()) == digest)
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
    for dir in file_dirs(tables) {
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
/// extensions and the files its language globs name, and what the build's
/// own admission ([`build_admission`]) lets through: a source the path policy
/// withholds is no source to scan.
pub(super) fn scope_files(
    paths: &PathPolicy,
    root: &Path,
    scope: &crate::ast_graph::build_scope::BuildScope,
    selected: Option<&Path>,
    extensions: &[String],
    dir: &mut dyn FnMut(&Path),
) -> BTreeSet<String> {
    #[cfg(test)]
    SCOPE_WALKS.with(|walks| walks.set(walks.get() + 1));
    let globs = scope
        .language_globs
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
    // The caller's exclusions as the build's walk reads them: a matching
    // directory is never entered, and `!x` excludes like `x`. A snapshot's
    // globs already compiled at build time, so `ok()` drops none.
    let excluded = octocode_engine::search::glob::compile(root, &[], &scope.exclude).ok();
    let admission = build_admission(paths, root);
    // The walk is depth first: a withheld directory's entries follow it.
    let mut withheld: Option<PathBuf> = None;
    let mut current = BTreeSet::new();
    let selected = selected.map(Path::to_owned);
    for entry in octocode_tools_shared::pruned_walk_in(
        root,
        scope.walk.default_excludes,
        [],
        (scope.walk.ignore, paths.global_ignore()),
        move |entry| {
            if selected
                .as_deref()
                .is_some_and(|file| entry.depth() != 0 && entry.path() != file)
            {
                return false;
            }
            let is_dir = entry.file_type().is_some_and(|kind| kind.is_dir());
            !excluded
                .as_ref()
                .is_some_and(|globs| globs.excludes(entry.path(), is_dir))
        },
    )
    .build()
    .flatten()
    {
        let path = entry.path();
        if withheld.as_ref().is_some_and(|held| path.starts_with(held)) {
            continue;
        }
        if path != root && !admission.admit(path, entry.file_type(), |_| false) {
            withheld = entry
                .file_type()
                .is_some_and(|kind| kind.is_dir())
                .then(|| path.to_path_buf());
            continue;
        }
        if entry.file_type().is_some_and(|kind| kind.is_dir()) {
            dir(path);
        }
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
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

#[cfg(test)]
thread_local! {
    /// Tests: the source walks this thread's freshness checks and proofs ran.
    pub(super) static SCOPE_WALKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Tests: the freshness checks ([`snapshot_changes`]) this thread ran.
    pub(super) static FRESHNESS_CHECKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
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

/// A snapshot's files under `root`, where the path policy lets the freshness
/// check read them. Its keys are data: one that is not a plain relative path,
/// leaves the allowed roots through a symlink, or names a withheld file is
/// `missing`, whether or not something is there, so the check never tells
/// what lies outside. Each directory is resolved through the policy once,
/// each file costs one `lstat`, and only a symlinked file is resolved.
pub(super) struct Readable<'p> {
    paths: &'p PathPolicy,
    root: &'p Path,
    dirs: std::collections::HashMap<PathBuf, Option<PathBuf>>,
}

impl<'p> Readable<'p> {
    pub(super) fn new(paths: &'p PathPolicy, root: &'p Path) -> Self {
        Self {
            paths,
            root,
            dirs: std::collections::HashMap::new(),
        }
    }

    pub(super) fn path(&mut self, file: &str) -> Option<PathBuf> {
        let relative = Path::new(file);
        if !relative
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
        {
            return None;
        }
        let lexical = self.root.join(relative);
        let (dir, name) = (lexical.parent()?, lexical.file_name()?);
        let paths = self.paths;
        if !self.dirs.contains_key(dir) {
            let resolved = paths.validate(dir).ok().map(|valid| valid.canonical);
            self.dirs.insert(dir.to_path_buf(), resolved);
        }
        let path = self.dirs.get(dir)?.as_ref()?.join(name);
        let entry = std::fs::symlink_metadata(&path).ok()?;
        if entry.file_type().is_symlink() {
            return paths.validate_read(&path).ok().map(|valid| valid.canonical);
        }
        let withheld = paths.withholds(
            &path,
            octocode_runtime_base::policy::discovery::EntryKind::File,
        );
        (entry.is_file() && !withheld).then_some(path)
    }
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
    #[cfg(test)]
    FRESHNESS_CHECKS.with(|checks| checks.set(checks.get() + 1));
    let mut changes = Vec::new();
    let done = |changes: &Vec<Change>| first_only && !changes.is_empty();
    let stamps = tables.stamps_by_node();
    let mut readable = Readable::new(paths, root);
    for (node, digest) in &tables.digests {
        let file = tables.str(tables.nodes[*node as usize].key);
        let matches = readable
            .path(file)
            .and_then(|path| source_matches(&path, tables.str(*digest), stamps.get(node)));
        let status = match matches {
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
    for (file, status) in scope_changes(paths, root, tables, manifest, &changes, &mut readable) {
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
    paths: &PathPolicy,
    root: &Path,
    tables: &format::GraphTables,
    manifest: &Value,
    changes: &[Change],
    readable: &mut Readable<'_>,
) -> Vec<(String, &'static str)> {
    if scope_proven(root, paths.global_ignore(), tables, manifest) {
        return Vec::new();
    }
    let recorded = tables
        .nodes
        .iter()
        .filter(|node| node.kind == format::NodeKind::File)
        .map(|node| tables.str(node.key))
        .collect::<BTreeSet<_>>();
    let extensions = manifest["registryExtensions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let current = scope_files(
        paths,
        root,
        &crate::ast_graph::build_scope::BuildScope::of_manifest(&manifest["scan"]),
        manifest["fileScope"].as_str().map(Path::new),
        &extensions,
        &mut |_| {},
    );
    let reported = changes
        .iter()
        .map(|change| change.file.as_str())
        .collect::<BTreeSet<_>>();
    let excluded = recorded
        .iter()
        .filter(|file| !current.contains(**file) && !reported.contains(**file))
        .map(|file| {
            let status = if readable.path(file).is_some() {
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
