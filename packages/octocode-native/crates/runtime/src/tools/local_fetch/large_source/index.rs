//! What earlier passes over a streamed source learned, memoized per source
//! version: its checkpoint index and byte pages' range views.

use super::held::Want;
use super::stream::{stream, stream_indexed};
use super::{MARKER_PREFIX_BYTES, Streamed};
use crate::execution::cancel::CancellationCheck;
use crate::security::{KeyBlockState, LineShapeScan};
use std::path::Path;

/// View bytes between two [`ViewPoint`]s: a later byte page of a range
/// streams about this much before its window.
pub(super) const VIEW_POINT_BYTES: usize = 1024 * 1024;

/// A place inside one range's byte view where a later byte page resumes:
/// the source offset of a piece and the state the range's bytes before it
/// leave (line, private-key block, marker head and line shape).
#[derive(Clone, Debug)]
pub(super) struct ViewPoint {
    /// Bytes of the range's view before the piece.
    pub(super) view: usize,
    pub(super) offset: u64,
    pub(super) line: usize,
    /// `view` at `line`'s start.
    pub(super) line_view_start: usize,
    /// `line`'s leading bytes before the piece.
    pub(super) head: Vec<u8>,
    pub(super) block: KeyBlockState,
    pub(super) shape: LineShapeScan,
    /// Range lines inside a private-key block before the piece.
    pub(super) inside_lines: usize,
}

/// One byte-paged range's view, learned on a pass that read all of it: its
/// size, whether a private-key block reaches it, and a [`ViewPoint`] about
/// every [`VIEW_POINT_BYTES`]. A later byte page of the range on the same
/// source version resumes at the point before its window and, when no key
/// block reaches the range, stops once its window is full.
#[derive(Debug)]
pub(super) struct RangeView {
    pub(super) stamp: octocode_engine::stamp::SourceStamp,
    pub(super) total: usize,
    pub(super) keyed: bool,
    pub(super) points: Vec<ViewPoint>,
}

impl RangeView {
    /// The last point at or before view byte `from`.
    pub(super) fn resume_at(&self, from: usize) -> Option<&ViewPoint> {
        let at = self.points.partition_point(|point| point.view <= from);
        self.points.get(at.checked_sub(1)?)
    }
}

/// Range views of streamed sources by canonical path and line range. The
/// byte weight bounds the memo; the entry cap only stops a flood of tiny
/// views (a view weighs about 350 KB per GB of range).
static RANGE_VIEWS: octocode_io::memo::Memo<(std::path::PathBuf, usize, usize), RangeView> =
    octocode_io::memo::Memo::new(octocode_io::memo::MemoLimits {
        max_entries: 256,
        max_weight: 4 * 1024 * 1024,
        keep_newest: false,
    });

/// The one byte page `wants` asks for, if that is all it asks for.
pub(super) fn byte_page(wants: &[Want]) -> Option<Want> {
    match wants {
        [want] if want.byte_page => Some(*want),
        _ => None,
    }
}

/// Lines between two checkpoints of a [`StreamIndex`]: a later page streams
/// at most this many lines before its window.
pub(super) const CHECKPOINT_LINES: usize = 4_096;

/// Where a line starts in the source, and the private-key block state the
/// lines before it leave.
#[derive(Clone, Copy, Debug)]
pub(super) struct Checkpoint {
    pub(super) line: usize,
    pub(super) offset: u64,
    pub(super) block: KeyBlockState,
}

/// What one whole pass over a streamed source learned: its digest, line
/// count, whether it holds a NUL byte, and a [`Checkpoint`] at every
/// [`CHECKPOINT_LINES`]-th line from line 1. While the file keeps the
/// settled stamp the pass was made under, a later page of the same version
/// streams from the checkpoint before its window instead of the whole file.
#[derive(Debug)]
pub(super) struct StreamIndex {
    pub(super) stamp: octocode_engine::stamp::SourceStamp,
    pub(super) digest: String,
    pub(super) total_lines: usize,
    pub(super) binary: bool,
    pub(super) checkpoints: Vec<Checkpoint>,
}

/// Indexes of streamed sources by canonical path. The stamp (size, times,
/// inode) recorded only for a file untouched for
/// [`octocode_engine::stamp::SourceStamp::SETTLE`] is the proof that the
/// bytes, and so the digest, are unchanged, as the graph and LSP memos use
/// it. A miss costs a whole-file pass, so the byte weight bounds the memo
/// and the entry cap sits far above a working set of large sources (an
/// index weighs about 80 KB per 10 million lines).
static INDEXES: octocode_io::memo::Memo<std::path::PathBuf, StreamIndex> =
    octocode_io::memo::Memo::new(octocode_io::memo::MemoLimits {
        max_entries: 1024,
        max_weight: 8 * 1024 * 1024,
        keep_newest: false,
    });

/// Keep a byte page's range view for later pages of the source version
/// `stamp`.
fn remember_view(
    path: &Path,
    wants: &[Want],
    view: Option<RangeView>,
    stamp: octocode_engine::stamp::SourceStamp,
) {
    let (Some(want), Some(mut view)) = (byte_page(wants), view) else {
        return;
    };
    view.stamp = stamp;
    let weight = view.points.len() * (std::mem::size_of::<ViewPoint>() + MARKER_PREFIX_BYTES);
    RANGE_VIEWS.insert(
        (path.to_path_buf(), want.first, want.last),
        view,
        weight,
        None,
    );
}

/// The windows of `wants` from `source` at `path`: from the source's index
/// while `stamp` (the handle's settled stamp, taken before the read) still
/// matches it, else by one whole pass, which indexes a source whose stamp
/// held through the pass.
pub(super) fn read_windows(
    source: &std::fs::File,
    path: &Path,
    stamp: Option<octocode_engine::stamp::SourceStamp>,
    wants: &[Want],
    cancel: &impl CancellationCheck,
) -> Result<Vec<Streamed>, String> {
    let unchanged = |stamp: &octocode_engine::stamp::SourceStamp| {
        source
            .metadata()
            .ok()
            .as_ref()
            .and_then(crate::tools::source::cache_stamp)
            == Some(*stamp)
    };
    if let Some(stamp) = stamp
        && let Some(hit) = INDEXES.get(path)
        && hit.value.stamp == stamp
    {
        let view = byte_page(wants)
            .and_then(|want| RANGE_VIEWS.get(&(path.to_path_buf(), want.first, want.last)))
            .filter(|view| view.value.stamp == stamp);
        let (windows, learned) = stream_indexed(
            source,
            wants,
            cancel,
            &hit.value,
            view.as_ref().map(|view| &*view.value),
        )?;
        if learned.is_some() && unchanged(&stamp) {
            remember_view(path, wants, learned, stamp);
        }
        return Ok(windows);
    }
    let (windows, mut index, view) = stream(source, wants, cancel)?;
    if let Some(stamp) = stamp.filter(|stamp| unchanged(stamp)) {
        index.stamp = stamp;
        let weight =
            index.checkpoints.len() * std::mem::size_of::<Checkpoint>() + index.digest.len();
        INDEXES.insert(path.to_path_buf(), index, weight, None);
        remember_view(path, wants, view, stamp);
    }
    Ok(windows)
}
