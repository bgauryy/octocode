//! What a page's scan could not cover (binary, default-excluded, hidden,
//! policy-withheld, unreadable), and how the row says so.

use super::binaries::*;
use super::layout::*;
use super::local_search_query;
use super::local_search_query::MAX_COLLECTED_FILES;
use super::types::*;
use octocode_engine::types::CapReason;

/// What the scan could not cover, and how the row says so.
pub(super) struct Coverage {
    pub(super) error_count: u32,
    /// Of `error_count`: readable files the regex failed on (a match limit).
    pub(super) pattern_error_count: u32,
    /// A file was searched only up to its first NUL (`binaryQuit`).
    pub(super) binary_cut: bool,
    pub(super) binary_files: String,
    pub(super) pcre2_deadline: bool,
    pub(super) skip_hint: Option<String>,
    /// include/exclude filtered out every file: nothing was searched.
    pub(super) scope_miss: bool,
    /// Entries the security path policy withheld from the walk.
    pub(super) withheld: usize,
    /// The warning for an include/exclude glob with a `..` segment, which
    /// never matches: globs match paths under `path`.
    pub(super) parent_glob: Option<String>,
    /// Warnings naming the fields the row's other fields leave without
    /// effect ([`inert_fields`]).
    pub(super) inert: Vec<String>,
    /// The walk stopped before it reached every path (a cap other than a
    /// binary cut, which only ends one file's content).
    pub(super) walk_cut: bool,
}

/// The fields set to a non-default value that the row's other fields leave
/// without effect, one warning per reason: context lines in a view that
/// lists no lines, and include/exclude globs when `path` is one file (it is
/// searched whatever the globs).
fn inert_fields(query: &LocalSearchQuery, root_is_file: bool) -> Vec<String> {
    let set = |fields: &[(&'static str, bool)]| {
        fields
            .iter()
            .filter(|(_, set)| *set)
            .map(|(name, _)| *name)
            .collect::<Vec<_>>()
    };
    let have = |fields: &[&str]| if fields.len() == 1 { "has" } else { "have" };
    let mut warnings = Vec::new();
    let canonical = local_search_query::canonical(query);
    let context = set(&[
        ("contextLines", canonical.context_lines.is_some()),
        ("contextBefore", canonical.context_before.is_some()),
        ("contextAfter", canonical.context_after.is_some()),
    ]);
    if list_view(query.result_view) && !context.is_empty() {
        let view = serde_json::to_value(query.result_view).unwrap_or_default();
        warnings.push(format!(
            "{} {} no effect with resultView {view}: only line views show context.",
            context.join(", "),
            have(&context)
        ));
    }
    let globs = set(&[
        ("include", !query.include.is_empty()),
        ("exclude", !query.exclude.is_empty()),
    ]);
    if root_is_file && !globs.is_empty() {
        warnings.push(format!(
            "{} {} no effect: path is one file, searched whatever the globs.",
            globs.join(", "),
            have(&globs)
        ));
    }
    warnings
}

impl Coverage {
    pub(super) fn of(
        query: &LocalSearchQuery,
        stats: &SearchStats,
        scanned: &octocode_engine::types::TextSearchStats,
        root: &std::path::Path,
        output_root: &std::path::Path,
        skipped: &crate::policy::discovery::WalkSkips,
    ) -> Self {
        let cap_has = |reason: CapReason| stats.cap_reasons.contains(&reason);
        let skip_hint = skipped_target_hint(root.is_file(), stats.files_searched, &stats.cap_reasons)
        .or_else(|| {
            (root.is_file() && scanned.skipped_binary_count.unwrap_or(0) > 0).then(|| {
                "The target file is binary from its leading bytes (a NUL in its header); it was not searched, and no text tool reads it.".into()
            })
        });
        Self {
            error_count: stats.error_count.unwrap_or(0),
            pattern_error_count: scanned.pattern_error_count.unwrap_or(0),
            binary_cut: cap_has(CapReason::BinaryQuit),
            binary_files: binary_file_list(
                scanned.binary_files.as_deref().unwrap_or_default(),
                scanned.binary_file_count.unwrap_or(0),
                output_root,
            ),
            pcre2_deadline: cap_has(CapReason::Pcre2Deadline),
            scope_miss: !root.is_file()
                && stats.files_searched == 0
                && skip_hint.is_none()
                && (!query.include.is_empty() || !query.exclude.is_empty()),
            skip_hint,
            withheld: skipped.withheld.total(),
            parent_glob: crate::policy::include::parent_glob_warning(
                &query.include,
                &query.exclude,
            ),
            inert: inert_fields(query, root.is_file()),
            walk_cut: stats.capped == Some(true)
                && (stats.cap_reasons.is_empty()
                    || stats
                        .cap_reasons
                        .iter()
                        .any(|reason| *reason != CapReason::BinaryQuit)),
        }
    }

    /// Unreadable paths or a binary cut leave coverage incomplete: absence
    /// is unproven, and prefix matches before a NUL are not the file's full
    /// set.
    pub(super) fn gap(&self) -> bool {
        self.error_count > 0 || self.binary_cut
    }

    pub(super) fn warnings(
        &self,
        layout: &Layout,
        shown_redacted: usize,
        unverified: bool,
        empty: bool,
    ) -> Vec<String> {
        let mut warnings = vec![];
        warnings.extend(self.parent_glob.clone());
        warnings.extend(self.inert.iter().cloned());
        // A clipped value is flagged on its row (`originalChars`,
        // `returnedChars`) and read whole by its `expandValues` lead.
        if shown_redacted > 0 && !layout.list {
            warnings.push(format!(
                "redactedMatches: {shown_redacted} returned match value(s) had secret-shaped text replaced by [REDACTED…] placeholders; those values are not verbatim source."
            ));
        }
        if unverified {
            warnings.push(
                "Some match values were redacted because their source file could not be re-read to check clipped text for secrets. Use localFetch at the returned anchors.".into(),
            );
        }
        if self.pcre2_deadline {
            warnings.push(
                "The PCRE2 (regex:\"pcre2\") search stopped at its wall-clock deadline; results cover only the files finished before it.".into(),
            );
        }
        if self.binary_cut {
            warnings.push(format!(
                "binaryFileSkipped: {} searched only up to the first NUL byte.",
                self.binary_files
            ));
        }
        // Hints are shown only on empty/error rows; a partial row keeps the
        // unreadable-path explanation as a warning.
        if self.pattern_error_count > 0 {
            warnings.push(format!(
                "patternLimit: the regex hit the engine's matching limit in {}, which went unsearched; simplify nested repetition, or use regex:\"rust\".",
                crate::response::pages::counted(u64::from(self.pattern_error_count), "file", "files")
            ));
        }
        let unreadable = self.error_count - self.pattern_error_count.min(self.error_count);
        if unreadable > 0 && empty {
            warnings.push(unreadable_hint(unreadable));
        }
        warnings
    }

    /// A scope miss stands only while no skipped path may hold a match: then
    /// the skip, not include/exclude, is why nothing was searched.
    pub(super) fn skipped_paths_hold_matches(&mut self, hints: &mut Vec<String>) {
        self.scope_miss = false;
        hints.retain(|hint| hint != SCOPE_MISS_HINT);
    }

    /// An empty row's one per-call fact beyond `status` and `filesScanned`;
    /// unreadable paths and a binary cut are stated once, in `warnings`. A
    /// plain miss has none: the status and the scanned count already say it.
    pub(super) fn empty_hints(&self, empty: bool) -> Vec<String> {
        if !empty {
            return vec![];
        }
        if self.scope_miss {
            vec![SCOPE_MISS_HINT.into()]
        } else if self.error_count > 0 {
            vec![]
        } else if let Some(hint) = &self.skip_hint {
            vec![hint.clone()]
        } else if self.withheld > 0 {
            // Not a spelling problem: `skippedPaths` names what the policy
            // withheld (once, there).
            vec!["No matches where searched; the withheld entries in skippedPaths may hold it (unproven).".into()]
        } else {
            vec![]
        }
    }
}

/// The empty row's fact when include/exclude left the walk no file.
const SCOPE_MISS_HINT: &str =
    "Nothing searched: include/exclude matched no file; viewTree lists the names to fix the globs.";

/// A query that explicitly targets a single file which the engine then skips
/// (e.g. over the per-file byte ceiling, surfaced as `capReason:"maxFileSize"`
/// with `filesScanned:0`) is a silent false negative without an explanation:
/// nothing was searched, so "no matches" would be misleading.
pub(super) fn skipped_target_hint(
    single_file: bool,
    files_searched: u32,
    cap_reasons: &[CapReason],
) -> Option<String> {
    if cap_reasons.is_empty() {
        return None;
    }
    if single_file && cap_reasons.contains(&CapReason::BinaryQuit) {
        return Some(
            "The target file is binary (NUL byte found); it was not searched past that point."
                .into(),
        );
    }
    (single_file && files_searched == 0).then(|| {
        format!(
            "Nothing searched: the target file was skipped ({}). Read it with localFetch.",
            cap_reasons
                .iter()
                .map(|reason| reason.name())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })
}

/// The engine collects at most [`MAX_COLLECTED_FILES`] files; the rest of
/// the matched files are counted but on no page. Names how many, and how to
/// reach them.
pub(super) fn uncollected_warning(stats: &SearchStats, collected: u32) -> Option<String> {
    let capped = stats.cap_reasons.contains(&CapReason::MaxCollectedFiles);
    let beyond = stats.files_matched.saturating_sub(collected);
    (capped && beyond > 0).then(|| {
        format!(
            "maxCollectedFiles: {beyond} of {} matched files are past the {MAX_COLLECTED_FILES}-file collection cap and no page reaches them; split the search by path or include.",
            stats.files_matched
        )
    })
}

pub(super) fn unreadable_hint(count: u32) -> String {
    format!(
        "{} unreadable",
        crate::response::pages::counted(u64::from(count), "path", "paths")
    )
}
