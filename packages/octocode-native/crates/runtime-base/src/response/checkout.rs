//! The checkout a local row's evidence lies in: its commit, branch and remote.
//!
//! A row names files; a citation names them at a commit of a repository. The
//! checkout is the one work tree holding every path the row names, found by
//! walking up from each path no higher than the allowed root holding it, so
//! no read leaves the allowed roots for a work tree above them (a folder
//! ignored by an outer repository never cites that repository). An error row
//! names no evidence, so the path it was asked decides: a refused `.git`
//! path states the checkout the caller was reading it for. `HEAD`, its ref
//! and the `origin` remote are read from the repository's own files, never
//! by running git; only an object id, a ref name and a remote's host and
//! path leave them. A ref those files do not resolve (an unborn branch, a
//! reftable store) states nothing.
use crate::policy::path::PathPolicy;
use octocode_engine::text::lexical;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::io::{BufRead, Read};
use std::path::{Component, Path, PathBuf};

/// Row fields naming the evidence a row reports. `skippedPaths` names what
/// a walk passed over, not evidence, and a call's input is a request.
const EVIDENCE_FIELDS: [&str; 2] = ["path", "dir"];

/// The most a git metadata line is read to: a one-line file (`HEAD`, a
/// loose ref, `commondir`, a `.git` file) or one line of `packed-refs` or
/// `config`. A path line (PATH_MAX, 4096) fits many times over; a longer
/// line is no ref and no remote.
const LINE_LIMIT: u64 = 64 * 1024;

/// The checked-out state of one work tree, as a row states it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Checkout {
    /// The object id `HEAD` names.
    pub commit_sha: String,
    /// The branch `HEAD` is on, without `refs/heads/`; `None` when `HEAD`
    /// is detached.
    pub reference: Option<String>,
    /// The `origin` remote as `<host>/<path>`; `None` when the repository
    /// has none or it names no host (a local path).
    pub remote: Option<String>,
}

impl Checkout {
    /// States this checkout on a row's `fields`. A commit the row states
    /// itself stays, and then names the row's state: the branch and remote
    /// are stated only beside the commit they were read with.
    pub fn state(self, fields: &mut Map<String, Value>) {
        let commit = Value::String(self.commit_sha);
        if *fields.entry("commitSha").or_insert_with(|| commit.clone()) != commit {
            return;
        }
        for (field, value) in [("ref", self.reference), ("remote", self.remote)] {
            if let Some(value) = value {
                fields.entry(field).or_insert(Value::String(value));
            }
        }
    }
}

/// The checkout to state on `value`, the data of one local row answering
/// `asked`: the one work tree holding every path it names (the asked path
/// when it names none, as an error row does). `None` for paths in several
/// work trees or in none.
pub fn checkout(value: &Value, paths: &PathPolicy, asked: &Value) -> Option<Checkout> {
    let mut named = Vec::new();
    if value.get("errorCode").is_none() {
        evidence_paths(value, &mut named);
    }
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
    head(&held?)
}

/// The commit of [`checkout`] alone.
pub fn checkout_commit(value: &Value, paths: &PathPolicy, asked: &Value) -> Option<String> {
    checkout(value, paths, asked).map(|checkout| checkout.commit_sha)
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

/// What `HEAD` names in the work tree at `tree`, with its `origin` remote.
fn head(tree: &Path) -> Option<Checkout> {
    let git_dir = git_dir(tree)?;
    let head = small_file(&git_dir.join("HEAD"))?;
    let head = head.trim();
    let common = small_file(&git_dir.join("commondir"))
        .map(|dir| lexical(&git_dir.join(dir.trim())))
        .unwrap_or_else(|| git_dir.clone());
    let (commit_sha, reference) = match head.strip_prefix("ref:") {
        Some(reference) => {
            let name = reference.trim();
            let branch = name.strip_prefix("refs/heads/").unwrap_or(name);
            (
                resolve_ref(&git_dir, &common, name)?,
                Some(branch.to_owned()),
            )
        }
        None => (object_id(head)?, None),
    };
    Some(Checkout {
        commit_sha,
        reference,
        remote: origin(&common),
    })
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
/// directory, then the `common` one a linked work tree shares), else its
/// `packed-refs` line. Only a name under `refs/` that climbs nowhere is read.
fn resolve_ref(git_dir: &Path, common: &Path, name: &str) -> Option<String> {
    let relative = Path::new(name);
    let plain = name.starts_with("refs/")
        && relative
            .components()
            .all(|part| matches!(part, Component::Normal(_)));
    if !plain {
        return None;
    }
    for dir in [git_dir, common] {
        if let Some(text) = small_file(&dir.join(relative)) {
            return object_id(text.trim());
        }
    }
    packed_ref(common, name)
}

/// `name`'s object id in `packed-refs`.
fn packed_ref(common: &Path, name: &str) -> Option<String> {
    bounded_lines(&common.join("packed-refs"), |text| {
        if text.starts_with(['#', '^']) {
            return None;
        }
        let (id, reference) = text.split_once(' ')?;
        (reference == name).then(|| object_id(id))
    })?
}

/// The first answer `found` gives a line of `file`, read one bounded line
/// at a time; `None` when no line answers or one is longer than
/// [`LINE_LIMIT`] or not UTF-8.
fn bounded_lines<T>(file: &Path, mut found: impl FnMut(&str) -> Option<T>) -> Option<T> {
    let file = std::fs::File::open(file).ok()?;
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
        if let Some(answer) = found(std::str::from_utf8(&line).ok()?.trim_end()) {
            return Some(answer);
        }
    }
}

/// The `origin` remote of the repository whose shared git directory is
/// `common`, as [`remote_name`] names its first `url`.
fn origin(common: &Path) -> Option<String> {
    let mut in_origin = false;
    bounded_lines(&common.join("config"), |line| {
        let line = line.trim_start();
        if let Some(section) = line.strip_prefix('[') {
            in_origin = is_origin_section(section.split(']').next().unwrap_or_default());
            return None;
        }
        let (key, value) = line.split_once('=')?;
        (in_origin && key.trim().eq_ignore_ascii_case("url")).then(|| remote_name(value))
    })?
}

/// Whether a config section header's text names the `origin` remote:
/// `remote "origin"`, or the older `remote.origin`.
fn is_origin_section(header: &str) -> bool {
    let header = header.trim();
    header.eq_ignore_ascii_case("remote.origin")
        || header
            .split_once(char::is_whitespace)
            .is_some_and(|(section, name)| {
                section.eq_ignore_ascii_case("remote") && name.trim() == "\"origin\""
            })
}

/// A remote `url` config value as `<host>/<path>`: the host and repository
/// path alone. The scheme, any user information (`user:token@`), a query, a
/// fragment and a trailing `.git` are dropped here, before the value reaches
/// any output, so a credential kept in the URL is never shown. `None` for a
/// remote on this machine (a path, `file://`), which names no repository a
/// reader can reach.
fn remote_name(value: &str) -> Option<String> {
    // An unquoted config value ends at a comment.
    let value = value.trim();
    let url = match value.strip_prefix('"') {
        Some(quoted) => quoted.split('"').next()?,
        None => value.split(['#', ';']).next()?.trim_end(),
    };
    let (authority, path) = match url.split_once("://") {
        Some((scheme, _)) if scheme.eq_ignore_ascii_case("file") => return None,
        Some((_, rest)) => rest.split_once('/')?,
        // `[user@]host:path`; a path on this machine has no host before
        // its first separator, and one letter is a drive.
        None => url
            .split_once(':')
            .filter(|(host, _)| host.len() > 1 && !host.contains(['/', '\\']))?,
    };
    let host = authority.rsplit('@').next()?.to_ascii_lowercase();
    let path = path.split(['?', '#']).next()?.trim_matches('/');
    let path = path
        .strip_suffix(".git")
        .unwrap_or(path)
        .trim_end_matches('/');
    let named = |text: &str| {
        !text.is_empty() && !text.contains(|c: char| c.is_whitespace() || c.is_control())
    };
    (named(&host) && named(path) && !path.contains('@')).then(|| format!("{host}/{path}"))
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
