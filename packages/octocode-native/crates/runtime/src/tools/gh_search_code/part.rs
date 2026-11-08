//! The part of one search page a response window holds: its rows are read in
//! rank order until the window is full, so a page that shows a few rows reads
//! a few files. The rest of the page is the next part: the same page's
//! continuation lists it from the same code-search answer ([`super::listed`]).
use super::code_output::{LinePlan, Resolution, shape_files};
use super::{CODE_ROW_ID, GhSearchCodeQuery, Withholds};
use crate::security::scan::ContentScan;
use crate::tools::gh_shared::{field_names, pick_rows};
use octocode_engine::json_size::utf16_units;
use octocode_github::{GitHubProvider, ProviderError, RequestContext};
use serde_json::{Value, json};

/// The chars a page's rows may fill: the row's share of the response page
/// less what the page states besides its rows, shown whole (`whole`) or as a
/// part that also carries the continuation to the rest of the page (`part`).
#[derive(Clone, Copy, Debug)]
pub(super) struct Room {
    pub(super) whole: usize,
    pub(super) part: usize,
}

/// The [`Room`] `window` leaves once `skeleton(continues)`, the chars the
/// page states besides its rows, is taken. `None` (every row of the page,
/// read at once) without a window, or on a classified page: its judge keeps
/// only some rows, so what the page shows is known only after every row is
/// read and judged.
pub(super) fn budget(
    query: &GhSearchCodeQuery,
    window: Option<usize>,
    skeleton: impl Fn(bool) -> usize,
) -> Option<Room> {
    if query.classification.is_some() || query.selection.is_some() {
        return None;
    }
    window.map(|window| Room {
        whole: window.saturating_sub(skeleton(false)),
        part: window.saturating_sub(skeleton(true)),
    })
}

/// Read the files of the leading rows of `items` that `room` holds, in rank
/// order, and return how many rows the page shows (at least one): every row
/// when all of them fit `room.whole`, else the rows that fit `room.part`.
///
/// The first row is read alone to measure it. Each later batch holds as many
/// rows as the room left holds at the largest row measured so far, and no
/// more than the rows already shown (so batches grow 1, 1, 2, 4, …): a batch
/// read past the window costs at most the rows shown, so a part reads at
/// most twice the rows it shows, and the reads of a batch go out together.
/// A row read past the window is not shown: the next part shows it.
pub(super) async fn read(
    plan: &mut LinePlan,
    items: &[Value],
    query: &GhSearchCodeQuery,
    withholds: Withholds<'_>,
    room: Option<Room>,
    provider: &GitHubProvider,
    context: &RequestContext,
    security: &(impl ContentScan + Sync),
) -> Result<usize, ProviderError> {
    let total = items.len();
    let Some(room) = room else {
        plan.read(provider, 0..total, context, security).await?;
        return Ok(total);
    };
    // `cut`: the rows a part shows, set once a row does not fit `room.part`;
    // reading then goes on only while the whole page may still fit.
    let (mut shown, mut used, mut largest, mut cut) = (0, 0, 0_usize, None);
    while shown < total {
        let limit = if cut.is_none() { room.part } else { room.whole };
        let take = match shown {
            0 => 1,
            _ => (limit.saturating_sub(used) / largest.max(1)).min(shown),
        };
        if take == 0 {
            if cut.is_none() {
                cut = Some(shown);
                continue;
            }
            break;
        }
        let end = shown.saturating_add(take).min(total);
        plan.read(provider, shown..end, context, security).await?;
        let resolution = plan.resolution(shown..end);
        for (row, hit) in items[shown..end].iter().zip(resolution.hits) {
            let chars = row_chars(
                row,
                hit,
                resolution.reference.clone(),
                query,
                withholds,
                shown == 0,
            );
            largest = largest.max(chars);
            if shown > 0 && used + chars > room.whole {
                return Ok(cut.unwrap_or(shown));
            }
            if shown > 0 && cut.is_none() && used + chars > room.part {
                cut = Some(shown);
            }
            used += chars;
            shown += 1;
        }
    }
    Ok(if shown == total {
        total
    } else {
        cut.unwrap_or(shown).max(1)
    })
}

/// The response chars `row` takes once shaped with `hit` as the page shapes
/// it: the row as its `fields` pick lists it, the read lead an unresolved
/// row adds and, on the page's `first` row, the page's top read lead.
fn row_chars(
    row: &Value,
    hit: (String, super::lines::FileHits),
    reference: Option<String>,
    query: &GhSearchCodeQuery,
    withholds: Withholds<'_>,
    first: bool,
) -> usize {
    let mut rows = vec![row.clone()];
    let resolution = Resolution {
        sha: None,
        reference,
        hits: vec![hit],
    };
    let reads = shape_files(&mut rows, query, withholds, Some(resolution));
    let mut page = json!({ "files": rows });
    pick_rows(&mut page, "files", CODE_ROW_ID, &field_names(&query.fields));
    let lead = |read: &Value| utf16_units(read).unwrap_or(0) + 1;
    let leads = reads.hits.iter().map(lead).sum::<usize>()
        + reads.top.as_ref().filter(|_| first).map_or(0, lead);
    utf16_units(&page["files"][0]).unwrap_or(0) + 1 + leads
}
