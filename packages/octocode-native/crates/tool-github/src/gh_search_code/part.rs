//! The part of one search page a response window holds: its rows are read in
//! rank order until the window is full, so a page that shows a few rows reads
//! a few files. The rest of the page is the next part: the same page's
//! continuation lists it from the same code-search answer ([`super::listed`]).
use super::code_output::{LinePlan, Resolution, shape_rows, withheld_path};
use super::cut::{Cut, Unlisted};
use super::leads::unlisted_read;
use super::lines::{FileHits, Window};
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

/// How many rows a batch may read for each row already shown. A larger
/// factor reads a page in fewer round trips (each costs a provider round
/// trip of latency) and may read more files a small window does not show
/// (each costs one core-quota request, and the next part shows it unread).
const READ_AHEAD: usize = 2;

/// Read the files of the leading rows of `items` that `room` holds, in rank
/// order, and return how many rows the page shows (at least one) and the
/// run of hits each lists ([`Shown`]): every row when all of them fit
/// `room.whole`, else the rows that fit `room.part`. A row fits at its
/// smallest cut ([`Runs`]), so a file with many hit lines pushes no row to a
/// later part while every row's smallest run fits; [`share`] then lists
/// each row as fully as the room allows.
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
pub(super) async fn read(
    plan: &mut LinePlan,
    items: &[Value],
    query: &GhSearchCodeQuery,
    withholds: Withholds<'_>,
    room: Option<Room>,
    provider: &GitHubProvider,
    context: &RequestContext,
    security: &(impl ContentScan + Sync),
) -> Result<Shown, ProviderError> {
    let total = items.len();
    // A symlink's target is judged as a row's own path is.
    let read_withholds = |path: &str| withheld_path(query, withholds, path);
    let Some(room) = room else {
        plan.read(provider, 0..total, context, security, &read_withholds)
            .await?;
        return Ok(Shown {
            rows: total,
            cuts: Vec::new(),
        });
    };
    let window = plan.window();
    // `cut`: the rows a part shows, set once a row does not fit `room.part`;
    // reading then goes on only while the whole page may still fit. Rows
    // fit at their smallest cut ([`Runs`]); [`share`] then lists each as
    // fully as the room left allows.
    let (mut shown, mut used, mut cut) = (0, Used::default(), None);
    let mut measured: Vec<(Measured, Runs)> = Vec::new();
    let (mut largest, mut smallest) = (0_usize, 0_usize);
    while shown < total {
        let left = |limit: usize| limit.saturating_sub(used.fitted());
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
            let runs = Runs::of(row, &hit, query, window);
            let measured_row = row_chars(
                row,
                hit,
                resolution.reference.clone(),
                query,
                withholds,
                shown == 0,
            );
            let before = used.fitted();
            let next = used.clone().with(measured_row.clone(), runs.floor());
            let added = next.fitted().saturating_sub(before);
            (largest, smallest) = if shown <= 1 {
                (added, added)
            } else {
                (largest.max(added), smallest.min(added))
            };
            if shown > 0 && next.fitted() > room.whole {
                return Ok(share(&measured, cut.unwrap_or(shown), room, total));
            }
            if shown > 0 && cut.is_none() && next.fitted() > room.part {
                cut = Some(shown);
            }
            used = next;
            measured.push((measured_row, runs));
            shown += 1;
        }
    }
    let rows = if shown == total {
        total
    } else {
        cut.unwrap_or(shown).max(1)
    };
    Ok(share(&measured, rows, room, total))
}

/// The rows a page shows, and the run each lists ([`share`]).
pub(super) struct Shown {
    pub(super) rows: usize,
    /// Per shown row, its cut; `None` (or past the end): listed whole.
    pub(super) cuts: Vec<Option<Cut>>,
}

/// The first `rows` of the `measured` rows, each listed as fully as the
/// room allows: the room of a whole page when they are all `total` rows,
/// else of a part. No row is cut while every row fits whole; else each
/// row is listed up to one level of chars (a row that fits under it whole,
/// a larger row cut to the most hits that fit it), the highest level whose
/// rows fit the room, so the largest rows are cut and every row keeps its
/// share. A row cut below its smallest run keeps that run.
fn share(measured: &[(Measured, Runs)], rows: usize, room: Room, total: usize) -> Shown {
    // Only the shown rows share the room; a row measured past them is the
    // next part's, listed there.
    let measured = &measured[..rows.min(measured.len())];
    let limit = if rows == total { room.whole } else { room.part };
    let chars = measured
        .iter()
        .fold(Used::default(), |used, (row, _)| used.with(row.clone(), 0))
        .chars();
    let whole = || Shown {
        rows,
        cuts: Vec::new(),
    };
    if chars <= limit {
        return whole();
    }
    // A page with a cut row also says it is partial.
    let limit = limit.saturating_sub(cut_page_chars());
    let at = |level: usize| {
        let saved = measured
            .iter()
            .map(|(row, runs)| runs.pick(row.full, level).0)
            .sum::<usize>();
        chars.saturating_sub(saved)
    };
    let (mut low, mut high) = (
        0,
        measured.iter().map(|(row, _)| row.full).max().unwrap_or(0),
    );
    while low < high {
        let mid = low + (high - low).div_ceil(2);
        if at(mid) <= limit {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    let cuts = measured
        .iter()
        .map(|(row, runs)| runs.pick(row.full, low).1)
        .collect::<Vec<_>>();
    if cuts.iter().all(Option::is_none) {
        return whole();
    }
    Shown { rows, cuts }
}

/// What a page with a cut row states besides its rows: that it is partial
/// for the hit lines it leaves to reads (`hitLinesNotListed`), the most
/// [`super::page_facts::disclose_completeness`] adds for them.
fn cut_page_chars() -> usize {
    let mut page = json!({});
    octocode_runtime_base::response::paging::mark_partial(
        &mut page,
        octocode_runtime_base::response::paging::PartialReason::HitLinesNotListed,
        &[],
    );
    utf16_units(&page).unwrap_or(0)
}

/// What listing a measured row cut to `count` of its hits saves, for each
/// count it can be cut to: the chars its unlisted lines take, less what the
/// cut adds (its `matchCount` and its `readHits` read, measured at the
/// widest line ranges a read of its file can name). Only a cut that saves
/// chars is kept: a row whose read costs more than its unlisted lines is
/// listed whole.
struct Runs {
    /// `(chars saved, cut)`, by the hits listed, ascending.
    cuts: Vec<(usize, Cut)>,
}

impl Runs {
    fn of(
        row: &Value,
        (sha, hits): &(String, FileHits),
        query: &GhSearchCodeQuery,
        window: Window,
    ) -> Self {
        let none = Self { cuts: Vec::new() };
        let FileHits::Lines {
            lines,
            matched,
            best,
            total,
            line_count,
            ..
        } = hits
        else {
            return none;
        };
        let listed =
            query.fields.is_empty() || query.fields.iter().any(|field| field.as_str() == "matches");
        let Some(row) = row.as_object().filter(|_| listed && matched.len() > 1) else {
            return none;
        };
        let count = u32::try_from(*line_count).unwrap_or(u32::MAX);
        let Some(lead) = unlisted_read(query, row, sha, &Unlisted::widest(count)) else {
            return none;
        };
        let added = utf16_units(&lead).unwrap_or(usize::MAX / 4)
            + READ_KEY_CHARS
            + READ_ENTRY_CHARS
            + if query
                .fields
                .iter()
                .any(|field| field.as_str() == "matchCount")
            {
                0
            } else {
                MATCH_COUNT_CHARS + total.to_string().len()
            };
        // Each listed entry, and each hit's number when `matchedLines` lists them.
        let entries = prefix(lines.iter().map(|line| utf16_units(line).unwrap_or(0) + 1));
        let numbered_hits = lines.len() > matched.len();
        let hit_numbers = prefix(matched.iter().map(|hit| {
            if numbered_hits {
                hit.to_string().len() + 1
            } else {
                0
            }
        }));
        let (all_entries, all_hits) = (entries[lines.len()], hit_numbers[matched.len()]);
        let cuts = (1..matched.len())
            .filter_map(|listed| {
                let cut = Cut::around(matched, *best, listed);
                let range = cut.listed(lines, matched, window, count);
                let kept = (entries[range.end] - entries[range.start])
                    + (hit_numbers[cut.end] - hit_numbers[cut.start]);
                let saved = (all_entries + all_hits)
                    .saturating_sub(kept)
                    .checked_sub(added)?;
                (saved > 0).then_some((saved, cut))
            })
            .collect();
        Self { cuts }
    }

    /// The most a cut saves: what the row takes at its smallest run.
    fn floor(&self) -> usize {
        self.cuts.iter().map(|&(saved, _)| saved).max().unwrap_or(0)
    }

    /// The chars saved and the cut of a row of `full` chars listed up to
    /// `level`: whole when it fits, else its largest run that fits, else its
    /// smallest.
    fn pick(&self, full: usize, level: usize) -> (usize, Option<Cut>) {
        if full <= level || self.cuts.is_empty() {
            return (0, None);
        }
        self.cuts
            .iter()
            .rev()
            .find(|&&(saved, _)| full.saturating_sub(saved) <= level)
            .or_else(|| self.cuts.iter().max_by_key(|&&(saved, _)| saved))
            .map_or((0, None), |&(saved, cut)| (saved, Some(cut)))
    }
}

/// `"matchCount":` and the comma before it: what a cut row adds besides its
/// count's digits.
const MATCH_COUNT_CHARS: usize = r#","matchCount":"#.len();

/// What a `readHits` read adds besides its call and key: the colon after
/// the key, the comma before it, and the page's `next` object it may open.
const READ_ENTRY_CHARS: usize = r#","next":{:}"#.len();

/// The longest key a page's reads take: `readHits`, `readHits2`, … one per
/// row, and a page holds at most the contract's `limit` maximum of rows.
const READ_KEY_CHARS: usize = r#""readHits"#.len()
    + octocode_runtime_base::contracts::query_limits::gh_search_code::LIMIT_MAXIMUM.ilog10()
        as usize
    + 1
    + 1;

/// Running sums of `costs`, from 0: `sums[i]` is the cost of the first `i`.
fn prefix(costs: impl Iterator<Item = usize>) -> Vec<usize> {
    std::iter::once(0)
        .chain(costs.scan(0_usize, |sum, cost| {
            *sum += cost;
            Some(*sum)
        }))
        .collect()
}

/// The chars one shaped row takes: `full` as the row states its own
/// identity, `moved` once the page states it, and the page-level fields a
/// page of this row alone would state
/// ([`octocode_runtime_base::response::hoist::apply_on`]).
#[derive(Clone)]
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
#[derive(Clone, Default)]
struct Used {
    full: usize,
    moved: usize,
    /// What the rows' smallest cuts save ([`Runs::floor`]).
    saved: usize,
    /// The first row's page-level fields and the chars they take on the page.
    stated: Option<(Map<String, Value>, usize)>,
    shared: bool,
}

impl Used {
    fn with(self, row: Measured, saved: usize) -> Self {
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
            saved: self.saved + saved,
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

    /// The chars the rows take, each at its smallest cut, with what a page
    /// of cut rows states besides them ([`cut_page_chars`]).
    fn fitted(&self) -> usize {
        match self.saved {
            0 => self.chars(),
            saved => self.chars().saturating_sub(saved) + cut_page_chars(),
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
        unlisted: Vec::new(),
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
