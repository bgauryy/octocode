//! Which files' rows state the byte provenance a snapshot recorded of them.
//!
//! A build parses each source's own text, so its digests, byte ranges,
//! origins and the occurrence ids made from them count that text, hidden
//! values included: shown, a digest or a length would test a guess of a
//! value every read hides. The build judges each source whole, as every
//! read does, and flags the file a read hides a value of
//! ([`ROLE_HIDDEN_VALUE`]); that file's rows are placed by path, line and
//! their range in the text a read shows ([`Graph::shown_range`]), which
//! counts the placeholders, not the values. The snapshot keeps its digests
//! for staleness checks; no row of such a file shows them.
use super::super::format::NodeRec;
use super::Graph;
use crate::ast_graph::classify::ROLE_HIDDEN_VALUE;
use octocode_engine::text::analysis::ByteRange;

impl Graph {
    /// Whether rows of `file`, a file node's record, state the byte
    /// provenance the snapshot recorded of it: only when a read of it hid
    /// nothing when it was built. A point answer passes the record it read
    /// ([`Graph::node_record`]); a page has every node decoded.
    pub(super) fn shows_provenance(file: &NodeRec) -> bool {
        file.flags & ROLE_HIDDEN_VALUE == 0
    }
}

/// A source range of a file a read hides a value of, as the text a read
/// shows: positions past a replaced run move by what replaced it.
#[derive(Debug, Eq, PartialEq)]
pub(super) enum ShownRange {
    /// No replaced run lies in it: the same text, at the shown position.
    Clear(ByteRange),
    /// It holds whole replaced runs: the shown text has their placeholders.
    Replaced(ByteRange),
    /// It starts or ends inside a replaced run: no shown range is its own.
    Cut,
    /// The snapshot records no replaced runs (a build before it recorded
    /// them): no position is stated.
    Unknown,
}

impl Graph {
    /// `range`, of `file`'s source, in the text a read of `file` shows
    /// (`manifest.hiddenSpans`, recorded at build).
    pub(super) fn shown_range(&self, file: &str, range: ByteRange) -> ShownRange {
        let Some(runs) = self.manifest["hiddenSpans"][file].as_array() else {
            return ShownRange::Unknown;
        };
        let runs = runs
            .iter()
            .filter_map(|run| {
                let at = |index: usize| run[index].as_u64().and_then(|n| i64::try_from(n).ok());
                Some([at(0)?, at(1)?, at(2)?, at(3)?])
            })
            .collect::<Vec<_>>();
        shown_range(&runs, range)
    }
}

/// [`Graph::shown_range`] over `runs`, ascending `[sourceStart, sourceEnd,
/// shownStart, shownEnd]`.
fn shown_range(runs: &[[i64; 4]], range: ByteRange) -> ShownRange {
    // A position outside every run, moved by the runs before it; `None`
    // inside one.
    let shown = |at: i64| {
        let mut moved = 0;
        for [start, end, _, shown_end] in runs {
            if at <= *start {
                break;
            }
            if at < *end {
                return None;
            }
            moved = shown_end - end;
        }
        u32::try_from(at + moved).ok()
    };
    let (start, end) = (i64::from(range.start_byte), i64::from(range.end_byte));
    match (shown(start), shown(end)) {
        (Some(from), Some(to)) => {
            let shown = ByteRange::new(from, to);
            let holds =
                |[run_start, run_end, ..]: &[i64; 4]| start <= *run_start && *run_end <= end;
            if start < end && runs.iter().any(holds) {
                ShownRange::Replaced(shown)
            } else {
                ShownRange::Clear(shown)
            }
        }
        _ => ShownRange::Cut,
    }
}

#[cfg(test)]
mod shown_range_tests;
