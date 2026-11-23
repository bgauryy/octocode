//! The commit a local row's evidence is checked out at.
//!
//! A row names files; a citation names them at a commit. The commit is the
//! git `HEAD` of the one work tree holding every path the row names, found
//! by walking up from each path no higher than the allowed root holding it,
//! so no read leaves the allowed roots for a work tree above them (a folder
//! ignored by an outer repository never cites that repository). `HEAD` and
//! its ref are read from the repository's own files, never by running git;
//! only an object id leaves them. A ref those files do not resolve (an
//! unborn branch, a reftable store) states nothing.
use crate::policy::path::PathPolicy;
use octocode_engine::text::lexical;
use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, Read};
use std::path::{Component, Path, PathBuf};

/// Row fields naming the evidence a row reports. `skippedPaths` names what
/// a walk passed over, not evidence, and a call's input is a request.
const EVIDENCE_FIELDS: [&str; 2] = ["path", "dir"];

/// The most a git metadata line is read to: a one-line file (`HEAD`, a
/// loose ref, `commondir`, a `.git` file) or one `packed-refs` line. A path
/// line (PATH_MAX, 4096) fits many times over; a longer line is no ref.
const LINE_LIMIT: u64 = 64 * 1024;

/// The commit to state on `value`, the data of one local row answering
/// `asked`: the `HEAD` of the one work tree holding every path it names
/// (the asked path when it names none). `None` for an error row, paths in
/// several work trees or in none.
pub fn checkout_commit(value: &Value, paths: &PathPolicy, asked: &Value) -> Option<String> {
    if value.get("errorCode").is_some() {
        return None;
    }
    let mut named = Vec::new();
    evidence_paths(value, &mut named);
    if named.is_empty() {
        named.extend(asked.get("path").and_then(Value::as_str));
    }
    let roots = paths.allowed_roots();
    let mut trees = WorkTrees {
        roots: &roots,
        memo: HashMap::new(),
    };
    let mut held = None;
    for path in named {
        let path = lexical(&paths.expand_and_resolve(Path::new(path)));
        let tree = trees.holding(&path)?;
        match &held {
            Some(other) if *other != tree => return None,
            Some(_) => {}
            None => held = Some(tree),
        }
    }
    head_commit(&held?)
}

fn evidence_paths<'a>(value: &'a Value, out: &mut Vec<&'a str>) {
    match value {
        Value::Object(fields) if super::paths::is_call(fields) => {}
        Value::Object(fields) => {
            for (key, value) in fields {
                match value {
                    Value::String(path) if EVIDENCE_FIELDS.contains(&key.as_str()) => {
                        out.push(path);
                    }
                    _ if key == "skippedPaths" => {}
                    value => evidence_paths(value, out),
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|item| evidence_paths(item, out)),
        _ => {}
    }
}

/// Work trees found for one row, memoized by directory: a listing of many
/// files in one tree checks each directory once.
struct WorkTrees<'a> {
    roots: &'a [PathBuf],
    memo: HashMap<PathBuf, Option<PathBuf>>,
}

impl WorkTrees<'_> {
    /// The top of the work tree holding `path`: its nearest ancestor (or
    /// itself) holding `.git`, at or below the outermost allowed root that
    /// holds `path`.
    fn holding(&mut self, path: &Path) -> Option<PathBuf> {
        let bound = self
            .roots
            .iter()
            .filter(|root| path.starts_with(root))
            .min_by_key(|root| root.components().count())?;
        let mut visited = Vec::new();
        let mut found = None;
        for dir in path.ancestors().take_while(|dir| dir.starts_with(bound)) {
            if let Some(known) = self.memo.get(dir) {
                found = known.clone();
                break;
            }
            visited.push(dir.to_owned());
            if dir.join(".git").exists() {
                found = Some(dir.to_owned());
                break;
            }
        }
        for dir in visited {
            self.memo.insert(dir, found.clone());
        }
        found
    }
}

/// The object id `HEAD` names in the work tree at `tree`.
fn head_commit(tree: &Path) -> Option<String> {
    let git_dir = git_dir(tree)?;
    let head = small_file(&git_dir.join("HEAD"))?;
    let head = head.trim();
    match head.strip_prefix("ref:") {
        Some(reference) => resolve_ref(&git_dir, reference.trim()),
        None => object_id(head),
    }
}

/// `.git` itself, or the directory a `.git` file names (`gitdir: …`, as a
/// linked work tree or a submodule has).
fn git_dir(tree: &Path) -> Option<PathBuf> {
    let dot = tree.join(".git");
    if dot.is_dir() {
        return Some(dot);
    }
    let text = small_file(&dot)?;
    let target = text.lines().next()?.strip_prefix("gitdir:")?.trim();
    Some(lexical(&tree.join(target)))
}

/// A branch ref's object id: its loose file (in the work tree's own git
/// directory, then the common one a linked work tree shares), else its
/// `packed-refs` line. Only a name under `refs/` that climbs nowhere is read.
fn resolve_ref(git_dir: &Path, name: &str) -> Option<String> {
    let relative = Path::new(name);
    let plain = name.starts_with("refs/")
        && relative
            .components()
            .all(|part| matches!(part, Component::Normal(_)));
    if !plain {
        return None;
    }
    let common = small_file(&git_dir.join("commondir"))
        .map(|dir| lexical(&git_dir.join(dir.trim())))
        .unwrap_or_else(|| git_dir.to_owned());
    for dir in [git_dir, common.as_path()] {
        if let Some(text) = small_file(&dir.join(relative)) {
            return object_id(text.trim());
        }
    }
    packed_ref(&common, name)
}

/// `name`'s object id in `packed-refs`, read one bounded line at a time.
fn packed_ref(common: &Path, name: &str) -> Option<String> {
    let file = std::fs::File::open(common.join("packed-refs")).ok()?;
    let mut reader = std::io::BufReader::new(file);
    let mut line = Vec::new();
    loop {
        line.clear();
        let read = (&mut reader)
            .take(LINE_LIMIT)
            .read_until(b'\n', &mut line)
            .ok()?;
        if read == 0 || (read as u64 == LINE_LIMIT && line.last() != Some(&b'\n')) {
            return None;
        }
        let text = std::str::from_utf8(&line).ok()?.trim_end();
        if text.starts_with(['#', '^']) {
            continue;
        }
        if let Some((id, reference)) = text.split_once(' ')
            && reference == name
        {
            return object_id(id);
        }
    }
}

/// A one-line git file's text, when it fits [`LINE_LIMIT`].
fn small_file(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let bytes = octocode_io::read::read_limited(file, LINE_LIMIT).ok()??;
    String::from_utf8(bytes).ok()
}

/// A SHA-1 or SHA-256 object id, lower case.
fn object_id(text: &str) -> Option<String> {
    (matches!(text.len(), 40 | 64) && text.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| text.to_ascii_lowercase())
}

#[cfg(test)]
mod tests;
