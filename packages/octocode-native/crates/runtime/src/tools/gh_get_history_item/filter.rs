//! Changed-file selection: the `include`/`status`/`minChanges` scope every
//! operation shares, and the `matchString` file filter.
use super::HistoryItemRequest;
use super::inventory::*;
use super::util::{str_at, usize_at};
use crate::num::usize_of;
use crate::tools::gh_get_history_item::gh_get_history_item_query;
use globset::{GlobBuilder, GlobMatcher};
use serde_json::Value;

/// Changed-file selection shared by the provider scan and output shaping.
#[derive(Clone, Copy)]
pub(super) struct FileFilter<'a> {
    pub(super) needle: Option<&'a str>,
    pub(super) scope: Option<&'a FileScope>,
}

/// A changed-file list through a read's filter.
pub(super) struct Filtered {
    /// The files the filter keeps, in order.
    pub(super) kept: Vec<Value>,
    /// In-scope files a patch search could not search
    /// ([`unsearched_files`]).
    pub(super) unsearched: Vec<(&'static str, String, bool)>,
    /// In-scope files searched and dropped: no patch line holds the literal.
    pub(super) dropped: usize,
    /// Files in scope.
    pub(super) in_scope: usize,
}

/// One page of a changed-file list through a read's filter.
pub(super) struct FilteredPage {
    /// The page's files (provider-shaped), in order.
    pub(super) files: Vec<Value>,
    /// The page object.
    pub(super) page: Value,
    /// Files the filter kept across every loaded file.
    pub(super) kept: usize,
    /// In-scope files a patch search could not search.
    pub(super) unsearched: Vec<(&'static str, String, bool)>,
    /// In-scope files searched and dropped.
    pub(super) dropped: usize,
    /// Files in scope.
    pub(super) in_scope: usize,
}

impl FileFilter<'_> {
    /// One file page for a commit, a comparison and a pull request alike:
    /// the filter runs over every loaded file first, then the page cuts the
    /// kept ones, so page N is the N-th page of files in scope (and, with a
    /// `matchString` read of patches, of files whose patch holds it).
    pub(super) fn page(
        &self,
        files: Vec<Value>,
        state: super::window::WindowState,
        query: &HistoryItemRequest,
        patches: bool,
    ) -> FilteredPage {
        let Filtered {
            kept,
            unsearched,
            dropped,
            in_scope,
        } = self.apply(files, patches);
        let count = kept.len();
        let (files, page) = state.paginate(
            kept,
            gh_get_history_item_query::page(query),
            Some(file_page_size(query, patches)),
        );
        FilteredPage {
            files,
            page,
            kept: count,
            unsearched,
            dropped,
            in_scope,
        }
    }

    /// Filter `files` once, for every file page (commit, comparison and pull
    /// request alike). `searches_patches`: the read searches the literal in
    /// patches, so in-scope files without one are unsearched.
    pub(super) fn apply(&self, files: Vec<Value>, searches_patches: bool) -> Filtered {
        let unsearched = if self.needle.is_some() && searches_patches {
            unsearched_files(&files, self)
        } else {
            Vec::new()
        };
        let in_scope = files
            .iter()
            .filter(|file| self.scope.is_none_or(|scope| scope.matches(file)))
            .count();
        let kept = files
            .into_iter()
            .filter(|file| self.matches(file))
            .collect::<Vec<_>>();
        let dropped = in_scope.saturating_sub(kept.len() + unsearched.len());
        Filtered {
            kept,
            unsearched,
            dropped,
            in_scope,
        }
    }

    pub(super) fn is_trivial(&self) -> bool {
        self.needle.is_none() && self.scope.is_none()
    }
    /// In scope, and (with a needle) its patch holds the needle: a path
    /// alone never keeps a file, whose whole patch would answer nothing.
    pub(super) fn matches(&self, file: &Value) -> bool {
        self.scope.is_none_or(|scope| scope.matches(file))
            && self.needle.is_none_or(|n| {
                str_at(file, "/patch").is_some_and(|v| super::text::contains_folded(v, n))
            })
    }
}

/// The per-call count of changed files a `matchString` dropped: in scope,
/// searched, and holding no hit.
pub(super) fn match_dropped_warning(dropped: usize, listed: usize) -> String {
    format!("matchString dropped {dropped} of {listed} changed files: no patch line holds it.")
}

/// A `matchString` that kept none of the listed changed files it could
/// search. It searches each file's diff text (never its path), so the
/// function context git prints after a hunk's `@@ … @@` counts as text: a
/// hit there is not a changed line.
pub(super) fn match_removed_all_warning(needle: &str, listed: usize) -> String {
    format!(
        "matchString matched 0 of {listed} changed files (it searched {needle:?} in each file's diff text, hunk-header context included); drop matchString to list the files."
    )
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

/// A read's changed-file scope: the `include` paths, `dir/` prefixes and
/// globs (any-of) of a pull request, a commit or a comparison, and a pull
/// request's `status`/`minChanges`. Pages and counts then cover matching
/// files only.
pub(super) struct FileScope {
    pub(super) paths: Vec<PathPattern>,
    pub(super) status: Vec<String>,
    pub(super) min_changes: Option<usize>,
}

impl FileScope {
    /// The scope `query` names; `None` when it names none.
    pub(super) fn from_query(query: &HistoryItemRequest) -> Result<Option<Self>, String> {
        let paths = gh_get_history_item_query::include_patterns(query)
            .into_iter()
            .map(PathPattern::parse)
            .collect::<Result<Vec<_>, _>>()?;
        let (status, min_changes) = match &query.query {
            octocode_contract_types::GhGetHistoryItemQuery::PullRequest {
                status,
                min_changes,
                ..
            } => (
                status
                    .iter()
                    .flat_map(|status| status.iter())
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
                min_changes.map(|n| usize_of(n.get())),
            ),
            _ => (Vec::new(), None),
        };
        if paths.is_empty() && status.is_empty() && min_changes.is_none() {
            return Ok(None);
        }
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

/// Scanned files a patch search skipped: selected and in scope, but GitHub
/// sent no patch (too large, or omitted past the diff budget). Binary files
/// hold no text to search. Only REST entries (with a blob `sha`) say
/// anything about patches. Each is `(reason, path, removed)`: a removed
/// file's text is at the base, not the head.
pub(super) fn unsearched_files(
    files: &[Value],
    filter: &FileFilter<'_>,
) -> Vec<(&'static str, String, bool)> {
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
            Some((
                reason,
                path.to_owned(),
                str_at(file, "/status") == Some("removed"),
            ))
        })
        .collect()
}
