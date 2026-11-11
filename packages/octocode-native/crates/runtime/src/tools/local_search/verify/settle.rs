//! Each shown value settled against what its source's read said: its key block, its
//! whole-source scan, or the lines around it.

use super::guard::{OVERSIZED_PLACEHOLDER, RowShape, UNPLACED_PLACEHOLDER, UNVERIFIED_PLACEHOLDER};
use super::lines::VerificationLines;
use super::*;
use crate::security::cut::{Cut, REDACTED, exposes, placed, rendered};
use octocode_engine::security::Edits;
use std::collections::BTreeMap;
use std::ops::Range;

/// What judging a whole source says of its rows.
pub(super) enum WholeScan {
    /// It could not be held whole: nothing judges its rows, and none is shown.
    Unheld,
    /// Judged as a read of it shows it.
    Judged(Verdict),
}

/// A whole source's judgment ([`crate::security::JudgedText`]): `None` when
/// the pass replaced nothing of it, so every row shows its own bytes and the
/// source's text need not be kept.
pub(in crate::tools::local_search) type Verdict =
    Option<std::sync::Arc<crate::security::JudgedText>>;

/// What the read of one source said of its shown values.
pub(super) struct Judged<'a> {
    pub(super) read: &'a VerificationLines,
    /// The whole source's scan, when one decides.
    pub(super) whole: Option<WholeScan>,
    /// The lines of `read` a walk of the source masked ([`super::exposed`]),
    /// each with what it replaced of its source line.
    pub(super) masked: &'a BTreeMap<usize, Edits>,
}

/// One judged window: what its scan replaced, by byte position in the window,
/// and the scan's text when it replaced anything (a clean window needs none).
type WindowJudgment = (std::sync::Arc<Edits>, std::sync::Arc<str>);

/// The row windows a stored scan's pages judged, by source, the digest its
/// bytes were proved to hash to, and the digest of the exact text judged (a
/// window's lines can be the source's or a walk's masked ones). The scan owns it (as it owns
/// its name prices), so a later page reuses what an earlier one judged of
/// the same bytes instead of scanning the window again: a walk of spans on
/// one long line scans that line once, not once per page.
///
/// Once the scan is stored, each verdict a later page learns is charged to
/// the store ([`Billing`]) and kept only when the charge holds.
///
/// A source judged whole is kept the same way, under its path and the digest
/// of its bytes ([`Verdict`]): the first page that shows it judges it, fresh
/// or stored, and every later page of the walk cuts its rows from that
/// judgment after proving the bytes, with no line of it read for a scan.
#[derive(Default)]
pub(in crate::tools::local_search) struct JudgedWindows(
    std::sync::Mutex<std::collections::HashMap<WindowKey, WindowJudgment>>,
    Billing,
    std::sync::Mutex<std::collections::HashMap<SourceKey, Verdict>>,
);

use crate::tools::local_search::manifest::Billing;
use crate::tools::local_search::sources::Digest;

/// A judged window: its source, the digest its bytes were proved to hash
/// to, and the digest of the text judged, which places its edits.
type WindowKey = (std::path::PathBuf, Digest, Digest);
/// A source judged whole: its path and the digest of its bytes.
type SourceKey = (std::path::PathBuf, Digest);
/// Bytes one kept whole-source verdict holds.
fn whole_weight((path, _): &SourceKey, verdict: &Verdict) -> usize {
    path.as_os_str().len()
        + verdict.as_ref().map_or(0, |judged| judged.weight())
        + std::mem::size_of::<(SourceKey, Verdict)>()
}
/// Bytes one kept verdict holds.
fn verdict_weight((path, ..): &WindowKey, (redacted, clean): &WindowJudgment) -> usize {
    path.as_os_str().len()
        + clean.len()
        + redacted.heap_bytes()
        + std::mem::size_of::<(WindowKey, WindowJudgment)>()
}

impl JudgedWindows {
    fn lock(
        &self,
    ) -> std::sync::MutexGuard<'_, std::collections::HashMap<WindowKey, WindowJudgment>> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn whole(&self) -> std::sync::MutexGuard<'_, std::collections::HashMap<SourceKey, Verdict>> {
        self.2
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Bytes the judgments keep.
    pub(in crate::tools::local_search) fn weight(&self) -> usize {
        let windows: usize = self
            .lock()
            .iter()
            .map(|(key, judgment)| verdict_weight(key, judgment))
            .sum();
        let whole: usize = self
            .whole()
            .iter()
            .map(|(key, verdict)| whole_weight(key, verdict))
            .sum();
        windows + whole
    }

    /// The judgment kept of the whole source at `source` whose bytes hash to
    /// `digest`; `None` when no page judged those bytes, or its charge was
    /// refused.
    pub(super) fn whole_verdict(
        &self,
        source: &std::path::Path,
        digest: Digest,
    ) -> Option<Verdict> {
        self.whole().get(&(source.to_path_buf(), digest)).cloned()
    }

    /// Keep `verdict` for the whole source at `key`, charged as
    /// [`Self::learn`] charges a window's.
    fn learn_whole(&self, key: SourceKey, verdict: Verdict) {
        let mut known = self.whole();
        if known.contains_key(&key) {
            return;
        }
        if self.1.charge(whole_weight(&key, &verdict)) {
            known.insert(key, verdict);
        }
    }

    /// Where verdicts learned once the scan is stored are charged.
    pub(in crate::tools::local_search) fn billing(&self) -> &Billing {
        &self.1
    }

    /// Keep `judgment` for `key` when its bytes are charged (or nothing bills
    /// them yet: the scan is weighed whole when it is stored). A refused one is
    /// not kept, and a later page judges that window again. Charged and kept
    /// under one lock, so a verdict two pages learn is charged once.
    fn learn(&self, key: WindowKey, judgment: WindowJudgment) {
        let mut known = self.lock();
        if known.contains_key(&key) {
            return;
        }
        if self.1.charge(verdict_weight(&key, &judgment)) {
            known.insert(key, judgment);
        }
    }
}

/// The source lines a shown row's value holds: the line it starts at and how
/// many.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RowLines {
    pub(super) first: usize,
    pub(super) span: usize,
}

/// Where `matched`'s value starts in its source, for a row of `shape`. A span
/// starts at its match. A lines row holds its match's lines with the context the
/// search joined around them, clamped at the source's start as the search
/// clamps it, or its match's lines alone. `None` for a value that is neither:
/// nothing places it.
pub(super) fn row_lines(
    shape: RowShape,
    matched: &octocode_engine::types::TextSearchMatch,
) -> Option<RowLines> {
    let line = matched.line as usize;
    let RowShape::Lines(context) = shape else {
        // A span ending in its line's newline holds no line past it.
        let span = matched.value.lines().count().max(1);
        return Some(RowLines { first: line, span });
    };
    // Where the search says its value starts: the lines it joined.
    let first = matched.first_line as usize;
    let span = matched.value.split('\n').count();
    let own = matched
        .last_line
        .map_or(1, |last| (last as usize).saturating_sub(line) + 1);
    // An invariant, not a derivation: a value whose lines disagree with the
    // window the search reports is not placed.
    let before = line.checked_sub(first)?;
    let after = span.checked_sub(before + own)?;
    (before <= context.before as usize && after <= context.after as usize)
        .then_some(RowLines { first, span })
}

/// A stored scan's judged windows, with the digest a read proved its source's
/// bytes hash to.
pub(super) type Learned<'a> = (
    &'a JudgedWindows,
    crate::tools::local_search::sources::Digest,
);

/// Settle `shown`, each with the lines `windows` holds for it and where its
/// value starts (`placed`, from [`row_lines`]). `learned`:
/// the stored scan's judged windows and the digest this read proved its
/// source's bytes hash to.
pub(super) fn settle_rows(
    shown: &mut [octocode_engine::types::TextSearchMatch],
    windows: &[(usize, usize)],
    judged: &Judged<'_>,
    source: &std::path::Path,
    security: &ContentSecurity,
    (placed, shape, learned): (&[Option<RowLines>], RowShape, Option<Learned<'_>>),
) -> Verification {
    let match_only = shape == RowShape::Span;
    let Judged {
        read,
        whole,
        masked,
    } = judged;
    if let Some(whole) = whole {
        return settle_judged(shown, placed, shape, whole, read.lines_read, security);
    }
    let mut verified = Verification::Verified;
    // Rows of one line, or of nearby lines, share a window: each distinct
    // window is judged once for the page, so a page of spans on one long
    // line scans that line once, not once per span.
    let mut judged_windows = std::collections::HashMap::<(usize, usize), WindowJudgment>::new();
    for ((matched, &(lo, hi)), placed) in shown.iter_mut().zip(windows).zip(placed) {
        let Some(RowLines { first, span }) = *placed else {
            // Its lines disagree with the window the search reports: nothing
            // places them in the source, so none is shown.
            matched.value = UNPLACED_PLACEHOLDER.to_owned();
            verified = Verification::Unverified;
            continue;
        };
        if !read.key_ranges.is_empty()
            && crate::security::match_window_intersects_key_block(
                matched.line,
                &matched.value,
                &read.key_ranges,
            )
        {
            let kept =
                redact_key_block_lines(matched.line, &matched.value, &read.key_ranges, |n| {
                    read.retained.get(&(n as usize)).map(String::as_str)
                });
            matched.value = security
                .sanitize_text(&kept, Some(source))
                .content
                .into_owned();
            // No whole-source scan judged a span's lines past the block: they
            // were judged only as the span cut them, and a token's first bytes
            // after the armor match nothing. Nothing places them, so the span
            // goes.
            if match_only && span > 1 {
                matched.value = REDACTED.to_owned();
            }
            // Its lines past the block were judged alone: they are judged
            // again in their window, the block's lines masked there.
            if match_only {
                continue;
            }
        }
        let line = matched.line as usize;
        if line == 0 {
            continue;
        }
        if line > read.lines_read {
            // The source shrank or was replaced since the search.
            matched.value = UNVERIFIED_PLACEHOLDER.to_owned();
            verified = Verification::Unverified;
            continue;
        }
        let (lo, hi) = row_window(
            &read.retained,
            (lo, hi.min(read.lines_read)),
            (first, span),
            security.limits().whole_source,
        );
        // Too long to keep for a second look, the search judged it, and only a
        // window of it or its own span is on the row.
        if read.long_lines.contains(&line) {
            continue;
        }
        // A private key's lines show as its placeholder wherever a window holds
        // them, so no row renders them from the window.
        let key_line = crate::security::key_fragment_placeholder();
        let key_line = key_line.as_str();
        let in_block = |n: usize| {
            read.key_ranges
                .iter()
                .any(|&(s, e)| s as usize <= n && n <= e as usize)
        };
        let Some(window) = (lo..=hi)
            .map(|n| {
                let text = read.retained.get(&n).map(String::as_str);
                if in_block(n) {
                    text.map(|_| key_line)
                } else {
                    text
                }
            })
            .collect::<Option<Vec<_>>>()
        else {
            matched.value = OVERSIZED_PLACEHOLDER.to_owned();
            verified = Verification::Unverified;
            continue;
        };
        let (redacted, clean) = judged_windows.entry((lo, hi)).or_insert_with(|| {
            let joined = window.join("\n");
            let key = learned.map(|(_, digest)| {
                let text = octocode_engine::digest::sha256_digest(joined.as_bytes());
                (source.to_path_buf(), digest, text)
            });
            if let (Some((known, _)), Some(key)) = (learned, &key)
                && let Some(judgment) = known.lock().get(key)
            {
                return judgment.clone();
            }
            #[cfg(test)]
            JUDGED.with(|judged| judged.set(judged.get() + joined.len()));
            let sanitized = security.sanitize_text(&joined, Some(source));
            let judgment: WindowJudgment = if sanitized.has_secrets {
                (
                    std::sync::Arc::new(fail_closed(
                        sanitized.edits,
                        joined.len(),
                        &sanitized.content,
                    )),
                    sanitized.content.as_ref().into(),
                )
            } else {
                (std::sync::Arc::default(), "".into())
            };
            if let (Some((known, _)), Some(key)) = (learned, key) {
                known.learn(key, judgment.clone());
            }
            judgment
        });
        let keyed = (first..first + span).any(in_block);
        show_in_window(
            matched,
            (&window, first, first - lo, span, keyed),
            (redacted, clean),
            masked,
            match_only,
        );
    }
    verified
}

/// Settle `matched`, a row starting on line `at` of `window` spanning `span`
/// lines (`keyed`: some of them a private key's, masked in `window`), against
/// the window's scan (`redacted`, leaving `clean`) and the walk's `masked`
/// lines: what it shows is rendered from the runs that decided it.
fn show_in_window(
    matched: &mut octocode_engine::types::TextSearchMatch,
    (window, first, at, span, keyed): (&[&str], usize, usize, usize, bool),
    (redacted, clean): (&Edits, &str),
    masked: &BTreeMap<usize, Edits>,
    match_only: bool,
) {
    let line = matched.line as usize;
    let walked = (first..first + span).any(|at| masked.contains_key(&at));
    if redacted.is_empty() && !walked && !keyed {
        return;
    }
    let cut = row_cut(match_only, matched.byte_offset, &matched.value, span);
    let joined = window.join("\n");
    // The row's own lines, in the window's coordinates.
    let lines = placed(window, at, &Cut::Lines(span));
    if !match_only {
        // A content row shows its lines as the window's scan left them (a
        // walk-masked line already holds its masked text).
        if exposes(redacted, &lines) || walked || keyed {
            let text = joined.get(lines.clone()).unwrap_or_default();
            matched.value = rendered(redacted.runs(clean), text, &lines, &lines, |r| {
                joined.get(r)
            });
        }
        return;
    }
    let Cut::Span { offset, text } = cut else {
        return;
    };
    if !walked {
        let own = placed(window, at, &cut);
        if exposes(redacted, &own) {
            matched.value = rendered(redacted.runs(clean), text, &own, &lines, |r| joined.get(r));
        }
        return;
    }
    // A walk-masked line holds its masked text, not the source's: a span on
    // one is judged by what the walk replaced of its own line, each shown
    // byte checked against the masked line at its masked place. Across
    // lines, or where the window's scan redacted those lines too, it goes.
    let own = offset..offset + text.len();
    let replaced = masked.get(&line);
    let value = match replaced {
        Some(replaced) if span == 1 && !exposes(redacted, &lines) => {
            let masked_line = window[at];
            let runs = replaced.runs(masked_line).collect::<Vec<_>>();
            if !runs
                .iter()
                .any(|(range, _)| range.start < own.end && own.start < range.end)
            {
                None
            } else {
                Some(rendered(
                    runs.iter().map(|(range, _)| (range.clone(), REDACTED)),
                    text,
                    &own,
                    &(0..0),
                    |r| masked_line.get(masked_place(&runs, r)),
                ))
            }
        }
        _ => Some(REDACTED.to_owned()),
    };
    if let Some(value) = value {
        matched.value = value;
    }
}

#[cfg(test)]
thread_local! {
    /// Window bytes this thread's row settling judged, for tests that pin
    /// how much a page scans.
    static JUDGED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Window bytes judged by row settling on this thread so far.
#[cfg(test)]
pub(crate) fn judged_on_this_thread() -> usize {
    JUDGED.with(std::cell::Cell::get)
}

/// The lines a row is judged with: `lo..=hi`, or its own when those are more than
/// the scanner judges at once. A multi-line secret cannot be judged across such a
/// neighbor anyway, and a row must not go with it.
fn row_window(
    retained: &std::collections::BTreeMap<usize, String>,
    (lo, hi): (usize, usize),
    (line, span): (usize, usize),
    whole_source: usize,
) -> (usize, usize) {
    let bytes: usize = (lo..=hi)
        .filter_map(|at| retained.get(&at))
        .map(|text| text.len() + 1)
        .sum();
    if bytes > whole_source {
        (line, (line + span - 1).min(hi))
    } else {
        (lo, hi)
    }
}

/// Where source bytes `range` of a line, outside every run of `runs` (what a
/// walk replaced, with its placeholder), sit in the masked line.
fn masked_place(runs: &[(Range<usize>, &str)], range: Range<usize>) -> Range<usize> {
    let before = runs.iter().take_while(|(run, _)| run.end <= range.start);
    let (taken, left) = before.fold((0, 0), |(taken, left), (run, placeholder)| {
        (taken + run.len(), left + placeholder.len())
    });
    let start = range.start - taken + left;
    start..start + range.len()
}

/// What a row shows of its lines: its own span in a matchOnly row, else its
/// lines whole (a content row's value can be clipped or windowed).
fn row_cut<'a>(match_only: bool, offset: u32, value: &'a str, span: usize) -> Cut<'a> {
    if match_only {
        Cut::Span {
            offset: offset as usize,
            text: value,
        }
    } else {
        Cut::Lines(span)
    }
}

/// `redacted`, the edits of a scan that found secrets in a text of `len` bytes
/// and left `clean`; when they place nothing although the text changed, the
/// whole text counts as redacted.
fn fail_closed(redacted: Edits, len: usize, clean: &str) -> Edits {
    if redacted.is_empty() {
        Edits::of_pass([(0..len, clean.len())])
    } else {
        redacted
    }
}

/// The whole source judged as a read of it shows it
/// ([`crate::security::JudgedText`]): the one redaction decision each of its
/// rows is cut from. [`WholeScan::Unheld`] when it could not be held whole.
/// `learned` is the stored scan's judgments and the digest this read proved
/// the source's bytes hash to: the judgment is kept there, so a later page of
/// the walk cuts its rows from it ([`JudgedWindows::whole_verdict`]) and a
/// file whose hits span many pages is scanned once, not once a page.
pub(super) fn whole_source_scan(
    read: &VerificationLines,
    source: &std::path::Path,
    security: &ContentSecurity,
    learned: Option<Learned<'_>>,
) -> WholeScan {
    if !read.whole_retained {
        return WholeScan::Unheld;
    }
    if let Some((known, digest)) = learned
        && let Some(verdict) = known.whole_verdict(source, digest)
    {
        security.note_judged_whole(source);
        return WholeScan::Judged(verdict);
    }
    let mut text = String::with_capacity(read.retained_bytes.saturating_add(read.retained.len()));
    for (index, line) in read.retained.values().enumerate() {
        if index > 0 {
            text.push('\n');
        }
        text.push_str(line);
    }
    #[cfg(test)]
    JUDGED.with(|judged| judged.set(judged.get() + text.len()));
    let judged = security.judge_whole(&text, source);
    let verdict = judged.is_redacted().then(|| std::sync::Arc::new(judged));
    if let Some((known, digest)) = learned {
        known.learn_whole((source.to_path_buf(), digest), verdict.clone());
    }
    WholeScan::Judged(verdict)
}

/// Settle `shown`, the rows of a source judged whole (`whole`), each placed
/// by `placed`: every row is the cut of its place in that one judgment, on
/// the page that judged the source and on every later page that reuses it.
/// `lines` is how many lines the source has: a row past them is not its
/// bytes.
pub(super) fn settle_judged(
    shown: &mut [octocode_engine::types::TextSearchMatch],
    placed: &[Option<RowLines>],
    shape: RowShape,
    whole: &WholeScan,
    lines: usize,
    security: &ContentSecurity,
) -> Verification {
    let match_only = shape == RowShape::Span;
    let mut verified = Verification::Verified;
    for (matched, placed) in shown.iter_mut().zip(placed) {
        let Some(RowLines { first, span }) = *placed else {
            // Its lines disagree with the window the search reports: nothing
            // places them in the source, so none is shown.
            matched.value = UNPLACED_PLACEHOLDER.to_owned();
            verified = Verification::Unverified;
            continue;
        };
        let line = matched.line as usize;
        if line == 0 {
            continue;
        }
        if line > lines {
            // The source shrank or was replaced since the search.
            matched.value = UNVERIFIED_PLACEHOLDER.to_owned();
            verified = Verification::Unverified;
            continue;
        }
        match whole {
            WholeScan::Unheld => {
                matched.value = OVERSIZED_PLACEHOLDER.to_owned();
                verified = Verification::Unverified;
            }
            // The pass replaced nothing: the row shows its own bytes.
            WholeScan::Judged(None) => {}
            WholeScan::Judged(Some(judged)) => {
                let raw = std::mem::take(&mut matched.value);
                let cut = row_cut(match_only, matched.byte_offset, &raw, span);
                let shown = cut_of_whole_source(&cut, first, judged);
                matched.value = shown.unwrap_or(raw);
                security.show(judged.path(), &matched.value);
            }
        }
    }
    verified
}

/// What a row that showed `cut` of the lines from `first` shows of the whole
/// source `judged`: the cut of its place there, as a read of those bytes
/// shows it; `None` when the pass replaced nothing of it. A value that is
/// not the source's bytes at its place is judged by every line it reaches.
fn cut_of_whole_source(
    cut: &Cut<'_>,
    first: usize,
    judged: &crate::security::JudgedText,
) -> Option<String> {
    let span = match cut {
        Cut::Span { text, .. } => text.lines().count().max(1),
        Cut::Lines(count) => *count,
    };
    let lines = judged.line_span(first, span);
    let own = match (cut, &lines) {
        (Cut::Span { offset, text }, Some(lines)) => {
            let start = lines.start + offset;
            let at = start..start + text.len();
            (judged.raw().get(at.clone()) == Some(*text)).then_some(at)
        }
        _ => None,
    };
    match (own, lines) {
        (Some(own), _) => judged.exposes(&own).then(|| judged.cut(own).into_owned()),
        (None, Some(lines)) if !judged.exposes(&lines) => None,
        (None, Some(lines)) => Some(match cut {
            Cut::Span { .. } => REDACTED.to_owned(),
            Cut::Lines(_) => judged.cut(lines).into_owned(),
        }),
        (None, None) => judged.is_redacted().then(|| REDACTED.to_owned()),
    }
}

#[cfg(test)]
mod judged_tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn verdict(n: usize) -> (WindowKey, WindowJudgment) {
        let key = (
            std::path::PathBuf::from(format!("src/{n}.rs")),
            [0; 32],
            [u8::try_from(n).expect("small"); 32],
        );
        let edits = Edits::of_pass([(0..n + 1, 10)]);
        (key, (Arc::new(edits), Arc::from("x".repeat(n * 7))))
    }

    /// Each verdict learned after the bill is set is charged its own bytes,
    /// once, and what is kept weighs exactly what was charged.
    #[test]
    fn learned_verdicts_are_charged_their_real_size() {
        let judged = JudgedWindows::default();
        let charged = Arc::new(AtomicUsize::new(0));
        let total = Arc::clone(&charged);
        judged.billing().set(Arc::new(move |bytes| {
            total.fetch_add(bytes, Ordering::SeqCst);
            true
        }));
        let mut expected = 0;
        for n in 1..=5 {
            let (key, judgment) = verdict(n);
            expected += verdict_weight(&key, &judgment);
            judged.learn(key.clone(), judgment.clone());
            judged.learn(key, judgment);
        }
        assert_eq!(charged.load(Ordering::SeqCst), expected);
        assert_eq!(judged.weight(), expected);
    }

    fn placed_spans(rows: &[octocode_engine::types::TextSearchMatch]) -> Vec<Option<RowLines>> {
        rows.iter()
            .map(|row| row_lines(RowShape::Span, row))
            .collect()
    }

    fn read_of(line: &str) -> VerificationLines {
        VerificationLines {
            key_ranges: Vec::new(),
            retained: BTreeMap::from([(1, line.to_owned())]),
            retained_bytes: line.len(),
            lines_read: 1,
            unclassified: false,
            source_wide: false,
            whole_retained: true,
            long_lines: std::collections::BTreeSet::new(),
        }
    }

    fn settled(line: &str, span: Range<usize>, known: &JudgedWindows) -> String {
        let value = &line[span.clone()];
        let mut rows = [octocode_engine::types::TextSearchMatch {
            deferred: None,
            line: 1,
            first_line: 1,
            last_line: None,
            column: u32::try_from(span.start + 1).expect("column"),
            byte_offset: u32::try_from(span.start).expect("offset"),
            value: value.into(),
            value_digest: octocode_engine::types::TextSearchMatch::digest_of(value),
            count: None,
            rank: None,
        }];
        let read = read_of(line);
        let judged = Judged {
            read: &read,
            whole: None,
            masked: &BTreeMap::new(),
        };
        let placed = placed_spans(&rows);
        settle_rows(
            &mut rows,
            &[(1, 1)],
            &judged,
            std::path::Path::new("/src/a.txt"),
            &ContentSecurity::new(),
            (&placed, RowShape::Span, Some((known, [7; 32]))),
        );
        rows[0].value.clone()
    }

    /// A window's verdict is reused only for the exact text it judged: the same
    /// lines of the same source bytes, read once raw and once as a walk masked
    /// them, are judged each by their own scan.
    #[test]
    fn a_verdict_is_never_replayed_onto_other_text_of_its_window() {
        let token = "ghp_Z9Q8W7E6R5T4Y3U2I1O0P9L8K7J6H5G4F3D2";
        let known = JudgedWindows::default();
        let masked = "k = [REDACTED-GITHUBTOKENS] and a tail of text";
        assert_eq!(settled(masked, 4..10, &known), "[REDAC");
        let raw = format!("k = {token} and a tail of text");
        let shown = settled(&raw, 4..10, &known);
        assert!(!shown.contains("Z9"), "{shown}");
        assert_eq!(known.lock().len(), 2, "one verdict per judged text");
    }

    /// A span from a key block's armor onto the line after it, in a source no
    /// whole scan could judge, shows nothing of that line: the token's first
    /// bytes there match no pattern alone.
    #[test]
    fn a_span_past_a_key_block_without_a_whole_scan_goes_whole() {
        let lines = [
            "-----BEGIN RSA PRIVATE KEY-----",
            "MIIEowIBAAKCAQEA",
            "-----END RSA PRIVATE KEY-----",
            "ghp_Z9Q8W7E6R5T4Y3U2I1O0P9L8K7J6H5G4F3D2",
        ];
        let mut read = read_of(lines[0]);
        read.retained = (1..=4).map(|n| (n, lines[n - 1].to_owned())).collect();
        read.lines_read = 4;
        read.key_ranges = vec![(1, 3)];
        let value = "KEY-----\nghp_Z9Q8";
        let mut rows = [octocode_engine::types::TextSearchMatch {
            deferred: None,
            line: 3,
            first_line: 3,
            last_line: Some(4),
            column: 22,
            byte_offset: 21,
            value: value.into(),
            value_digest: octocode_engine::types::TextSearchMatch::digest_of(value),
            count: None,
            rank: None,
        }];
        let judged = Judged {
            read: &read,
            whole: None,
            masked: &BTreeMap::new(),
        };
        let placed = placed_spans(&rows);
        settle_rows(
            &mut rows,
            &[(2, 5)],
            &judged,
            std::path::Path::new("/src/key.txt"),
            &ContentSecurity::new(),
            (&placed, RowShape::Span, None),
        );
        assert!(!rows[0].value.contains("Z9"), "{}", rows[0].value);
    }

    const TOKEN: &str = "ghp_Z9Q8W7E6R5T4Y3U2I1O0P9L8K7J6H5G4F3D2";
    type MaskedLine = Edits;

    /// A masked line whose walk hid `hid` and left `placeholder` there.
    fn masked_of(hid: Range<usize>, placeholder: &str) -> MaskedLine {
        Edits::of_pass([(hid, placeholder.len())])
    }

    /// `value`, a matchOnly row at `offset` of line 1 of `line`, as settled
    /// with the walk's masked lines `masked`.
    fn settle_one(
        line: &str,
        value: &str,
        offset: usize,
        masked: &BTreeMap<usize, MaskedLine>,
    ) -> String {
        let mut rows = [octocode_engine::types::TextSearchMatch {
            deferred: None,
            line: 1,
            first_line: 1,
            last_line: None,
            column: u32::try_from(offset + 1).expect("column"),
            byte_offset: u32::try_from(offset).expect("offset"),
            value: value.into(),
            value_digest: octocode_engine::types::TextSearchMatch::digest_of(value),
            count: None,
            rank: None,
        }];
        let read = read_of(line);
        let judged = Judged {
            read: &read,
            whole: None,
            masked,
        };
        let placed = placed_spans(&rows);
        settle_rows(
            &mut rows,
            &[(1, 1)],
            &judged,
            std::path::Path::new("/src/a.txt"),
            &ContentSecurity::new(),
            (&placed, RowShape::Span, None),
        );
        rows[0].value.clone()
    }

    /// A row whose text is not its line's bytes at its place is not shown by
    /// its length: equal lengths do not make it the source's bytes.
    #[test]
    fn a_row_of_the_wrong_bytes_goes_whole_even_at_the_right_length() {
        let line = format!("ab {TOKEN}");
        let value = "x".repeat(line.len());
        assert_eq!(settle_one(&line, &value, 0, &BTreeMap::new()), REDACTED);
    }

    /// On a line a walk masked, a span beside the secret is checked against
    /// the source bytes the masked line still holds, at their masked place.
    #[test]
    fn a_span_beside_a_walk_masked_secret_is_shown_exactly() {
        let source = format!("aa {TOKEN} bb cc");
        let placeholder = "[REDACTED-GITHUBTOKENS]";
        let masked_line = format!("aa {placeholder} bb cc");
        let masked = BTreeMap::from([(1, masked_of(3..3 + TOKEN.len(), placeholder))]);
        let at = source.find("bb").expect("bb");
        assert_eq!(settle_one(&masked_line, "bb", at, &masked), "bb");
        let at = source.find("Z9").expect("Z9");
        assert_eq!(settle_one(&masked_line, "Z9", at, &masked), REDACTED);
    }

    /// A store that refuses the bytes keeps no verdict: it is judged again.
    #[test]
    fn a_refused_charge_keeps_no_verdict() {
        let judged = JudgedWindows::default();
        judged.billing().set(Arc::new(|_| false));
        let (key, judgment) = verdict(3);
        judged.learn(key.clone(), judgment);
        assert_eq!(judged.weight(), 0);
        assert!(judged.lock().get(&key).is_none());
    }
}
