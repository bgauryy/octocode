//! Scan totals: what each candidate counted toward, the warnings and status.

use super::file_scan::Tally;
use crate::structural::query::Prefilter;
use crate::structural::types::StructuralSearchDetailedFileResult;

#[derive(Default)]
pub(super) struct ScanCounts {
    pub(super) total_matches: u32,
    pub(super) parsed_files: u32,
    pub(super) skipped_by_pre_filter: u32,
    pub(super) skipped_unsupported: u32,
    pub(super) skipped_unreadable: u32,
    pub(super) skipped_large: u32,
    pub(super) compile_failures: u32,
    pub(super) walk_errors: u32,
}

impl ScanCounts {
    pub(super) fn record(&mut self, tally: Tally) {
        match tally {
            Tally::None => {}
            Tally::PreFilter => self.skipped_by_pre_filter += 1,
            Tally::Unsupported => self.skipped_unsupported += 1,
            Tally::Unreadable => self.skipped_unreadable += 1,
            Tally::Large => self.skipped_large += 1,
            Tally::CompileFailure => self.compile_failures += 1,
            Tally::Parsed(count) => {
                self.parsed_files += 1;
                self.total_matches = self.total_matches.saturating_add(count);
            }
        }
    }

    pub(super) fn warnings(
        &self,
        prefilter: &Prefilter,
        is_rule: bool,
        max_file_bytes: u64,
    ) -> Vec<String> {
        let Self {
            parsed_files,
            skipped_by_pre_filter,
            skipped_unsupported,
            skipped_unreadable,
            skipped_large,
            ..
        } = self;
        let mut warnings = Vec::new();
        match prefilter {
            Prefilter::None => {
                warnings.push(format!(
                    "No literal anchor in the {} — parsed all {parsed_files} supported candidate file(s) with no text pre-filter.",
                    if is_rule { "rule" } else { "pattern" }
                ));
            }
            Prefilter::Single(_) if *skipped_by_pre_filter > 0 => {
                warnings.push(format!(
                    "Pre-filter skipped parsing {skipped_by_pre_filter} file(s); parsed {parsed_files}."
                ));
            }
            Prefilter::Union(anchors) if *skipped_by_pre_filter > 0 => {
                warnings.push(format!(
                    "Union pre-filter ({} anchors: {}) skipped {skipped_by_pre_filter} file(s); parsed {parsed_files}.",
                    anchors.len(),
                    anchors.join("|")
                ));
            }
            _ => {}
        }
        if *skipped_unsupported > 0 {
            warnings.push(format!(
                "Skipped {skipped_unsupported} candidate file(s) with unsupported extensions."
            ));
        }
        if *skipped_unreadable > 0 {
            warnings.push(format!(
                "Skipped {skipped_unreadable} unreadable or vanished candidate file(s)."
            ));
        }
        if *skipped_large > 0 {
            warnings.push(format!(
                "Skipped {skipped_large} candidate file(s) larger than {max_file_bytes} bytes."
            ));
        }
        warnings
    }

    pub(super) fn status(&self, files: &[StructuralSearchDetailedFileResult]) -> &'static str {
        if files.iter().any(|file| file.status == "truncated") {
            "truncated"
        } else if self.compile_failures > 0 {
            "parserFailed"
        } else if self.parsed_files == 0
            && self.skipped_unsupported > 0
            && self.skipped_by_pre_filter == 0
            && self.skipped_unreadable == 0
            && self.skipped_large == 0
            && self.walk_errors == 0
        {
            "unsupported"
        } else if self.skipped_unsupported > 0
            || self.skipped_unreadable > 0
            || self.skipped_large > 0
            || self.walk_errors > 0
        {
            "partial"
        } else {
            "ok"
        }
    }
}
