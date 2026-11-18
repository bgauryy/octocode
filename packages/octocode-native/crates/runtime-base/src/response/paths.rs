//! One spelling for local evidence paths before shared response shaping.
//!
//! A row names a path relative to one base. Inside the workspace that base
//! is the workspace itself, and a related call's visible input names its
//! workspace paths as rows do (a relative input path resolves against the
//! same workspace). A row whose evidence all lies outside it (a clone, a
//! materialized tree, another allowed root) names its paths from the root of
//! the tree they lie in, stated once as the row's `root`, while its calls
//! keep their absolute paths, so each runs unchanged from wherever it is
//! sent. A graph names its own rows (from its snapshot root, `graph.root`,
//! outside the workspace). A continuation runs sealed, as written.
use crate::policy::path::PathPolicy;
use octocode_engine::text::lexical;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Row fields whose text names a local file or directory.
const PATH_FIELDS: [&str; 3] = ["path", "workspaceRoot", "dir"];

/// Paths of `value`, the data of one row answering `asked`, as rows name
/// them. Provider URIs and source text stay unchanged. `states_own_root`
/// says the caller's rows name their paths from a root they state
/// themselves (a graph's snapshot root, `graph.root`), so no row root is
/// derived for them.
pub fn shown_paths(states_own_root: bool, value: &mut Value, paths: &PathPolicy, asked: &Value) {
    // Outside the workspace, the row's own `path` that only repeats the
    // request's stays as asked: the response drops it as an echo.
    let outside = asked["path"].as_str().is_some_and(|path| {
        Path::new(path).is_absolute() && paths.workspace_relative(path).is_none()
    });
    let echo = value
        .as_object_mut()
        .filter(|fields| outside && fields.get("path") == asked.get("path"))
        .and_then(|fields| fields.shift_remove("path"));
    let root = (!states_own_root)
        .then(|| outside_root(value, paths))
        .flatten();
    let mut shown = Shown {
        paths,
        root: root.as_deref(),
        stated: false,
    };
    shown.walk(value);
    let Some(fields) = value.as_object_mut() else {
        return;
    };
    if let Some(echo) = echo {
        fields.insert("path".to_owned(), echo);
    }
    if shown.stated
        && let Some(root) = root
    {
        fields.insert(
            "root".to_owned(),
            Value::String(root.to_string_lossy().into_owned()),
        );
    }
}

/// `path` (absolute) as a row of the response names it, for text that quotes
/// it, such as a warning: from the workspace inside it; outside it, from the
/// root of the tree it lies in, which the row states once as its `root`; as
/// given when no root holds it.
pub fn shown_path(paths: &PathPolicy, path: &str) -> String {
    let root = paths
        .workspace_relative(path)
        .is_none()
        .then(|| paths.tree_root(path))
        .flatten();
    Shown {
        paths,
        root: root.as_deref(),
        stated: false,
    }
    .shown(path)
    .unwrap_or_else(|| path.to_owned())
}

struct Shown<'a> {
    paths: &'a PathPolicy,
    /// The tree every evidence path of the row lies in, when that is not
    /// the workspace.
    root: Option<&'a Path>,
    /// Some path was named from `root`.
    stated: bool,
}

impl Shown<'_> {
    fn walk(&mut self, value: &mut Value) {
        match value {
            Value::Object(fields) => {
                if is_call(fields) {
                    // Only a workspace path of a related call changes.
                    if fields.get("kind").and_then(Value::as_str) == Some("related")
                        && let Some(input) = fields.get_mut("input")
                    {
                        Shown {
                            paths: self.paths,
                            root: None,
                            stated: false,
                        }
                        .walk(input);
                    }
                    return;
                }
                for (key, value) in fields.iter_mut() {
                    match value {
                        Value::String(path) if PATH_FIELDS.contains(&key.as_str()) => {
                            if let Some(shown) = self.shown(path) {
                                *path = shown;
                            }
                        }
                        value => self.walk(value),
                    }
                }
            }
            Value::Array(items) => items.iter_mut().for_each(|item| self.walk(item)),
            _ => {}
        }
    }

    /// `path` as the row names it: from `root` when it lies under it, else
    /// from the workspace; `None` keeps it as it is.
    fn shown(&mut self, path: &str) -> Option<String> {
        let Some(root) = self.root else {
            return self.paths.workspace_relative(path);
        };
        let relative = lexical(Path::new(path)).strip_prefix(root).ok()?.to_owned();
        self.stated = true;
        Some(if relative.as_os_str().is_empty() {
            ".".to_owned()
        } else {
            octocode_engine::text::slash_path(&relative)
        })
    }
}

/// A call object: the tool it runs and its input.
fn is_call(fields: &serde_json::Map<String, Value>) -> bool {
    fields.get("tool").is_some_and(Value::is_string)
        && fields.get("input").is_some_and(Value::is_object)
}

/// The root of the tree a row's evidence lies in when none of it lies in
/// the workspace: the clone, materialized tree or allowed root holding its
/// first absolute path. `None` when a path lies in the workspace, or no
/// path lies under a root.
fn outside_root(value: &Value, paths: &PathPolicy) -> Option<PathBuf> {
    let mut evidence = Vec::new();
    collect_paths(value, &mut evidence);
    if evidence
        .iter()
        .any(|path| paths.workspace_relative(path).is_some())
    {
        return None;
    }
    evidence
        .iter()
        .filter(|path| Path::new(path).is_absolute())
        .find_map(|path| paths.tree_root(path))
}

fn collect_paths<'a>(value: &'a Value, out: &mut Vec<&'a str>) {
    match value {
        Value::Object(fields) if is_call(fields) => {}
        Value::Object(fields) => {
            for (key, value) in fields {
                match value {
                    Value::String(path) if PATH_FIELDS.contains(&key.as_str()) => out.push(path),
                    value => collect_paths(value, out),
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|item| collect_paths(item, out)),
        _ => {}
    }
}

#[cfg(test)]
mod tests;
