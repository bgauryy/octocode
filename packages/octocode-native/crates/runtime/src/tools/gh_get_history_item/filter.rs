//! Changed-file selection: `include`/`status`/`minChanges` filters, the
//! commit/compare path scope, and patch-range selection.
use super::HistoryItemRequest;
use super::inventory::*;
use super::util::{str_at, usize_at};
use globset::{GlobBuilder, GlobMatcher};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// Changed-file selection shared by the provider scan and output shaping.
#[derive(Clone, Copy)]
pub(super) struct FileFilter<'a> {
    pub(super) selected: &'a [String],
    pub(super) needle: Option<&'a str>,
    pub(super) scope: Option<&'a InventoryFilter>,
}

impl FileFilter<'_> {
    pub(super) fn is_trivial(&self) -> bool {
        self.selected.is_empty() && self.needle.is_none() && self.scope.is_none()
    }
    pub(super) fn matches(&self, file: &Value) -> bool {
        let path = str_at(file, "/filename").unwrap_or("");
        (self.selected.is_empty() || self.selected.iter().any(|selected| selected == path))
            && self.scope.is_none_or(|scope| scope.matches(file))
            && self.needle.is_none_or(|n| {
                path.to_lowercase().contains(n)
                    || str_at(file, "/patch").is_some_and(|v| v.to_lowercase().contains(n))
            })
    }
}

/// One `include` entry.
pub(super) enum PathPattern {
    /// A plain path: that file, or every file under that directory.
    Scope(String),
    /// A glob over the whole path, or over the file name when it has no `/`.
    Glob {
        matcher: GlobMatcher,
        file_name: bool,
    },
}

impl PathPattern {
    pub(super) fn parse(pattern: &str) -> Result<Self, String> {
        if !pattern.contains(['*', '?', '[', '{']) {
            return Ok(Self::Scope(pattern.to_owned()));
        }
        GlobBuilder::new(pattern)
            .literal_separator(true)
            .build()
            .map(|glob| Self::Glob {
                matcher: glob.compile_matcher(),
                file_name: !pattern.contains('/'),
            })
            .map_err(|error| format!("include: invalid glob {pattern:?}: {error}"))
    }

    pub(super) fn matches(&self, path: &str) -> bool {
        match self {
            Self::Scope(scope) => {
                path == scope
                    || path
                        .strip_prefix(scope.trim_end_matches('/'))
                        .is_some_and(|rest| rest.starts_with('/'))
            }
            Self::Glob { matcher, file_name } => {
                let subject = if *file_name {
                    path.rsplit('/').next().unwrap_or(path)
                } else {
                    path
                };
                matcher.is_match(subject)
            }
        }
    }
}

/// A pull request's `include`/`status`/`minChanges`: path, status and change-count narrowing of
/// the changed-file list. Pages and counts then cover matching files only.
pub(super) struct InventoryFilter {
    pub(super) paths: Vec<PathPattern>,
    pub(super) status: Vec<String>,
    pub(super) min_changes: Option<usize>,
}

impl InventoryFilter {
    pub(super) fn from_query(query: &HistoryItemRequest) -> Result<Option<Self>, String> {
        let crate::contracts::tool_types::GhGetHistoryItemQuery::PullRequest {
            include,
            status,
            min_changes,
            ..
        } = &query.query
        else {
            return Ok(None);
        };
        // With `patchRanges`, `include` names the selected files instead.
        let include = include.as_ref().filter(|_| query.patch_ranges().is_empty());
        if include.is_none() && status.is_none() && min_changes.is_none() {
            return Ok(None);
        }
        let paths = include
            .iter()
            .flat_map(|paths| paths.iter())
            .map(|pattern| PathPattern::parse(pattern))
            .collect::<Result<Vec<_>, _>>()?;
        let status = status
            .iter()
            .flat_map(|status| status.iter())
            .map(ToString::to_string)
            .collect();
        let min_changes = min_changes.map(|n| usize::try_from(n.get()).unwrap_or(usize::MAX));
        Ok(Some(Self {
            paths,
            status,
            min_changes,
        }))
    }

    /// A rename matches by its new or previous path. Files GitHub sent
    /// without line counts (no patch, 0/0) pass `minChanges`: their size is
    /// unknown, not zero.
    pub(super) fn matches(&self, file: &Value) -> bool {
        let status = str_at(file, "/status").unwrap_or("");
        let path = str_at(file, "/filename").unwrap_or("");
        let previous = str_at(file, "/previous_filename");
        let changes = usize_at(file, "/additions") + usize_at(file, "/deletions");
        let countless = changes == 0 && file.get("patch").is_none();
        (self.status.is_empty() || self.status.iter().any(|s| s == status))
            && (self.paths.is_empty()
                || self.paths.iter().any(|pattern| {
                    pattern.matches(path) || previous.is_some_and(|p| pattern.matches(p))
                }))
            && self
                .min_changes
                .is_none_or(|min| countless || changes >= min)
    }
}

pub(super) type PatchRanges = HashMap<String, (Option<Vec<i64>>, Option<Vec<i64>>)>;

/// Selected file names (files plus range targets) and per-file line ranges.
pub(super) fn patch_selection(selector: Option<&Map<String, Value>>) -> (Vec<String>, PatchRanges) {
    let mut selected_names = selector
        .and_then(|v| v.get("files"))
        .and_then(Value::as_array)
        .map(|v| {
            v.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let lines = |range: &Value, key: &str| {
        range
            .get(key)
            .and_then(Value::as_array)
            .map(|lines| lines.iter().filter_map(Value::as_i64).collect::<Vec<_>>())
    };
    let ranges = selector
        .and_then(|v| v.get("ranges"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|range| {
            let file = range.get("file")?.as_str()?.to_owned();
            Some((file, (lines(range, "additions"), lines(range, "deletions"))))
        })
        .collect::<HashMap<_, _>>();
    for file in ranges.keys() {
        if !selected_names.contains(file) {
            selected_names.push(file.clone());
        }
    }
    (selected_names, ranges)
}

/// Scanned files a patch search skipped: selected and in scope, but GitHub
/// sent no patch (too large, or omitted past the diff budget) and the path
/// itself does not match. Binary files hold no text to search. Only REST
/// entries (with a blob `sha`) say anything about patches.
pub(super) fn unsearched_files(
    files: &[Value],
    filter: &FileFilter<'_>,
    needle: &str,
) -> Vec<(&'static str, String)> {
    let scope_only = FileFilter {
        needle: None,
        ..*filter
    };
    files
        .iter()
        .filter(|file| {
            file.get("patch").is_none() && file.get("sha").is_some() && scope_only.matches(file)
        })
        .filter_map(|file| {
            let reason = missing_patch_reason(file).filter(|r| *r != "binary")?;
            let path = str_at(file, "/filename")?;
            (!path.to_lowercase().contains(needle)).then(|| (reason, path.to_owned()))
        })
        .collect()
}

/// A commit or comparison file scope: the `path` prefix and the `include`
/// paths/globs (any-of). A rename matches by its new or previous path.
pub(super) struct PathScope(Vec<PathPattern>);

impl PathScope {
    pub(super) fn from_query(query: &HistoryItemRequest) -> Result<Option<Self>, String> {
        let mut patterns = query
            .path()
            .map(|path| PathPattern::Scope(path.to_owned()))
            .into_iter()
            .collect::<Vec<_>>();
        for pattern in &query.file_scope {
            patterns.push(PathPattern::parse(pattern)?);
        }
        Ok((!patterns.is_empty()).then_some(Self(patterns)))
    }

    pub(super) fn matches(&self, file: &Value) -> bool {
        let name = str_at(file, "/filename").unwrap_or("");
        let previous = str_at(file, "/previous_filename");
        self.0.iter().any(|pattern| {
            pattern.matches(name) || previous.is_some_and(|previous| pattern.matches(previous))
        })
    }
}

/// Whether a file sits in the optional scope.
pub(super) fn in_path_scope(file: &Value, scope: Option<&PathScope>) -> bool {
    scope.is_none_or(|scope| scope.matches(file))
}

pub(super) fn scope_files(files: Vec<Value>, scope: Option<&PathScope>) -> Vec<Value> {
    files
        .into_iter()
        .filter(|file| in_path_scope(file, scope))
        .collect()
}
