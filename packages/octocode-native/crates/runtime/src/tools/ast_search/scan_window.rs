//! The one rule for continuing an astSearch scan that stopped before its
//! scope ended: the lead's `maxFiles` and `scanOffset` stay inside the
//! contract, and a cut that no valid call can continue is terminal.
use crate::contracts::query_limits::ast_search;
use crate::num::u32_of_usize;

/// The fields a continuation sets: a raised `maxFiles` (an explicit-limit cut
/// only) and the `scanOffset` the scan resumes from.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct ScanWindow {
    pub(super) max_files: Option<u32>,
    pub(super) scan_offset: u32,
}

/// The contract maxima of one operation's scan fields.
#[derive(Clone, Copy)]
pub(super) struct ScanLimits {
    pub(super) max_files: usize,
    pub(super) scan_offset: usize,
}

impl ScanLimits {
    pub(super) const MATCH: Self = Self {
        max_files: ast_search::r#match::MAX_FILES_MAXIMUM,
        scan_offset: ast_search::r#match::SCAN_OFFSET_MAXIMUM,
    };
    pub(super) const SYMBOLS: Self = Self {
        max_files: ast_search::symbols::MAX_FILES_MAXIMUM,
        scan_offset: ast_search::symbols::SCAN_OFFSET_MAXIMUM,
    };
}

/// Where a cut scan continues, `None` when nothing can: the scan was whole,
/// or the cut is terminal (an explicit `maxFiles` at its maximum, or an
/// unbounded scan that reached the walk ceiling).
///
/// An explicit `maxFiles` cut scans on with a doubled bound after the files
/// it evaluated. An unbounded scan cut at its time budget keeps the budget
/// and resumes where it stopped.
pub(super) fn expand_scan(
    truncated: bool,
    max_files: Option<u32>,
    resume: Option<u32>,
    limits: ScanLimits,
) -> Option<ScanWindow> {
    if !truncated {
        return None;
    }
    let within = |offset: u32| offset as usize <= limits.scan_offset;
    match max_files {
        None => octocode_engine::scan_budget::resume_offset(resume?)
            .filter(|offset| within(*offset))
            .map(|scan_offset| ScanWindow {
                max_files: None,
                scan_offset,
            }),
        Some(limit) => ((limit as usize) < limits.max_files && within(limit)).then(|| ScanWindow {
            max_files: Some(u32_of_usize(
                (limit as usize).saturating_mul(2).min(limits.max_files),
            )),
            scan_offset: limit,
        }),
    }
}

/// The `partialReasons` entry of a cut scan: the caller's `maxFiles`, or
/// the time budget of a scan without one.
pub(super) fn cut_reason(max_files: Option<u32>) -> &'static str {
    if max_files.is_some() {
        "maxFiles"
    } else {
        "scanBudget"
    }
}

/// Why an unbounded cut has no continuation, for the scan diagnostic: the
/// walk ceiling, reachable files only by narrowing the scope.
pub(super) fn ceiling_note(resume: Option<u32>) -> Option<String> {
    let ceiling = octocode_engine::scan_budget::walk_ceiling();
    resume
        .filter(|resume| octocode_engine::scan_budget::resume_offset(*resume).is_none())
        .map(|_| {
            format!(
                "The scan reached its walk ceiling of {ceiling} files and cannot resume past it; narrow path, include or exclude to reach files beyond it."
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use octocode_engine::scan_budget::SCAN_WALK_FILES;

    const BOTH: [ScanLimits; 2] = [ScanLimits::MATCH, ScanLimits::SYMBOLS];

    /// The engine walks at most `SCAN_WALK_FILES` candidates; the contract
    /// bounds `maxFiles` and `scanOffset` at the same number for every
    /// operation, so a lead never needs a value the contract rejects.
    #[test]
    fn the_contract_scan_bounds_are_the_engine_walk_ceiling() {
        for limits in BOTH {
            assert_eq!(limits.max_files, SCAN_WALK_FILES);
            assert_eq!(limits.scan_offset, SCAN_WALK_FILES);
        }
    }

    #[test]
    fn a_whole_scan_has_no_continuation() {
        for limits in BOTH {
            assert_eq!(expand_scan(false, None, Some(5), limits), None);
            assert_eq!(expand_scan(false, Some(5), None, limits), None);
        }
    }

    #[test]
    fn a_time_budget_cut_resumes_below_the_ceiling_and_is_terminal_at_it() {
        for limits in BOTH {
            let last = SCAN_WALK_FILES as u32 - 1;
            assert_eq!(
                expand_scan(true, None, Some(last), limits),
                Some(ScanWindow {
                    max_files: None,
                    scan_offset: last
                })
            );
            // Resuming at the ceiling skips the whole walk: an endless loop.
            for resume in [SCAN_WALK_FILES as u32, 74_240, u32::MAX] {
                let lead = expand_scan(true, None, Some(resume), limits);
                assert_eq!(lead.is_some(), resume < SCAN_WALK_FILES as u32, "{resume}");
            }
            assert_eq!(
                expand_scan(true, None, Some(SCAN_WALK_FILES as u32), limits),
                None
            );
            assert_eq!(expand_scan(true, None, None, limits), None);
        }
    }

    #[test]
    fn an_explicit_limit_doubles_within_the_contract_maximum() {
        for limits in BOTH {
            let half = (limits.max_files / 2 + 10) as u32;
            assert_eq!(
                expand_scan(true, Some(10), None, limits),
                Some(ScanWindow {
                    max_files: Some(20),
                    scan_offset: 10
                })
            );
            assert_eq!(
                expand_scan(true, Some(half), None, limits),
                Some(ScanWindow {
                    max_files: Some(limits.max_files as u32),
                    scan_offset: half
                })
            );
            assert_eq!(
                expand_scan(true, Some(limits.max_files as u32), None, limits),
                None
            );
        }
    }

    #[test]
    fn the_ceiling_is_named_only_when_the_resume_reached_it() {
        assert!(ceiling_note(Some(SCAN_WALK_FILES as u32)).is_some());
        assert!(ceiling_note(Some(74_240)).is_none());
        assert!(ceiling_note(None).is_none());
    }
}
