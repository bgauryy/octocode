//! A proof that a re-ingest's walk would list what a snapshot holds, without
//! the walk: the stamps of every directory it entered and every ignore input
//! that shaped it. An added, removed or renamed entry moves its directory's
//! stamp, and an edit moves an ignore file's, on the terms of
//! [`source_matches`](super::source_matches); a directory or ignore file no
//! listing can miss was stamped before the walk that checked it, so a later
//! write moves a stamp past what was listed.
use super::super::format;
use crate::tools::page_memo::{directory_ignore_inputs_with, repository_markers};
use octocode_engine::search::walk::GlobalIgnore;
use octocode_engine::stamp::{SourceStamp, exists};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// A digest of `dirs` (root-relative), the ignore inputs `rules` names, those
/// outside any walk of `root`, and the repository markers above it. `None` when
/// `stamp_of` refuses a stamp.
fn stamps<'a>(
    root: &Path,
    global: GlobalIgnore,
    dirs: impl Iterator<Item = &'a str>,
    rules: impl Iterator<Item = &'a str>,
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
    for dir in dirs {
        stamped(&root.join(dir), dir.as_bytes())?;
    }
    let outside = octocode_engine::search::walk::external_ignore_inputs_with(root, global);
    for rule in rules {
        let path = root.join(rule);
        if exists(&path) {
            stamped(&path, rule.as_bytes())?;
        }
    }
    for path in &outside {
        if exists(path) {
            stamped(path, path.as_os_str().as_encoded_bytes())?;
        }
    }
    for marker in repository_markers(root) {
        if exists(&marker) {
            proof.update(marker.as_os_str().as_encoded_bytes());
            proof.update([0]);
        }
    }
    Some(hex::encode(proof.finalize()))
}

/// The directories holding the snapshot's files, root-relative, each once.
pub(super) fn file_dirs(tables: &format::GraphTables) -> BTreeSet<&str> {
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
    dirs
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root).map_or_else(
        |_| path.to_string_lossy().into_owned(),
        octocode_engine::text::slash_path,
    )
}

/// What a snapshot's manifest records to prove its listing later: taken with a
/// walk before the stamps and one after, both listing exactly the snapshot's
/// files. `None` when an input is unsettled, the listing moved or differs, or an
/// ignore file is one no stamp can watch: each query then walks. Unix only, as
/// [`settled_input_stamps`](super::settled_input_stamps).
pub(in super::super) fn settled_scope_proof(
    root: &Path,
    global: GlobalIgnore,
    tables: &format::GraphTables,
    walk: impl Fn(&mut dyn FnMut(&Path)) -> BTreeSet<String>,
) -> Option<Value> {
    if !cfg!(unix) {
        return None;
    }
    let listed = || {
        let mut dirs = BTreeSet::<PathBuf>::new();
        let files = walk(&mut |dir| {
            dirs.insert(dir.to_path_buf());
        });
        (files, dirs)
    };
    let (files, dirs) = listed();
    let recorded = tables
        .nodes
        .iter()
        .filter(|node| node.kind == format::NodeKind::File)
        .map(|node| tables.str(node.key).to_owned())
        .collect::<BTreeSet<_>>();
    if files != recorded {
        return None;
    }
    let mut rules = BTreeSet::new();
    for dir in &dirs {
        let inputs = directory_ignore_inputs_with(dir, root, global, |_| true);
        if inputs.untracked {
            return None;
        }
        rules.extend(
            inputs
                .files
                .iter()
                .chain(&inputs.external)
                .map(|path| relative(root, path)),
        );
    }
    let names = dirs
        .iter()
        .map(|dir| relative(root, dir))
        .collect::<BTreeSet<_>>();
    let now = octocode_engine::stamp::settle_clock();
    let digest = stamps(
        root,
        global,
        names.iter().map(String::as_str),
        rules.iter().map(String::as_str),
        |meta| SourceStamp::settled(meta, now),
    )?;
    if listed() != (files, dirs) {
        return None;
    }
    let held = file_dirs(tables);
    let empty = names
        .iter()
        .filter(|dir| !dir.is_empty() && !held.contains(dir.as_str()))
        .collect::<Vec<_>>();
    Some(json!({"stamps": digest, "dirs": empty, "rules": rules}))
}

/// Whether the snapshot's recorded listing proof still holds.
pub(super) fn scope_proven(
    root: &Path,
    global: GlobalIgnore,
    tables: &format::GraphTables,
    manifest: &Value,
) -> bool {
    let proof = &manifest["scopeProof"];
    let Some(recorded) = proof["stamps"].as_str() else {
        return false;
    };
    let strings = |field: &'static str| {
        proof[field]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
    };
    let mut dirs = file_dirs(tables);
    dirs.insert("");
    dirs.extend(strings("dirs"));
    stamps(
        root,
        global,
        dirs.into_iter(),
        strings("rules"),
        SourceStamp::of,
    )
    .as_deref()
        == Some(recorded)
}
