//! One window over a file page's patch stream: where it ends and the
//! rows it shows.
use crate::response::result::remove_nulls;
use crate::tools::gh_get_history_item::inventory::{file_metadata, missing_patch_reason};
use crate::tools::gh_get_history_item::util::str_at;
use serde_json::{Value, json};

/// One window over a file page's patch stream.
pub(super) struct PatchStream<'a> {
    pub(super) views: &'a [Option<String>],
    /// The earlier file each repeated view names (`samePatchAs`).
    pub(super) same_as: &'a [Option<usize>],
    /// Whole-patch sizes of `matchString` views narrowed to hit hunks.
    pub(super) narrowed: &'a [Option<usize>],
    /// Hit-line starts of each narrowed view (`None`: not narrowed).
    pub(super) hits: &'a [Option<Vec<usize>>],
    /// New-side windows each view's context could not reach.
    pub(super) clipped: &'a [Vec<String>],
    pub(super) starts: &'a [usize],
    pub(super) lengths: &'a [usize],
    pub(super) offset: usize,
    pub(super) end: usize,
}

impl PatchStream<'_> {
    /// The rows this window shows, the files it leaves unfinished, and the
    /// clipped files it starts.
    #[expect(
        clippy::type_complexity,
        reason = "one caller destructures the three parts of a window together"
    )]
    pub(super) fn rows(
        &self,
        files: &[Value],
    ) -> (Vec<Value>, Vec<String>, Vec<(String, Vec<String>)>) {
        let (offset, end) = (self.offset, self.end);
        let first_window = offset == 0;
        let mut rows = Vec::new();
        let mut unfinished = Vec::new();
        let mut clipped = Vec::new();
        for (i, (file, view)) in files.iter().zip(self.views).enumerate() {
            let (start, len) = (self.starts[i], self.lengths[i]);
            let mut row = file_metadata(file);
            match view {
                // Binary or oversized files have no patch: nothing to window, so
                // they are reported once, on the first window.
                None => {
                    if !first_window {
                        continue;
                    }
                    match missing_patch_reason(file) {
                        // A pure rename changes no content: its diff is empty.
                        None => row["patch"] = json!(""),
                        Some(reason) => {
                            row["isPartial"] = json!(true);
                            row["terminalLimit"] = json!(true);
                            row["patchUnavailable"] = json!(reason);
                        }
                    }
                }
                Some(_) if len == 0 => match self.same_as[i] {
                    // Lossless dedupe: the earlier row of this page holds
                    // the identical patch text. The repeat is listed on the
                    // window that finishes delivering it, so it only ever
                    // names text the caller already has.
                    Some(earlier) => {
                        let delivered = self.starts[earlier] + self.lengths[earlier];
                        if delivered <= offset || delivered > end {
                            continue;
                        }
                        row["samePatchAs"] =
                            json!(str_at(&files[earlier], "/filename").unwrap_or(""));
                    }
                    None if first_window => row["patch"] = json!(""),
                    None => continue,
                },
                Some(view) => {
                    if start + len <= offset {
                        continue; // delivered by an earlier window
                    }
                    let local_start = offset.saturating_sub(start);
                    let local_end = end.saturating_sub(start).min(len);
                    if local_end < len {
                        unfinished.push(str_at(file, "/filename").unwrap_or("").to_owned());
                    }
                    // Files not reached yet are counted (`unfinished`), not
                    // shown as empty placeholder rows.
                    if local_end <= local_start {
                        continue;
                    }
                    if let Some(full) = self.narrowed[i] {
                        row["fullPatchChars"] = json!(full);
                    }
                    // Hits this window shows of the file, so "every hit
                    // shown" is checkable against the rows.
                    if let Some(hits) = &self.hits[i] {
                        let shown = hits
                            .iter()
                            .filter(|&&at| at >= local_start && at < local_end)
                            .count();
                        row["matchCount"] = json!(shown);
                    }
                    if local_start == 0 && !self.clipped[i].is_empty() {
                        row["contextClipped"] = json!(true);
                        clipped.push((
                            str_at(file, "/filename").unwrap_or("").to_owned(),
                            self.clipped[i].clone(),
                        ));
                    }
                    let text = view
                        .chars()
                        .skip(local_start)
                        .take(local_end - local_start)
                        .collect::<String>();
                    row["patch"] = json!(text);
                    let cut = local_end < len;
                    if cut || local_start > 0 {
                        // Per-file coordinates only, without a window's
                        // `nextOffset`: the cursor `offset` continues is the
                        // page stream's (`PatchPage::cursor`), never a file's.
                        row["patchPagination"] = crate::response::pages::SpanFacts::local(
                            "chars",
                            local_start,
                            local_end - local_start,
                            len,
                        )
                        .to_value();
                    }
                }
            }
            remove_nulls(&mut row);
            rows.push(row);
        }
        (rows, unfinished, clipped)
    }
}

/// Share of a window (`num/den`) a whole-file boundary must fill before the
/// window ends there; below it the window packs on into the next file up
/// to its last line end, so a walk does not ship windows mostly empty.
const PACK_SHARE: (usize, usize) = (3, 4);

/// Where a window that starts at stream position `offset` ends: the last
/// file end it holds whole, unless the next file is larger than a whole
/// window (it would never fit, so it starts at once) or that end fills less
/// than [`PACK_SHARE`] of the window (the window packs on into the next
/// file). A cut inside a file ends after the last line end the window
/// holds; a packed window whose next file has no line end in reach keeps
/// the file boundary.
pub(super) fn window_end(
    views: &[Option<String>],
    starts: &[usize],
    lengths: &[usize],
    offset: usize,
    window: usize,
) -> usize {
    let total = lengths.iter().sum::<usize>();
    let stream_end = offset.saturating_add(window).min(total);
    if stream_end == total {
        return total;
    }
    let boundary = (0..lengths.len())
        .map(|i| starts[i] + lengths[i])
        .filter(|&file_end| file_end > offset && file_end <= stream_end)
        .max();
    let next_overflows = |at: usize| {
        (0..lengths.len())
            .find(|&i| starts[i] == at && lengths[i] > 0)
            .is_some_and(|i| lengths[i] > window)
    };
    let (num, den) = PACK_SHARE;
    let filled = |at: usize| (at - offset) * den >= window * num;
    if let Some(at) = boundary.filter(|&at| !next_overflows(at) && filled(at)) {
        return at;
    }
    let Some(cut) =
        (0..lengths.len()).find(|&i| starts[i] < stream_end && starts[i] + lengths[i] > stream_end)
    else {
        return boundary.unwrap_or(stream_end);
    };
    let from = offset.max(starts[cut]) - starts[cut];
    let to = stream_end - starts[cut];
    let line_end = views[cut]
        .as_deref()
        .and_then(|view| {
            view.chars()
                .take(to)
                .enumerate()
                .skip(from)
                .filter(|(_, c)| *c == '\n')
                .last()
        })
        .map(|(at, _)| starts[cut] + at + 1);
    match (line_end, boundary) {
        (Some(at), _) => at,
        // Packing found no line end past the boundary: end on the file.
        (None, Some(at)) if !next_overflows(at) => at,
        (None, _) => stream_end,
    }
}
