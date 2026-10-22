//! Line-anchored hits for repo-scoped code searches. GitHub's code index
//! returns at most a few unnumbered fragments per file, so every file of the
//! page is read (core API quota, through the contents cache) and its keyword
//! lines are listed as `"<line>\t<text>"` (the numbered-line form), the way
//! `grep -n` would: with several keywords, the lines holding all of them.
use crate::num::u32_of_usize;
use crate::security::scan::ContentScan;
use octocode_engine::graph::{GraphFactDeclaration, GraphFactsDocument};
use octocode_github::{GitHubProvider, ProviderError, ProviderErrorKind, RequestContext};
use std::path::Path;

/// Hit lines listed per file; a file with more states `matchCount`, and its
/// `next.readHits` row reads every keyword line. 20 → 5 (2026-10-09): one
/// repo-scoped search listed up to 20 lines per file (8.9 KB for 20 files)
/// where a shell search returns paths; five lines show what a file
/// holds, and its read returns the rest.
pub(super) const MAX_LINES_PER_FILE: usize = 5;
/// Characters kept per hit line (a window around the first keyword).
const MAX_LINE_CHARS: usize = 200;
/// Blobs larger than this keep their index fragments.
const MAX_SCAN_BYTES: usize = 2 * 1024 * 1024;

/// One file's hits at a resolved commit.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum FileHits {
    /// The lines holding every keyword, else each keyword's first line
    /// (capped at [`MAX_LINES_PER_FILE`]); `total` counts every keyword line.
    Lines {
        lines: Vec<String>,
        /// A shown line was cut to its keyword window.
        clipped: bool,
        first: u32,
        last: u32,
        /// The hit that best fits the goal ([`hit_score`]).
        best: u32,
        total: usize,
        line_count: usize,
        /// The best hit's line, verbatim and trimmed, when it declares a
        /// block (a function, class, ...) and no other line of the file
        /// holds it: a `matchString` + `block` read returns that whole
        /// declaration.
        declaration: Option<String>,
        /// The last shown line, when the listing is exactly the file's
        /// first keyword lines with none cut: a read of the lines after it
        /// reaches every unshown hit once. `None` when a hit line was
        /// skipped or cut, so only a whole re-read reaches it.
        resume_after: Option<u32>,
    },
    /// The blob was read but no line holds a keyword (index lag or
    /// tokenization differences): keep the index fragments.
    Unmatched,
    /// The path does not exist at the resolved ref.
    Missing,
    /// The blob could not be read or scanned (binary, too large, transient).
    Unavailable,
}

/// Read each path at `sha` concurrently and list its keyword lines, the
/// read blobs scanned on parallel threads ([`scan_all`]). Cancellation
/// and deadline expiry abort; other read failures leave the file `Unavailable`.
pub(super) async fn resolve_files(
    provider: &GitHubProvider,
    owner: &str,
    repo: &str,
    sha: &str,
    paths: &[String],
    keywords: &[String],
    goal: &str,
    context: &RequestContext,
    security: &(impl ContentScan + Sync),
) -> Result<Vec<FileHits>, ProviderError> {
    context.check()?;
    let reads = paths.iter().map(|path| async move {
        provider
            .get_file_content(
                &octocode_github::ContentRequest {
                    owner: owner.to_owned(),
                    repo: repo.to_owned(),
                    path: path.clone(),
                    reference: Some(sha.to_owned()),
                    force_refresh: false,
                },
                context,
            )
            .await
    });
    let fetched = futures_util::future::join_all(reads).await;
    context.check()?;
    let mut out = Vec::with_capacity(paths.len());
    let mut blobs = Vec::new();
    for (path, result) in paths.iter().zip(&fetched) {
        out.push(match result {
            Ok(content) => {
                blobs.push((out.len(), content.bytes.as_ref(), path.as_str()));
                FileHits::Unavailable
            }
            Err(error) if error.is_cancelled_or_timed_out() => {
                return Err(error.clone());
            }
            Err(error)
                if error.kind == ProviderErrorKind::NotFound || error.status == Some(404) =>
            {
                FileHits::Missing
            }
            Err(_) => FileHits::Unavailable,
        });
    }
    let scanned = scan_all(
        &blobs
            .iter()
            .map(|&(_, bytes, path)| (bytes, path))
            .collect::<Vec<_>>(),
        keywords,
        goal,
        security,
        context,
    )?;
    for ((at, _, _), hits) in blobs.into_iter().zip(scanned) {
        out[at] = hits;
    }
    Ok(out)
}

/// [`scan`] of each `(bytes, path)`, in order, on scoped threads within the
/// process's scan budget ([`SCANS`]). A scan that panics leaves its file
/// `Unavailable` (its index fragments stay) rather than failing the row.
fn scan_all(
    files: &[(&[u8], &str)],
    keywords: &[String],
    goal: &str,
    security: &(impl ContentScan + Sync),
    context: &RequestContext,
) -> Result<Vec<FileHits>, ProviderError> {
    let cores = crate::tools::workers::host_cores();
    Ok(fan_out(
        &SCANS,
        context,
        cores,
        files,
        |&(bytes, _)| scan_weight(bytes),
        |&(bytes, path)| scan(bytes, path, keywords, goal, security),
    )?
    .into_iter()
    .map(|hits| hits.unwrap_or(FileHits::Unavailable))
    .collect())
}

/// Blob bytes the process scans at once. A scanned file's working set (its
/// lines, their lowercase copies and its declarations parse) runs to about
/// 35 times its bytes, so this keeps concurrent scans near 300 MB whatever
/// the core count, page size or number of concurrent calls, while small
/// files still scan on every core.
const SCAN_BUDGET_BYTES: usize = 8 * 1024 * 1024;

/// The process's in-flight scan bytes: every row, and every call a
/// long-lived server runs at once, shares it.
static SCANS: ScanBudget = ScanBudget::new(SCAN_BUDGET_BYTES);

/// The bytes [`scan`] holds for a blob: none for one it refuses unread.
fn scan_weight(bytes: &[u8]) -> usize {
    if bytes.len() > MAX_SCAN_BYTES {
        0
    } else {
        bytes.len()
    }
}

/// Bytes held by running scans, at most `limit`.
struct ScanBudget {
    limit: usize,
    held: std::sync::Mutex<usize>,
    freed: std::sync::Condvar,
    #[cfg(test)]
    waiting: std::sync::atomic::AtomicUsize,
}

impl ScanBudget {
    const fn new(limit: usize) -> Self {
        Self {
            limit,
            held: std::sync::Mutex::new(0),
            freed: std::sync::Condvar::new(),
            #[cfg(test)]
            waiting: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Hold `bytes` (at most the whole budget: a larger item runs alone)
    /// until the guard drops, waiting while other scans hold too much.
    fn admit(&self, bytes: usize, context: &RequestContext) -> Result<Admitted<'_>, ProviderError> {
        context.check()?;
        let bytes = bytes.min(self.limit);
        let mut held = self
            .held
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while bytes > self.limit - *held {
            context.check()?;
            let wait = context
                .budget
                .effective_deadline()
                .saturating_duration_since(std::time::Instant::now())
                .min(std::time::Duration::from_millis(10));
            #[cfg(test)]
            self.waiting
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            held = self
                .freed
                .wait_timeout(held, wait)
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0;
            #[cfg(test)]
            self.waiting
                .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        }
        context.check()?;
        *held += bytes;
        Ok(Admitted {
            budget: self,
            bytes,
        })
    }

    /// Workers for `items` items, the largest weighing `largest`: one per
    /// core, never more than the items, nor than the budget admits at once.
    fn workers(&self, cores: usize, items: usize, largest: usize) -> usize {
        cores.min(items).min(self.limit / largest.max(1)).max(1)
    }
}

/// A scan's hold on its [`ScanBudget`] bytes, released on drop (a
/// panicking scan's too).
struct Admitted<'a> {
    budget: &'a ScanBudget,
    bytes: usize,
}

impl Drop for Admitted<'_> {
    fn drop(&mut self) {
        let mut held = self
            .budget
            .held
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *held -= self.bytes;
        drop(held);
        self.budget.freed.notify_all();
    }
}

/// `work` on each item, in order, each holding `weight(item)` bytes of
/// `budget` while it runs; several items run on scoped threads (at most
/// [`ScanBudget::workers`]) that each take the next item until none is
/// left. An item whose `work` panicked is `None`; the others keep their
/// results, on one thread or many.
fn fan_out<I: Sync, T: Send>(
    budget: &ScanBudget,
    context: &RequestContext,
    cores: usize,
    items: &[I],
    weight: impl Fn(&I) -> usize + Sync,
    work: impl Fn(&I) -> T + Sync,
) -> Result<Vec<Option<T>>, ProviderError> {
    context.check()?;
    let run = |item: &I| {
        context.check()?;
        let _held = budget.admit(weight(item), context)?;
        context.check()?;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(item))).ok();
        context.check()?;
        Ok(result)
    };
    if items.len() < 2 {
        return items.iter().map(run).collect();
    }
    let largest = items.iter().map(&weight).max().unwrap_or(0);
    let workers = budget.workers(cores, items.len(), largest);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let mut out: Vec<Option<T>> = items.iter().map(|_| None).collect();
    std::thread::scope(|scope| {
        let tasks: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        context.check()?;
                        let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(item) = items.get(index) else {
                            return Ok::<_, ProviderError>(done);
                        };
                        done.push((index, run(item)?));
                    }
                })
            })
            .collect();
        for task in tasks {
            let done = match task.join() {
                Ok(done) => done?,
                Err(panic) => std::panic::resume_unwind(panic),
            };
            for (index, value) in done {
                out[index] = value;
            }
        }
        context.check()?;
        Ok(out)
    })
}

/// Case-insensitive keyword lines of one blob, numbered from 1.
pub(super) fn scan(
    bytes: &[u8],
    path: &str,
    keywords: &[String],
    goal: &str,
    security: &impl ContentScan,
) -> FileHits {
    if bytes.len() > MAX_SCAN_BYTES || bytes.iter().take(8192).any(|byte| *byte == 0) {
        return FileHits::Unavailable;
    }
    let text = String::from_utf8_lossy(bytes);
    let needles: Vec<String> = keywords
        .iter()
        .map(|keyword| keyword.trim().to_lowercase())
        .filter(|keyword| !keyword.is_empty())
        .collect();
    if needles.is_empty() {
        return FileHits::Unmatched;
    }
    let mut blob = Blob {
        raw: text.lines().collect(),
        lowered: text.lines().map(str::to_lowercase).collect(),
        key_blocks: Vec::new(),
        declared: Vec::new(),
        path,
    };
    let found = blob.found(&needles);
    if found.is_empty() {
        return FileHits::Unmatched;
    }
    // Only a file with a keyword line is outlined.
    blob.key_blocks = crate::security::private_key_block_line_ranges(&text);
    let outline = crate::tools::declarations_cache::file_document(&text, path);
    blob.declared = hit_declarations(outline.as_deref(), &found);
    let hits = blob.hits(found, &needles, goal);
    let anchor = blob.anchor(&hits, &needles, security);
    let listing = blob.listing(&hits, &needles, anchor.window, security);
    FileHits::Lines {
        declaration: blob.declaration(&hits, anchor.best, security),
        lines: listing.lines,
        clipped: listing.clipped,
        first: anchor.first,
        last: anchor.last,
        best: anchor.best,
        total: hits.len(),
        line_count: blob.lowered.len(),
        resume_after: listing.resume_after,
    }
}

/// One line holding a keyword.
struct Hit {
    /// Its 0-based line index.
    index: usize,
    /// The byte of its first keyword in the lowercase line.
    at: usize,
    /// It holds every keyword.
    every: bool,
    /// How well it fits the goal ([`hit_score`]).
    score: (usize, usize),
}

impl Hit {
    /// Its 1-based line number.
    fn number(&self) -> u32 {
        u32_of_usize(self.index + 1)
    }
}

/// Where a read anchors: the first and last anchoring lines and the best
/// hit, as line numbers (0: none), and the covering window (hit indexes)
/// when no line holds every keyword.
struct Anchor {
    window: Option<(usize, usize)>,
    first: u32,
    last: u32,
    best: u32,
}

/// The listed hit lines of a file.
struct Listing {
    lines: Vec<String>,
    clipped: bool,
    resume_after: Option<u32>,
}

/// One scanned blob: its lines, as read and lowercased, the private-key
/// blocks no listed line may show, and the declarations its grammar
/// outlines on its keyword lines ([`hit_declarations`]; none for a file
/// type without an outline), sorted by name line.
struct Blob<'a> {
    raw: Vec<&'a str>,
    lowered: Vec<String>,
    key_blocks: Vec<(u32, u32)>,
    declared: Vec<&'a GraphFactDeclaration>,
    path: &'a str,
}

/// The declarations of `outline` whose name line is a `found` keyword line
/// (the only lines a hit is scored or anchored on), by name line, outline
/// order within one line. They borrow the shared parse: the rest of the
/// outline is never copied.
fn hit_declarations<'a>(
    outline: Option<&'a GraphFactsDocument>,
    found: &[Found],
) -> Vec<&'a GraphFactDeclaration> {
    let mut declared: Vec<&GraphFactDeclaration> = outline
        .into_iter()
        .flat_map(|outline| &outline.declarations)
        .filter(|declared| {
            found
                .binary_search_by_key(&(name_line(declared) - 1), |&(index, _, _)| index)
                .is_ok()
        })
        .collect();
    declared.sort_by_key(|declared| name_line(declared));
    declared
}

/// A declaration's 1-based name line.
fn name_line(declared: &GraphFactDeclaration) -> usize {
    declared.selection_range.start.line as usize + 1
}

/// A keyword line before scoring: its 0-based index, the byte of its first
/// keyword in the lowercase line and that keyword's length.
type Found = (usize, usize, usize);

impl Blob<'_> {
    /// The declaration whose name the keyword at byte `at` of line `index`
    /// is: its name line is the hit line, and the name ends past `at`. A
    /// binary search finds the line's declarations, so scoring every hit
    /// costs no pass over the whole outline.
    fn declares(&self, index: usize, at: usize) -> Option<&GraphFactDeclaration> {
        let lower = &self.lowered[index];
        let from = self
            .declared
            .partition_point(|declared| name_line(declared) < index + 1);
        self.declared[from..]
            .iter()
            .copied()
            .take_while(|declared| name_line(declared) == index + 1)
            .find(|declared| {
                let name = declared.name.to_lowercase();
                !name.is_empty()
                    && lower
                        .find(&name)
                        .is_some_and(|start| start + name.len() > at)
            })
    }

    /// Every keyword line, in file order, unscored.
    fn found(&self, needles: &[String]) -> Vec<Found> {
        self.lowered
            .iter()
            .enumerate()
            .filter_map(|(index, lower)| {
                let (at, needle) = needles
                    .iter()
                    .filter_map(|needle| lower.find(needle.as_str()).map(|at| (at, needle)))
                    .min()?;
                Some((index, at, needle.len()))
            })
            .collect()
    }

    /// The `found` keyword lines, scored against the goal.
    fn hits(&self, found: Vec<Found>, needles: &[String], goal: &str) -> Vec<Hit> {
        let terms = goal_terms(goal, needles, &self.lowered);
        found
            .into_iter()
            .map(|(index, at, length)| Hit {
                index,
                at,
                every: needles
                    .iter()
                    .all(|needle| self.lowered[index].contains(needle.as_str())),
                score: hit_score(
                    &self.lowered,
                    index,
                    at + length,
                    self.declares(index, at).is_some(),
                    &terms,
                ),
            })
            .collect()
    }

    fn in_key_block(&self, number: u32) -> bool {
        self.key_blocks
            .iter()
            .any(|&(start, end)| (start..=end).contains(&number))
    }

    /// The read anchors where the listing does: on lines holding every
    /// keyword when any exist; else on the smallest window holding every
    /// keyword (one opened by a declaration first); else on any keyword line.
    fn anchor(&self, hits: &[Hit], needles: &[String], security: &impl ContentScan) -> Anchor {
        let any_every = hits.iter().any(|hit| hit.every);
        let window = (!any_every && needles.len() > 1)
            .then(|| {
                covering_window(hits, &self.lowered, needles, |start| {
                    self.declared_head(&hits[start], security).is_some()
                })
            })
            .flatten();
        if let Some((start, end)) = window {
            let first = hits[start].number();
            return Anchor {
                window,
                first,
                last: hits[end].number(),
                best: first,
            };
        }
        let (mut first, mut last, mut best) = (0, 0, (0, (0, 0)));
        for hit in hits.iter().filter(|hit| !any_every || hit.every) {
            let number = hit.number();
            if first == 0 || hit.score > best.1 {
                best = (number, hit.score);
            }
            if first == 0 {
                first = number;
            }
            last = number;
        }
        Anchor {
            window,
            first,
            last,
            best: best.0,
        }
    }

    /// Listed: the lines holding every keyword; without one, each keyword's
    /// first line and the anchoring window's first and last lines. The rest
    /// stay counted for the whole-file read.
    fn listing(
        &self,
        hits: &[Hit],
        needles: &[String],
        window: Option<(usize, usize)>,
        security: &impl ContentScan,
    ) -> Listing {
        let mut uncovered: Vec<&String> = if hits.iter().any(|hit| hit.every) {
            Vec::new()
        } else {
            needles.iter().collect()
        };
        let in_window = |hit: usize| window.is_some_and(|(start, end)| hit == start || hit == end);
        let mut listing = Listing {
            lines: Vec::new(),
            clipped: false,
            resume_after: None,
        };
        let (mut shown_hits, mut prefix, mut last_shown) = (0, true, 0);
        for (position, hit) in hits.iter().enumerate() {
            if listing.lines.len() >= MAX_LINES_PER_FILE {
                break;
            }
            let lower = &self.lowered[hit.index];
            let before = uncovered.len();
            uncovered.retain(|needle| !lower.contains(needle.as_str()));
            if !hit.every && uncovered.len() == before && !in_window(position) {
                continue;
            }
            let number = hit.number();
            prefix &= position == shown_hits;
            shown_hits += 1;
            last_shown = number;
            let line = self.raw[hit.index];
            let shown = if self.in_key_block(number) {
                crate::security::key_fragment_placeholder()
            } else {
                listing.clipped |= line.chars().count() > MAX_LINE_CHARS;
                let window = clip(
                    line,
                    octocode_engine::text::case_fold::Folded::of(line).char_index(hit.at),
                );
                security
                    .sanitize(&window, Path::new(self.path))
                    .map(|(clean, _)| clean)
                    .unwrap_or_else(|_| "[REDACTED]".to_owned())
            };
            listing.lines.push(format!(
                "{number}{}{shown}",
                crate::response::numbered::SEPARATOR
            ));
        }
        listing.resume_after = (prefix && !listing.clipped && last_shown > 0).then_some(last_shown);
        listing
    }

    /// The best hit's declaration line, unless it sits in a private-key block.
    fn declaration(&self, hits: &[Hit], best: u32, security: &impl ContentScan) -> Option<String> {
        let hit = hits.iter().find(|hit| hit.number() == best)?;
        if self.in_key_block(best) {
            return None;
        }
        self.declared_head(hit, security)
    }

    /// The hit line as a `matchString` when its keyword is the name of a
    /// block declaration the grammar outlines (a `matchString` + `block`
    /// read then returns the whole declaration), the line is short and
    /// unique in the file, and scanning leaves it unchanged.
    fn declared_head(&self, hit: &Hit, security: &impl ContentScan) -> Option<String> {
        self.declares(hit.index, hit.at)
            .filter(|declared| declared.range.end.line > declared.range.start.line)?;
        let head = self.raw[hit.index].trim();
        if head.is_empty() || head.chars().count() > MAX_LINE_CHARS {
            return None;
        }
        let needle = head.to_lowercase();
        if self
            .lowered
            .iter()
            .filter(|other| other.contains(&needle))
            .count()
            != 1
        {
            return None;
        }
        let (clean, _) = security.sanitize(head, Path::new(self.path)).ok()?;
        (clean == head).then_some(clean)
    }
}

/// Widest span (in lines) of a window that anchors a multi-keyword read.
const MAX_WINDOW_LINES: usize = 60;

/// The hits (`start..=end` indexes into `hits`) of the smallest window of at
/// most [`MAX_WINDOW_LINES`] lines holding every keyword, preferring one
/// opened by a declaration (`declares`), then the narrowest, then the
/// earliest. `None` when no such window exists.
fn covering_window(
    hits: &[Hit],
    lowered: &[String],
    needles: &[String],
    declares: impl Fn(usize) -> bool,
) -> Option<(usize, usize)> {
    // ((opened by no declaration, span, start), (start, end)).
    type Ranked = ((bool, usize, usize), (usize, usize));
    let mut chosen: Option<Ranked> = None;
    for start in 0..hits.len() {
        let mut missing: Vec<&String> = needles.iter().collect();
        for end in start..hits.len() {
            let span = hits[end].index - hits[start].index;
            if span > MAX_WINDOW_LINES {
                break;
            }
            missing.retain(|needle| !lowered[hits[end].index].contains(needle.as_str()));
            if missing.is_empty() {
                let rank = (!declares(start), span, start);
                if chosen.as_ref().is_none_or(|(best, _)| rank < *best) {
                    chosen = Some((rank, (start, end)));
                }
                break;
            }
        }
    }
    chosen.map(|(_, window)| window)
}

/// Lines on each side of a hit searched for the goal's other terms.
const GOAL_CONTEXT_LINES: usize = 8;

/// Goal words (three or more characters, lowercase) that are not part of a
/// searched keyword and are rare in this file: on at most one line in
/// [`COMMON_TERM_LINES`] (and at least one line allowed). A word on more
/// lines (`the`, `use`, a type every line names) cannot tell one hit's
/// neighborhood from another's.
fn goal_terms(goal: &str, needles: &[String], lowered: &[String]) -> Vec<String> {
    let mut terms: Vec<String> = goal
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.chars().count() >= 3)
        .filter(|word| !needles.iter().any(|needle| needle.contains(word)))
        .map(str::to_owned)
        .collect();
    terms.sort_unstable();
    terms.dedup();
    let common = (lowered.len() / COMMON_TERM_LINES).max(1);
    terms.retain(|term| {
        lowered
            .iter()
            .filter(|line| line.contains(term.as_str()))
            .count()
            <= common
    });
    terms
}

/// A goal term on more than one line in this many is too common to locate.
const COMMON_TERM_LINES: usize = 20;

/// How well hit line `index` (keyword ending at byte `after`; `declares`:
/// the keyword is a declared name) fits the goal, ordered by the hit's own
/// shape first (a literal value assigned to the keyword, `x = 512` or
/// `x: 512`, 3; any other assignment or declaration 2; other code 1; a
/// comment 0), then by the goal terms named within [`GOAL_CONTEXT_LINES`].
/// Shape leads because neighboring prose names goal terms near uses and
/// definitions alike.
fn hit_score(
    lowered: &[String],
    index: usize,
    after: usize,
    declares: bool,
    terms: &[String],
) -> (usize, usize) {
    let window = &lowered[index.saturating_sub(GOAL_CONTEXT_LINES)
        ..(index + GOAL_CONTEXT_LINES + 1).min(lowered.len())];
    let named = terms
        .iter()
        .filter(|term| window.iter().any(|line| line.contains(term.as_str())))
        .count();
    (hit_shape(&lowered[index], after, declares), named)
}

fn hit_shape(lower: &str, after: usize, declares: bool) -> usize {
    let code = lower.trim_start();
    if ["//", "#", "/*", "*", "--", "<!--", "\"\"\""]
        .iter()
        .any(|marker| code.starts_with(marker))
    {
        return 0;
    }
    let rest = lower.get(after..).unwrap_or_default().trim_start();
    let value = rest
        .strip_prefix(":=")
        .or_else(|| {
            rest.strip_prefix('=')
                .filter(|value| !value.starts_with(['=', '>']))
        })
        .or_else(|| {
            rest.strip_prefix(':')
                .filter(|value| !value.starts_with(':'))
        })
        .map(str::trim_start);
    if let Some(value) = value {
        let literal = value
            .starts_with(|c: char| c.is_ascii_digit() || matches!(c, '"' | '\'' | '`'))
            || (value.starts_with('-') && value[1..].starts_with(|c: char| c.is_ascii_digit()))
            || ["true", "false", "none", "null", "nil"]
                .iter()
                .any(|word| value.starts_with(word));
        return if literal { 3 } else { 2 };
    }
    if declares { 2 } else { 1 }
}

/// The line itself when short; otherwise a window starting a little before
/// the first keyword, marked with `…`.
fn clip(line: &str, at_char: usize) -> String {
    let chars = line.chars().count();
    if chars <= MAX_LINE_CHARS {
        return line.to_owned();
    }
    let start = at_char.saturating_sub(40).min(chars - MAX_LINE_CHARS);
    let body: String = line.chars().skip(start).take(MAX_LINE_CHARS).collect();
    let prefix = if start > 0 { "…" } else { "" };
    let suffix = if start + MAX_LINE_CHARS < chars {
        "…"
    } else {
        ""
    };
    format!("{prefix}{body}{suffix}")
}

#[cfg(test)]
mod tests;
