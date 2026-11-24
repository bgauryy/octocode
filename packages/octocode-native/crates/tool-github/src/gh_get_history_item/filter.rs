//! Changed-file selection: the `include`/`status` scope every
//! operation shares, and the `matchString` file filter.
use super::HistoryItemRequest;
use super::inventory::*;
use super::util::str_at;
use crate::gh_get_history_item::gh_get_history_item_query;
use serde_json::Value;

/// Changed-file selection shared by the provider scan and output shaping.
#[derive(Clone, Copy)]
pub(super) struct FileFilter<'a> {
    pub(super) needle: Option<&'a super::matcher::Matcher>,
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
    /// Changed files loaded before the filter.
    pub(super) loaded: usize,
}

impl FileFilter<'_> {
    /// One file page for a commit, a comparison and a pull request alike:
    /// the filter runs over every loaded file first, then the page cuts the
    /// kept ones, so page N is the N-th page of files in scope (and, with a
    /// `matchString`, of files whose patch holds it). `patches`: the page
    /// shows patches, which sizes it.
    pub(super) fn page(
        &self,
        files: Vec<Value>,
        state: super::window::WindowState,
        query: &HistoryItemRequest,
        patches: bool,
    ) -> FilteredPage {
        let loaded = files.len();
        let Filtered {
            kept,
            unsearched,
            dropped,
            in_scope,
        } = self.apply(files);
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
            loaded,
        }
    }

    /// Filter `files` once, for every file page (commit, comparison and pull
    /// request alike). A literal is searched in patches, so in-scope files
    /// without one are unsearched, whether the page shows patches or not.
    pub(super) fn apply(&self, files: Vec<Value>) -> Filtered {
        let unsearched = if self.needle.is_some() {
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
            && self
                .needle
                .is_none_or(|n| str_at(file, "/patch").is_some_and(|v| n.matches(v)))
    }
}

/// An `include` scope that kept some of the changed files: the item's
/// `changedFileCount` counts them all.
pub(super) fn include_kept_warning(kept: usize, loaded: usize) -> String {
    format!("include kept {kept} of {loaded} changed files.")
}

/// `matchString` dropped `dropped` of `listed` `noun`s (changed files,
/// comments, reviews): none holds it in its `holder` (a patch line, a body).
pub(super) fn match_dropped_warning(
    dropped: usize,
    listed: usize,
    noun: &str,
    holder: &str,
) -> String {
    format!("matchString dropped {dropped} of {listed} {noun}: no {holder} holds it.")
}

/// A `matchString` that kept none of the listed changed files it could
/// search. It searches each file's diff text (never its path), so the
/// function context git prints after a hunk's `@@ … @@` counts as text: a
/// hit there is not a changed line. The terms are the caller's own and are
/// not echoed: a term can be part of a value the read redacted.
pub(super) fn match_removed_all_warning(listed: usize) -> String {
    format!(
        "matchString matched 0 of {listed} changed files (each file's diff text was searched, hunk-header context included)."
    )
}

/// A read's changed-file scope: the `include` globs (any-of) of a pull
/// request, a commit or a comparison, read from the repository root in the
/// local walks' dialect ([`crate::include_globs`]), and a pull request's
/// `status`. Pages and counts then cover matching files only.
pub(super) struct FileScope {
    pub(super) paths: Option<crate::include_globs::IncludeGlobs>,
    pub(super) status: Vec<String>,
}

impl FileScope {
    /// The scope `query` names; `None` when it names none.
    pub(super) fn from_query(query: &HistoryItemRequest) -> Result<Option<Self>, String> {
        let include = gh_get_history_item_query::include_patterns(query);
        let paths = if include.is_empty() {
            None
        } else {
            Some(crate::include_globs::IncludeGlobs::new(include)?)
        };
        let status = gh_get_history_item_query::file_status(query);
        if paths.is_none() && status.is_empty() {
            return Ok(None);
        }
        Ok(Some(Self { paths, status }))
    }

    /// A rename matches by its new or previous path.
    pub(super) fn matches(&self, file: &Value) -> bool {
        let status = str_at(file, "/status").unwrap_or("");
        let path = str_at(file, "/filename").unwrap_or("");
        let previous = str_at(file, "/previous_filename");
        (self.status.is_empty() || self.status.iter().any(|s| s == status))
            && self.paths.as_ref().is_none_or(|globs| {
                globs.matches(path, false) || previous.is_some_and(|p| globs.matches(p, false))
            })
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
