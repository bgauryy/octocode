//! The part of one search page a response window holds: its rows are read in
//! rank order until the window is full, so a page that shows a few rows reads
//! a few files. The rest of the page is the next part: the same page's
//! continuation lists it from the same code-search answer ([`super::listed`]).
use super::code_output::{LinePlan, Resolution, shape_rows, withheld_path};
use super::{CODE_ROW_ID, GhSearchCodeQuery, Withholds};
use octocode_engine::json_size::utf16_units;
use octocode_github::{GitHubProvider, ProviderError, RequestContext};
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::security::scan::ContentScan;
use octocode_tools_shared::gh_shared::{field_names, pick_rows};
use serde_json::{Map, Value, json};

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
/// Each batch holds as many rows as the room left holds at the largest row
/// measured so far, and no more than [`READ_AHEAD`] times the rows already
/// shown (the first, one more than that: a page shows at least one row). So
/// batches grow 3, 6, 18, …, the reads of a batch go out together, and a
/// batch read past the window costs at most [`READ_AHEAD`] times the rows
/// shown: a part reads at most `READ_AHEAD + 1` times the rows it shows, in
/// about `log3` round trips of the rows it reads.
/// A row read past the window is not shown: the next part shows it. A row's
/// size is what it adds to the published page; the first row's counts only
/// until a second is measured, since it alone adds the page's top read lead
/// and identity. Past the part's cut, reading goes on only while the rest of
/// the page, at the smallest row measured, may still fit whole.
/// How many rows a batch may read for each row already shown. A larger
/// factor reads a page in fewer round trips (each costs a provider round
/// trip of latency) and may read more files a small window does not show
/// (each costs one core-quota request, and the next part shows it unread).
const READ_AHEAD: usize = 2;

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
    // A symlink's target is judged as a row's own path is.
    let read_withholds = |path: &str| withheld_path(query, withholds, path);
    let Some(room) = room else {
        plan.read(provider, 0..total, context, security, &read_withholds)
            .await?;
        return Ok(total);
    };
    // `cut`: the rows a part shows, set once a row does not fit `room.part`;
    // reading then goes on only while the whole page may still fit.
    let (mut shown, mut used, mut cut) = (0, Used::default(), None);
    let (mut largest, mut smallest) = (0_usize, 0_usize);
    while shown < total {
        let left = |limit: usize| limit.saturating_sub(used.chars());
        let ahead = shown.saturating_mul(READ_AHEAD);
        let take = match cut {
            _ if shown == 0 => READ_AHEAD + 1,
            None => (left(room.part) / largest.max(1)).min(ahead),
            Some(_) if (total - shown).saturating_mul(smallest) > left(room.whole) => 0,
            Some(_) => (left(room.whole) / largest.max(1)).min(ahead),
        };
        if take == 0 {
            if cut.is_none() {
                cut = Some(shown);
                continue;
            }
            break;
        }
        let end = shown.saturating_add(take).min(total);
        plan.read(provider, shown..end, context, security, &read_withholds)
            .await?;
        let resolution = plan.resolution(shown..end);
        for (row, hit) in items[shown..end].iter().zip(resolution.hits) {
            let measured = row_chars(
                row,
                hit,
                resolution.reference.clone(),
                query,
                withholds,
                shown == 0,
            );
            let before = used.chars();
            let next = used.with(measured);
            let added = next.chars().saturating_sub(before);
            (largest, smallest) = if shown <= 1 {
                (added, added)
            } else {
                (largest.max(added), smallest.min(added))
            };
            if shown > 0 && next.chars() > room.whole {
                return Ok(cut.unwrap_or(shown));
            }
            if shown > 0 && cut.is_none() && next.chars() > room.part {
                cut = Some(shown);
            }
            used = next;
            shown += 1;
        }
    }
    Ok(if shown == total {
        total
    } else {
        cut.unwrap_or(shown).max(1)
    })
}

/// The chars one shaped row takes: `full` as the row states its own
/// identity, `moved` once the page states it, and the page-level fields a
/// page of this row alone would state
/// ([`octocode_runtime_base::response::hoist::apply_on`]).
struct Measured {
    full: usize,
    moved: usize,
    stated: Map<String, Value>,
}

/// The chars the rows shown so far take as the response stage publishes
/// them: while every row would state the same page identity, the page states
/// it once and each row takes its `moved` size; else each keeps its own. It
/// never counts less than the published rows take, so a part never overflows
/// its window.
#[derive(Default)]
struct Used {
    full: usize,
    moved: usize,
    /// The first row's page-level fields and the chars they take on the page.
    stated: Option<(Map<String, Value>, usize)>,
    shared: bool,
}

impl Used {
    fn with(self, row: Measured) -> Self {
        let (stated, shared) = match self.stated {
            None => {
                let once = if row.stated.is_empty() {
                    0
                } else {
                    // `{…}` less its braces, plus the comma that joins it.
                    utf16_units(&Value::Object(row.stated.clone()))
                        .unwrap_or(0)
                        .saturating_sub(1)
                };
                (Some((row.stated, once)), true)
            }
            Some((first, once)) => {
                let shared = self.shared && row.stated == first;
                (Some((first, once)), shared)
            }
        };
        Self {
            full: self.full + row.full,
            moved: self.moved + row.moved,
            stated,
            shared,
        }
    }

    fn chars(&self) -> usize {
        match &self.stated {
            Some((_, once)) if self.shared => self.moved + once,
            _ => self.full,
        }
    }
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
) -> Measured {
    let mut rows = vec![row.clone()];
    let resolution = Resolution {
        sha: None,
        reference,
        hits: vec![hit],
    };
    // Only the page's first row adds the page's top read lead.
    let reads = shape_rows(&mut rows, query, withholds, Some(resolution), first);
    let mut page = json!({ "files": rows });
    pick_rows(&mut page, "files", CODE_ROW_ID, &field_names(&query.fields));
    let lead = |read: &Value| utf16_units(read).unwrap_or(0) + 1;
    let leads = reads.hits.iter().map(lead).sum::<usize>() + reads.top.as_ref().map_or(0, lead);
    let full = utf16_units(&page["files"][0]).unwrap_or(0) + 1 + leads;
    let mut data = Map::new();
    data.insert("files".into(), page["files"].take());
    octocode_runtime_base::response::hoist::apply_on(ToolId::GhSearchCode, &mut data);
    let moved = utf16_units(&data["files"][0]).unwrap_or(0) + 1 + leads;
    data.remove("files");
    Measured {
        full,
        moved,
        stated: data,
    }
}
