//! What a ranked scan keeps across the windows its time budget cuts.
//!
//! A ranked order ([`ranks_whole_scope`]: `matchCount`, or any descending
//! order) orders the scope's whole file set, so a window that ranked only the
//! files it evaluated before its cut would list a wrong first page. A cut
//! window lists no file: it keeps every file the scan evaluated so far
//! ([`Standing`]) under the position the scan stopped at and the pin of the
//! candidates before it, and the window that resumes it (`next.expandScan`,
//! `scanOffset`) evaluates on from there and adds its files to them. The window
//! that ends the scan answers as the scan's first window (`scanOffset` 0) with
//! every file: it lists the pages a scan the budget never cut lists, whatever
//! the clock did.
//!
//! What is kept is a candidate: a resuming window serves it only while the
//! candidates before its offset are the ones the kept windows evaluated (its
//! pin, which [`super::window_moved`] checks first). A lost standing (another
//! process, an eviction) is rebuilt by evaluating those candidates again from
//! the first, as an explicit file window the engine never cuts by time, so
//! every window still makes progress.
use super::continuations::insert_expand_scan;
use super::memo::scan_bytes;
use super::query::MatchQuery;
pub(super) use super::scan::ranks_whole_scope;
use super::scan::scan_directory;
use super::snapshot::{directory_source_digest, scan_fields};
use super::types::{Scan, Scope};
use octocode_runtime_base::execution::cancel::CancellationCheck;
use octocode_runtime_base::policy::path::PathPolicy;
use octocode_runtime_base::response::result::ToolError;
use octocode_tools_shared::page_memo::PageMemo;
use serde_json::{Value, json};
use std::sync::{Mutex, PoisonError};
use std::time::SystemTime;

/// Every file the windows of a ranked scan evaluated so far, with the
/// coverage of the latest window's walk.
pub(super) struct Standing {
    /// Files and `sources` in evaluation (walk) order, so `sources[i]` is the
    /// path `files[i]` was read at.
    scan: Scan,
    /// When the first of its windows started.
    started: SystemTime,
}

impl Standing {
    /// Add the files `window` evaluated after the ones kept. What a window
    /// counts of the files it evaluated adds up; what its walk met (each walk
    /// meets every candidate a window before it met) is the latest walk's.
    fn absorb(&mut self, window: Scan) {
        let Scan {
            files,
            truncated,
            filled,
            scan_resume,
            listed_before,
            listed_through,
            earlier_files: _,
            diagnostics,
            skips,
            walk_errors,
            skipped_by_prefilter,
            sources,
            source_digest: _,
            discovery_sources,
            ignore_rules,
            repository_markers,
            watches_ignore_files,
            skipped,
            unparsed,
            census,
            present,
            test_code_excluded,
            test_code_walked,
            test_code_unread,
        } = window;
        let kept = &mut self.scan;
        kept.files.extend(files);
        kept.sources.extend(sources);
        kept.diagnostics.extend(diagnostics);
        kept.skips = (
            kept.skips.0.saturating_add(skips.0),
            kept.skips.1.saturating_add(skips.1),
            kept.skips.2.saturating_add(skips.2),
        );
        kept.skipped_by_prefilter = kept
            .skipped_by_prefilter
            .saturating_add(skipped_by_prefilter);
        let read_before = kept
            .test_code_excluded
            .files
            .saturating_sub(kept.test_code_walked);
        let read_now = test_code_excluded.files.saturating_sub(test_code_walked);
        kept.test_code_excluded = octocode_engine::types::TestCodeExcluded {
            files: test_code_walked
                .saturating_add(read_before)
                .saturating_add(read_now),
            lines: kept
                .test_code_excluded
                .lines
                .saturating_add(test_code_excluded.lines),
        };
        kept.test_code_walked = test_code_walked;
        union(&mut kept.ignore_rules, ignore_rules);
        union(&mut kept.test_code_unread, test_code_unread);
        kept.truncated = truncated;
        kept.filled = filled;
        kept.scan_resume = scan_resume;
        kept.listed_before = listed_before;
        kept.listed_through = listed_through;
        kept.walk_errors = walk_errors;
        kept.discovery_sources = discovery_sources;
        kept.repository_markers = repository_markers;
        kept.watches_ignore_files = watches_ignore_files;
        let artifacts = std::mem::take(&mut kept.skipped.artifacts);
        kept.skipped = skipped;
        kept.skipped.artifacts.extend(artifacts);
        kept.unparsed = unparsed;
        kept.census = census;
        kept.present = present;
    }

    /// The scan the windows add up to, as one scan from the first candidate:
    /// no candidate lies before it, and its digest is of every file read.
    fn finish(self) -> (Scan, SystemTime) {
        let mut scan = self.scan;
        scan.source_digest =
            directory_source_digest(scan.sources.iter().zip(&scan.files).map(|(path, file)| {
                (
                    path.to_str().unwrap_or_default(),
                    file.source.as_ref().map(|(_, digest)| digest.as_str()),
                )
            }));
        scan.listed_before = None;
        scan.earlier_files = Vec::new();
        (scan, self.started)
    }
}

/// `more` added to `paths`, each path once.
fn union(paths: &mut Vec<std::path::PathBuf>, more: Vec<std::path::PathBuf>) {
    for path in more {
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
}

/// Where a ranked window leaves its scan.
pub(super) enum Ranked {
    /// The budget cut the scan again: the page that resumes it.
    Paused(Value),
    /// The candidates before the window are no longer the ones its pin names.
    Moved,
    /// The scan ended: every window's files as one scan, and when its first
    /// window started.
    Whole(Box<Scan>, SystemTime),
}

/// Standings by what they belong to; a taken one leaves the store.
static STANDINGS: PageMemo<Mutex<Option<Standing>>> = PageMemo::new();

/// What a standing belongs to: the scan and everything that decides its
/// files, what it keeps of them and their order, the pin of the candidates
/// before the position it stopped at, and that position.
fn key(q: MatchQuery<'_>, scope: &Scope, paths: &PathPolicy, pin: Option<&str>, at: u32) -> String {
    octocode_runtime_base::digest::json_sha256(&json!([
        "astSearch.standing",
        super::memo::scan_key(q, paths),
        scan_fields(q, scope),
        pin,
        at,
    ]))
}

/// The window `q` resumes a ranked scan with: the files the windows before
/// it evaluated added to `window`'s, and what follows from them.
pub(super) fn advance(
    q: MatchQuery<'_>,
    scope: &Scope,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
    (window, started): (Scan, SystemTime),
) -> Result<Ranked, ToolError> {
    let standing = match q.scan_offset() {
        0 => Standing {
            scan: window,
            started,
        },
        at => {
            let Some(mut earlier) = earlier(q, scope, paths, cancel, at)? else {
                return Ok(Ranked::Moved);
            };
            earlier.absorb(window);
            // A rebuilt standing started after this window read its files.
            earlier.started = earlier.started.min(started);
            earlier
        }
    };
    if !standing.scan.truncated {
        let (scan, started) = standing.finish();
        return Ok(Ranked::Whole(Box::new(scan), started));
    }
    let page = paused_page(q, &standing.scan)?;
    keep(q, scope, paths, standing);
    Ok(Ranked::Paused(page))
}

/// What the windows before `at` evaluated: kept, else evaluated again;
/// `None` when they are no longer the candidates `q`'s pin names.
fn earlier(
    q: MatchQuery<'_>,
    scope: &Scope,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
    at: u32,
) -> Result<Option<Standing>, ToolError> {
    let pin = q.snapshot();
    let key = key(q, scope, paths, pin.as_deref(), at);
    let kept = STANDINGS
        .get(&key, &paths.identity(), |kept| {
            kept.lock().unwrap_or_else(PoisonError::into_inner).take()
        })
        .flatten();
    STANDINGS.evict(&key);
    if let Some(kept) = kept {
        return Ok(Some(kept));
    }
    let started = octocode_engine::stamp::settle_clock();
    let scan = scan_directory(q, scope, paths, cancel, Some(at))?;
    cancel.check().map_err(ToolError::cancelled)?;
    let holds = pin.is_none_or(|pin| scan.listed_through.as_deref() == Some(pin.as_str()));
    Ok(holds.then_some(Standing { scan, started }))
}

/// A later page of a ranked scan whose scan was not kept (its sources had not
/// settled, or the store let it go): every candidate evaluated again, as an
/// explicit file window through the last position a `scanOffset` can name,
/// which the engine never cuts by time. Only the whole scope reproduces the
/// order the pages were cut from; a window the budget cut never does.
pub(super) fn rescan(
    q: MatchQuery<'_>,
    scope: &Scope,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<(Scan, SystemTime), ToolError> {
    let started = octocode_engine::stamp::settle_clock();
    let scan = scan_directory(q, scope, paths, cancel, Some(u32::MAX))?;
    cancel.check().map_err(ToolError::cancelled)?;
    Ok((scan, started))
}

/// Keep what a cut window evaluated for the window that resumes it.
fn keep(q: MatchQuery<'_>, scope: &Scope, paths: &PathPolicy, standing: Standing) {
    let Some(at) = standing.scan.scan_resume else {
        return;
    };
    let key = key(q, scope, paths, standing.scan.listed_through.as_deref(), at);
    let bytes = scan_bytes(&standing.scan);
    STANDINGS.put(key, paths.identity(), bytes, Mutex::new(Some(standing)));
}

/// The page of a ranked window the budget cut: no file yet, the count so far
/// and the call that resumes the scan where it stopped.
fn paused_page(q: MatchQuery<'_>, scan: &Scan) -> Result<Value, ToolError> {
    let matches = scan
        .files
        .iter()
        .map(|file| file.matches.len() as u64)
        .sum::<u64>();
    let mut out = json!({"matchCount": matches});
    let at = super::super::scan_window::expand_scan(scan.truncated, scan.scan_resume)
        .map(|window| window.scan_offset);
    insert_expand_scan(
        q,
        &mut out,
        at.map(u64::from),
        scan.listed_through.as_deref().unwrap_or_default(),
    )?;
    let at = at.unwrap_or_default();
    let order = match (q.sort().unwrap_or_default(), q.reverse().unwrap_or(false)) {
        (by, true) => format!("{by} descending"),
        (by, false) => format!("{by} ascending"),
    };
    octocode_runtime_base::response::paging::mark_partial(
        &mut out,
        super::super::scan_window::CUT_REASON,
        &[format!(
            "the scan paused at its time budget after {at} candidate files: an order by {order} ranks the whole scope, so it lists files once the scan ends, and matchCount counts the files scanned so far; `next.expandScan` resumes the scan there."
        )],
    );
    Ok(out)
}

/// Drop every standing kept for scans of the workspace `root`, as eviction
/// or another process would; what other tests in the process kept stays.
#[cfg(test)]
pub(super) fn forget_kept(root: &std::path::Path) {
    STANDINGS.evict_policy(&octocode_runtime_base::test_support::workspace_policy(root).identity());
}
