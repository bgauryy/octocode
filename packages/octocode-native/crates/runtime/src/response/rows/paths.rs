use serde_json::{Map, Value, json};
use std::path::Path;

/// Spell every absolute local `path` of a continuation's rows (`{tool,
/// query}` under `next`, `hints`, or a row) relative to the workspace root,
/// which relative paths resolve against: the same file, without repeating
/// the root the response already names as `root`. Paths outside the
/// workspace, `file://` uris and non-local fields stay as they are.
pub(super) fn relativize_continuation_paths(value: &mut Value, workspace: &str) {
    crate::response::result::for_each_call_mut(value, &mut |call| {
        for row in call
            .pointer_mut("/query/queries")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            if let Some(Value::String(path)) = row.get_mut("path")
                && let Some(relative) = workspace_relative(path, workspace)
            {
                *path = relative;
            }
        }
    });
}

/// Whether a continuation (`{tool, query}` at any depth) names a relative
/// local `path`: it resolves against the workspace root, which the response
/// then names as `root` even when no row carries a path (an empty row).
pub(super) fn relative_continuation_path(value: &Value) -> bool {
    let mut relative = false;
    crate::response::result::for_each_call(value, &mut |call| {
        relative |= call
            .pointer("/query/queries")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .any(|row| {
                row.get("path")
                    .and_then(Value::as_str)
                    .is_some_and(|path| !Path::new(path).is_absolute() && !path.contains("://"))
            });
    });
    relative
}

/// `path` relative to `workspace` (`.` for the root itself) when it is an
/// absolute path under it.
fn workspace_relative(path: &str, workspace: &str) -> Option<String> {
    if path == workspace {
        return Some(".".to_owned());
    }
    path.strip_prefix(workspace)?
        .strip_prefix('/')
        .filter(|rest| !rest.is_empty())
        .map(str::to_owned)
}

/// Inverse of [`rewrite_paths`] for one directory prefix: same traversal and
/// exclusions, applied to the relative `path` fields it produced.
pub(super) fn prefix_relative_paths(value: &mut Value, depth: usize, prefix: &str) {
    if depth > 8 {
        return;
    }
    match value {
        Value::Object(map) => {
            if let Some(Value::String(path)) = map.get_mut("path")
                && !Path::new(path.as_str()).is_absolute()
            {
                *path = format!("{prefix}/{path}");
            }
            for (key, child) in map {
                if !matches!(key.as_str(), "next" | "location") {
                    prefix_relative_paths(child, depth + 1, prefix);
                }
            }
        }
        Value::Array(array) => {
            for child in array {
                prefix_relative_paths(child, depth + 1, prefix);
            }
        }
        _ => {}
    }
}

fn absolute_path(map: &Map<String, Value>) -> Option<String> {
    if let Some(path) = map
        .get("absolutePath")
        .and_then(Value::as_str)
        .filter(|p| Path::new(p).is_absolute())
    {
        return Some(path.into());
    }
    if let Some(uri) = map
        .get("uri")
        .and_then(Value::as_str)
        .filter(|p| p.starts_with("file://"))
    {
        return url::Url::parse(uri)
            .ok()?
            .to_file_path()
            .ok()
            .map(|p| p.to_string_lossy().into_owned());
    }
    map.get("path")
        .and_then(Value::as_str)
        .filter(|p| Path::new(p).is_absolute())
        .map(str::to_owned)
}

pub(super) fn visit_paths(value: &Value, depth: usize, paths: &mut Vec<String>) {
    if depth > 8 {
        return;
    }
    match value {
        Value::Object(map) => {
            if let Some(path) = absolute_path(map) {
                paths.push(path);
            }
            for (key, child) in map {
                if !matches!(key.as_str(), "next" | "location") {
                    visit_paths(child, depth + 1, paths);
                }
            }
        }
        Value::Array(array) => {
            for child in array {
                visit_paths(child, depth + 1, paths);
            }
        }
        _ => {}
    }
}

pub(super) fn common_directory(paths: &[String]) -> Option<String> {
    let first = paths.first()?;
    let mut length = first.len();
    for path in paths.iter().skip(1) {
        length = first
            .bytes()
            .zip(path.bytes())
            .take(length)
            .take_while(|(a, b)| a == b)
            .count();
    }
    // The common prefix may end inside a UTF-8 character; search bytes for '/'.
    let slash = first.as_bytes()[..length]
        .iter()
        .rposition(|b| *b == b'/')?;
    (slash > 1).then(|| first[..slash].into())
}

/// Name every file location by `path`: relative to `base` under it, else
/// absolute (a `file://` uri outside `base` becomes its absolute path).
pub(super) fn rewrite_paths(value: &mut Value, depth: usize, base: Option<&str>) {
    if depth > 8 {
        return;
    }
    match value {
        Value::Object(map) => {
            let file_uri = map
                .get("uri")
                .and_then(Value::as_str)
                .is_some_and(|uri| uri.starts_with("file://"));
            if let Some(path) = absolute_path(map).and_then(|p| {
                let relative = base.and_then(|base| {
                    if p == base {
                        Some(".".to_owned())
                    } else {
                        p.strip_prefix(&format!("{base}/")).map(str::to_owned)
                    }
                });
                relative.or_else(|| file_uri.then_some(p))
            }) {
                map.shift_remove("absolutePath");
                map.shift_remove("uri");
                map.insert("path".into(), json!(path));
            } else if let Some(Value::String(path)) = map.get_mut("path")
                && let Some(clean) = without_dot_segments(path)
            {
                *path = clean;
            }
            for (key, child) in map {
                if !matches!(key.as_str(), "next" | "location") {
                    rewrite_paths(child, depth + 1, base);
                }
            }
        }
        Value::Array(array) => {
            for child in array {
                rewrite_paths(child, depth + 1, base);
            }
        }
        _ => {}
    }
}

/// A relative `path` without its `.` segments (`./src/a.ts` → `src/a.ts`),
/// so a row names a file the way the tools spell it whatever the caller
/// typed; `None` when there is nothing to drop. `..` stays: collapsing it is
/// not lexical across symlinks.
fn without_dot_segments(path: &str) -> Option<String> {
    if Path::new(path).is_absolute() || !path.split('/').any(|segment| segment == ".") {
        return None;
    }
    let kept = path
        .split('/')
        .filter(|segment| *segment != ".")
        .collect::<Vec<_>>()
        .join("/");
    Some(if kept.is_empty() {
        ".".to_owned()
    } else {
        kept
    })
}
