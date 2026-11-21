//! What a page's scan could not cover (binary, default-excluded, hidden,
//! policy-withheld, unreadable), and how the row says so.

use super::binaries::*;
use super::layout::*;
use super::local_search_query;
use super::types::*;
use octocode_contract_types::LocalSearchQuery;
use octocode_engine::num::saturating_u32;
use octocode_engine::types::CapReason;

/// What the scan could not cover, and how the row says so.
pub(super) struct Coverage {
    pub(super) error_count: u32,
    /// Of `error_count`: readable files the regex failed on (a match limit).
    pub(super) pattern_error_count: u32,
    /// A file was searched only up to its first NUL (`binaryQuit`).
    pub(super) binary_cut: bool,
    pub(super) binary_files: String,
    /// The returned files whose values are decoded text, not UTF-8 bytes.
    pub(super) not_utf8_files: Option<String>,
    /// The returned files whose leading UTF-8 byte-order mark was dropped.
    pub(super) utf8_bom_files: Option<String>,
    /// The returned files that grew while they were searched, each with the
    /// bytes its hits are of.
    pub(super) grown_files: Option<String>,
    pub(super) skip_hint: Option<String>,
    /// include/exclude filtered out every file: nothing was searched.
    pub(super) scope_miss: bool,
    /// `keyPath` left out every file in scope, none of them a JSON or TOML
    /// document: how many it left out, when nothing was searched.
    pub(super) key_path_miss: Option<u32>,
    /// The walk found no file: no include/exclude, scope or skip explains it.
    pub(super) nothing_walked: bool,
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
    let globs = set(&[
        ("include", !local_search_query::include(query).is_empty()),
        ("exclude", !local_search_query::exclude(query).is_empty()),
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

/// Each file that grew while it was searched, under `root`, with the bytes
/// its hits are of: `app.log (as of 512 bytes)`.
fn grown_file_list(files: &[octocode_engine::types::GrownFile], root: &std::path::Path) -> String {
    files
        .iter()
        .map(|file| {
            let path = std::path::Path::new(&file.path);
            let shown = path
                .strip_prefix(root)
                .ok()
                .filter(|relative| !relative.as_os_str().is_empty())
                .unwrap_or(path);
            format!(
                "{} (as of {})",
                octocode_engine::text::slash_path(shown),
                octocode_runtime_base::response::pages::counted(file.bytes, "byte", "bytes")
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

impl Coverage {
    pub(super) fn of(
        query: &LocalSearchQuery,
        stats: &SearchStats,
        scanned: &octocode_engine::types::TextSearchStats,
        root: &std::path::Path,
        output_root: &std::path::Path,
        skipped: &octocode_runtime_base::policy::discovery::WalkSkips,
    ) -> Self {
        let cap_has = |reason: CapReason| stats.cap_reasons.contains(&reason);
        // A file named alone and binary from its leading bytes never gets
        // here: [`super::executor`] refuses it as `binaryFileUnsupported`.
        let skip_hint = skipped_target_hint(root, stats.files_searched, &stats.cap_reasons);
        // A key path reads only JSON and TOML documents: with nothing
        // searched, the files it left out are why, whatever else was set.
        let key_path_miss = scanned
            .scope_left_out
            .filter(|_| {
                local_search_query::key_path(query).is_some()
                    && stats.files_searched == 0
                    && skip_hint.is_none()
            })
            .filter(|left_out| *left_out > 0);
        Self {
            error_count: stats.error_count.unwrap_or(0),
            pattern_error_count: scanned.pattern_error_count.unwrap_or(0),
            binary_cut: cap_has(CapReason::BinaryQuit),
            binary_files: binary_file_list(
                scanned.binary_files.as_deref().unwrap_or_default(),
                scanned.binary_file_count.unwrap_or(0),
                output_root,
            ),
            not_utf8_files: scanned
                .not_utf8_files
                .as_deref()
                .filter(|files| !files.is_empty())
                .map(|files| binary_file_list(files, saturating_u32(files.len()), output_root)),
            utf8_bom_files: scanned
                .utf8_bom_files
                .as_deref()
                .filter(|files| !files.is_empty())
                .map(|files| binary_file_list(files, saturating_u32(files.len()), output_root)),
            grown_files: scanned
                .grown_files
                .as_deref()
                .filter(|files| !files.is_empty())
                .map(|files| grown_file_list(files, output_root)),
            scope_miss: !root.is_file()
                && stats.files_searched == 0
                && skip_hint.is_none()
                && key_path_miss.is_none()
                && (!local_search_query::include(query).is_empty()
                    || !local_search_query::exclude(query).is_empty()),
            nothing_walked: !root.is_file()
                && stats.files_searched == 0
                && skip_hint.is_none()
                && key_path_miss.is_none()
                && local_search_query::include(query).is_empty()
                && local_search_query::exclude(query).is_empty()
                && scanned
                    .test_code_excluded
                    .is_none_or(|excluded| excluded.files == 0),
            skip_hint,
            key_path_miss,
            withheld: skipped.withheld.total(),
            parent_glob: octocode_runtime_base::policy::include::parent_glob_warning(
                local_search_query::include(query),
                local_search_query::exclude(query),
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

    /// The declared reasons of the gaps no cap names: unreadable files
    /// (`filesSkipped`) and files the regex hit its matching limit in
    /// (`regexMatchLimit`), so a partial row states why.
    pub(super) fn partial_reasons(
        &self,
    ) -> Vec<octocode_runtime_base::response::paging::PartialReason> {
        use octocode_runtime_base::response::paging::PartialReason;
        let unreadable = self.error_count.saturating_sub(self.pattern_error_count) > 0;
        [
            (unreadable, PartialReason::FilesSkipped),
            (self.pattern_error_count > 0, PartialReason::RegexMatchLimit),
        ]
        .into_iter()
        .filter_map(|(gap, reason)| gap.then_some(reason))
        .collect()
    }

    pub(super) fn warnings(&self, layout: &Layout, shown_redacted: usize) -> Vec<String> {
        let mut warnings = vec![];
        warnings.extend(self.parent_glob.clone());
        warnings.extend(self.inert.iter().cloned());
        if shown_redacted > 0 && !layout.list {
            warnings.push(octocode_runtime_base::security::redacted_mark());
        }
        // Values are decoded as localFetch decodes the file; say so once.
        if let Some(files) = &self.not_utf8_files
            && !layout.list
        {
            warnings.push(format!(
                "notUtf8: {files} not valid UTF-8: match values are decoded text (Latin-1, or U+FFFD for invalid bytes), as localFetch reads the file, not verbatim bytes."
            ));
        }
        if let Some(files) = &self.utf8_bom_files
            && !layout.list
        {
            warnings.push(format!(
                "utf8Bom: {files} searched without its leading UTF-8 byte-order mark (3 bytes), as localFetch reads it: line 1 columns and byte offsets count the text after it."
            ));
        }
        if let Some(files) = &self.grown_files {
            warnings.push(format!(
                "grewWhileRead: {files} grew while searched: hits are of the bytes named, the start of one version; text written after that read is not searched."
            ));
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
                octocode_runtime_base::response::pages::counted(u64::from(self.pattern_error_count), "file", "files")
            ));
        }
        warnings
    }

    /// The empty row's fact when include/exclude left the walk no file: the
    /// globs matched none, or only files the security path policy withholds
    /// ([`octocode_runtime_base::policy::include::glob_miss_hint`]).
    fn scope_miss_hint(&self) -> String {
        octocode_runtime_base::policy::include::glob_miss_hint(self.withheld).into_owned()
    }

    /// The empty row's fact when `keyPath` left out every file in scope.
    pub(super) fn key_path_miss_hint(&self) -> Option<String> {
        self.key_path_miss.map(key_path_miss_hint)
    }

    /// An empty row's fact about its scope: include/exclude matched no
    /// file, nothing was under the path, or the one target file was
    /// skipped. `sparse`: the scope is a checkout of some of its
    /// repository, where the path and glob advice does not hold (the row's
    /// warning says what is on disk). Unreadable paths are stated in
    /// `warnings`; a plain miss has none.
    pub(super) fn scope_hint(&self, sparse: bool) -> Option<String> {
        if self.key_path_miss.is_some() {
            None
        } else if self.scope_miss {
            (!sparse).then(|| self.scope_miss_hint())
        } else if self.nothing_walked && self.withheld == 0 && self.error_count == 0 {
            (!sparse).then(|| NOTHING_WALKED_HINT.into())
        } else if self.error_count > 0 {
            None
        } else {
            self.skip_hint.clone()
        }
    }
}

/// What an empty page over files holding `masked` masked spans says: text
/// a read masks as a secret is matched by no search. With nothing else left
/// out everything else was searched, so the absence is exact outside those
/// values; when the walk also left paths out (`left_out`: withheld, ignored,
/// hidden, excluded, unreadable or binary) the sentence leaves them out too
/// and proves no absence beyond the text searched.
pub(super) fn masked_hint(masked: u32, left_out: bool) -> String {
    let masked = masked_unsearched(masked);
    if left_out {
        format!("No match in the searched text; {masked}, and skippedPaths were not searched.")
    } else {
        format!("No match in the searched text; {masked}, so absence is proven everywhere else.")
    }
}

/// What a miss over `masked` masked spans leaves out, as a clause: the
/// wording a read's miss uses too.
pub(super) fn masked_unsearched(masked: u32) -> String {
    octocode_tools_shared::source_view::masked_unsearched(
        usize::try_from(masked).unwrap_or(usize::MAX),
    )
}

/// The empty row's fact when the walk found no file at all, with no
/// include/exclude, test-code scope or skip to blame.
const NOTHING_WALKED_HINT: &str = "Nothing searched: no file under path; check the path, or set includeHidden/includeIgnored when the files are dot or ignored entries.";

/// The empty row's fact when `keyPath` left out every file in scope, its
/// `left_out` count of them: none is a JSON or TOML document. The related[]
/// call is the same search without the key path.
fn key_path_miss_hint(left_out: u32) -> String {
    format!(
        "Nothing searched: keyPath reads only .json, .jsonc, .json5 and .toml files, and left out the {} in scope; the related[] call searches without keyPath.",
        octocode_engine::text::counted(left_out, "file", "files")
    )
}

/// A query that explicitly targets a single file which the engine then skips
/// (e.g. over the per-file byte ceiling, surfaced as `capReason:"fileTooLarge"`
/// with `filesScanned:0`) is a silent false negative without an explanation:
/// nothing was searched, so "no matches" would be misleading. The hint
/// names localFetch only while its streamed windows read the file.
pub(super) fn skipped_target_hint(
    root: &std::path::Path,
    files_searched: u32,
    cap_reasons: &[CapReason],
) -> Option<String> {
    if cap_reasons.is_empty() {
        return None;
    }
    let single_file = root.is_file();
    if single_file && cap_reasons.contains(&CapReason::BinaryQuit) {
        return Some(
            "The target file is binary (NUL byte found); it was not searched past that point."
                .into(),
        );
    }
    (single_file && files_searched == 0).then(|| {
        let reasons = cap_reasons
            .iter()
            .map(|reason| reason.name())
            .collect::<Vec<_>>()
            .join(", ");
        let readable = std::fs::metadata(root).is_ok_and(|meta| {
            meta.len() <= octocode_tools_shared::source_view::prepare::MAX_STREAM_SOURCE_BYTES
        });
        if readable {
            format!("Nothing searched: the target file was skipped ({reasons}). Read it with localFetch.")
        } else {
            format!(
                "Nothing searched: the target file was skipped ({reasons}); it is past every local reader's size ceiling."
            )
        }
    })
}
