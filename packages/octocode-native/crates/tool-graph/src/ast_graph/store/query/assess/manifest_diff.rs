//! Which keys of a changed manifest differ from a git revision, and whether
//! any of them is one the graph's linking reads.
//!
//! A manifest change seeds every file below the manifest's directory, since
//! module resolution, workspace discovery and dependency declarations read
//! it. An edit confined to keys no linking step reads (a `scripts` entry, a
//! version bump, a description) changes no edge, so it seeds nothing.

use std::collections::BTreeSet;

use serde_json::{Map, Value};

use super::changed::{Change, git};
use super::*;

/// The top-level keys of each manifest form, or `table.key` below the
/// tables listed in [`NESTED_TABLES`], that no linking step reads.
const UNREAD_KEYS: &[(&str, &[&str])] = &[
    (
        "package.json",
        &[
            "scripts",
            "version",
            "description",
            "keywords",
            "author",
            "contributors",
            "maintainers",
            "license",
            "repository",
            "bugs",
            "homepage",
            "funding",
            "private",
            "publishConfig",
            "engines",
            "packageManager",
            "files",
            "volta",
            "prettier",
            "eslintConfig",
            "lint-staged",
            "husky",
            "os",
            "cpu",
        ],
    ),
    (
        "Cargo.toml",
        &[
            "package.version",
            "package.description",
            "package.authors",
            "package.license",
            "package.license-file",
            "package.repository",
            "package.homepage",
            "package.documentation",
            "package.readme",
            "package.keywords",
            "package.categories",
            "package.publish",
            "package.rust-version",
            "package.exclude",
            "package.include",
            "package.metadata",
            "profile",
            "lints",
            "badges",
            "workspace.metadata",
            "workspace.lints",
        ],
    ),
    (
        "pyproject.toml",
        &[
            "project.version",
            "project.description",
            "project.readme",
            "project.license",
            "project.authors",
            "project.maintainers",
            "project.keywords",
            "project.classifiers",
            "project.urls",
            "tool.black",
            "tool.ruff",
            "tool.isort",
            "tool.mypy",
            "tool.coverage",
        ],
    ),
];

/// TOML tables whose keys are compared one by one.
const NESTED_TABLES: &[&str] = &["package", "project", "workspace", "tool"];

/// A changed manifest's differing keys, in key order.
#[derive(Debug)]
pub(super) struct ManifestDiff {
    pub keys: Vec<String>,
    /// Some differing key is one a linking step reads.
    pub read: bool,
}

/// The largest working-tree manifest compared key by key; a larger one
/// counts as read, so a bound can only widen what a change seeds.
const MANIFEST_MAX_BYTES: u64 = 8 << 20;

/// The diff of the root-relative manifest `path` between `rev` and the
/// working tree. `None` when either side cannot be compared key by key
/// (another manifest form, an added or deleted file, a parse failure, a
/// manifest past [`MANIFEST_MAX_BYTES`]): the change then counts as read.
pub(super) fn manifest_diff(
    root: &Path,
    rev: &str,
    path: &str,
    change: Change,
    cancel: &dyn octocode_runtime_base::execution::cancel::CancellationCheck,
) -> Option<ManifestDiff> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let unread = UNREAD_KEYS
        .iter()
        .find(|(form, _)| *form == name)
        .map(|(_, keys)| *keys)?;
    if change != Change::Modified {
        return None;
    }
    let old = git(root, &["show", &format!("{rev}:./{path}")], cancel).ok()?;
    let file = std::fs::File::open(root.join(path)).ok()?;
    let new = octocode_io::read::read_limited(file, MANIFEST_MAX_BYTES).ok()??;
    let (old, new) = (flatten(name, &old)?, flatten(name, &new)?);
    let keys = old
        .keys()
        .chain(new.keys())
        .filter(|key| old.get(*key) != new.get(*key))
        .cloned()
        .collect::<BTreeSet<_>>();
    let read = keys.iter().any(|key| !unread.contains(&key.as_str()));
    Some(ManifestDiff {
        keys: keys.into_iter().collect(),
        read,
    })
}

/// A manifest's values by compared key: JSON top-level keys; TOML
/// top-level keys, with the [`NESTED_TABLES`] split one level further.
fn flatten(name: &str, bytes: &[u8]) -> Option<Map<String, Value>> {
    let text = std::str::from_utf8(bytes).ok()?;
    if name.ends_with(".json") {
        return match serde_json::from_str::<Value>(text).ok()? {
            Value::Object(map) => Some(map),
            _ => None,
        };
    }
    let table = serde_json::to_value(text.parse::<toml::Table>().ok()?).ok()?;
    let Value::Object(table) = table else {
        return None;
    };
    let mut flat = Map::new();
    for (key, value) in table {
        match value {
            Value::Object(inner) if NESTED_TABLES.contains(&key.as_str()) => {
                for (inner_key, inner_value) in inner {
                    flat.insert(format!("{key}.{inner_key}"), inner_value);
                }
            }
            value => {
                flat.insert(key, value);
            }
        }
    }
    Some(flat)
}
