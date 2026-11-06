//! Each shown value settled against what its source's read said: its key block, its
//! whole-source scan, or the lines around it.

use super::guard::{OVERSIZED_PLACEHOLDER, UNVERIFIED_PLACEHOLDER};
use super::lines::VerificationLines;
use super::*;
use crate::security::cut::{Cut, REDACTED, exposes, placed, shown_span};
use octocode_engine::security::Edits;
use std::collections::BTreeMap;
use std::ops::Range;

/// A whole source's text as a read of it shows it, `None` when it could not be
/// scanned whole.
pub(super) struct WholeScan(pub(super) Option<Scanned>);

/// A text as its scan left it, and what the scan replaced of it by position.
pub(super) struct Scanned {
    pub(super) clean: String,
    pub(super) redacted: Edits,
}

/// What the read of one source said of its shown values.
pub(super) struct Judged<'a> {
    pub(super) read: &'a VerificationLines,
    /// The whole source's scan, when one decides.
    pub(super) whole: Option<WholeScan>,
    /// The lines of `read` a walk of the source masked ([`super::exposed`]),
    /// each with the byte ranges of the source line it hid.
    pub(super) masked: &'a BTreeMap<usize, Vec<Range<usize>>>,
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
#[derive(Default)]
pub(in crate::tools::local_search) struct JudgedWindows(
    std::sync::Mutex<std::collections::HashMap<WindowKey, WindowJudgment>>,
    Billing,
);

use crate::tools::local_search::manifest::Billing;
use crate::tools::local_search::sources::Digest;

/// A judged window: its source, the digest its bytes were proved to hash
/// to, and the digest of the text judged, which places its edits.
type WindowKey = (std::path::PathBuf, Digest, Digest);
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

    /// Bytes the judgments keep.
    pub(in crate::tools::local_search) fn weight(&self) -> usize {
        self.lock()
            .iter()
            .map(|(key, judgment)| verdict_weight(key, judgment))
            .sum()
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

/// Settle `shown`, each with the lines `windows` holds for it. `learned`:
/// the stored scan's judged windows and the digest this read proved its
/// source's bytes hash to.
pub(super) fn settle_rows(
    shown: &mut [octocode_engine::types::TextSearchMatch],
    windows: &[(usize, usize)],
    judged: &Judged<'_>,
    source: &std::path::Path,
    security: &ContentSecurity,
    (match_only, learned): (
        bool,
        Option<(&JudgedWindows, crate::tools::local_search::sources::Digest)>,
    ),
) -> Verification {
    let Judged {
        read,
        whole,
        masked,
    } = judged;
    let mut verified = Verification::Verified;
    // Rows of one line, or of nearby lines, share a window: each distinct
    // window is judged once for the page, so a page of spans on one long
    // line scans that line once, not once per span.
    let mut judged_windows = std::collections::HashMap::<(usize, usize), WindowJudgment>::new();
    for (matched, &(lo, hi)) in shown.iter_mut().zip(windows) {
        let span = matched.value.lines().count().max(1);
        if !read.key_ranges.is_empty()
            && crate::security::match_window_intersects_key_block(
                matched.line,
                &matched.value,
                &read.key_ranges,
            )
        {
            let raw = matched.value.clone();
            let kept =
                redact_key_block_lines(matched.line, &matched.value, &read.key_ranges, |n| {
                    read.retained.get(&(n as usize)).map(String::as_str)
                });
            matched.value = security
                .sanitize_text(&kept, Some(source))
                .content
                .into_owned();
            let cut = row_cut(match_only, matched.byte_offset, &raw, span);
            // Without a whole-source scan, a span's lines past the block were
            // judged only as the span cut them: a token's first bytes after the
            // armor match nothing. Nothing places them, so the span goes.
            if whole.is_none() && match_only && span > 1 {
                matched.value = REDACTED.to_owned();
            }
            if let Some(scan) = &whole
                && !settle_with_whole_source(matched, &cut, read, &scan.0, match_only)
            {
                verified = Verification::Unverified;
            }
            continue;
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
        if let Some(scan) = &whole {
            let raw = matched.value.clone();
            let cut = row_cut(match_only, matched.byte_offset, &raw, span);
            if !settle_with_whole_source(matched, &cut, read, &scan.0, match_only) {
                verified = Verification::Unverified;
            }
            continue;
        }
        let (lo, hi) = row_window(
            &read.retained,
            (lo, hi.min(read.lines_read)),
            (line, span),
            security.limits().whole_source,
        );
        // Too long to keep for a second look, the search judged it, and only a
        // window of it or its own span is on the row.
        if read.long_lines.contains(&line) {
            continue;
        }
        let Some(window) = (lo..=hi)
            .map(|n| read.retained.get(&n).map(String::as_str))
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
        let reached = line..line + span;
        let walked = reached.clone().filter_map(|at| masked.get(&at));
        if redacted.is_empty() && walked.clone().next().is_none() {
            continue;
        }
        let at = line - lo;
        let cut = row_cut(match_only, matched.byte_offset, &matched.value, span);
        // A walk-masked line holds its masked text, not the source's: a row on
        // one is judged by what the walk hid of its own line, and by any
        // redaction of those lines. Its own bytes are its span when the span is
        // that line's text at its place, else the whole line.
        let hid = walked.flatten().cloned().collect::<Vec<_>>();
        let (own, redactions): (_, Vec<(Range<usize>, &str)>) = if hid.is_empty() {
            (placed(&window, at, &cut), redacted.runs(clean).collect())
        } else if exposes(redacted, &placed(&window, at, &Cut::Lines(span))) {
            (0..usize::MAX, vec![(0..usize::MAX, REDACTED)])
        } else {
            let own = match cut {
                Cut::Span { offset, text }
                    if span == 1 && window[at].get(offset..offset + text.len()) == Some(text) =>
                {
                    offset..offset + text.len()
                }
                _ => 0..usize::MAX,
            };
            (
                own,
                hid.into_iter().map(|range| (range, REDACTED)).collect(),
            )
        };
        let shown_masked = redactions
            .iter()
            .any(|(range, _)| range.start < own.end.max(own.start + 1) && own.start < range.end);
        // A clean window's scan is its own text.
        let joined;
        let clean: &str = if redacted.is_empty() {
            joined = window.join("\n");
            &joined
        } else {
            clean
        };
        if shown_masked {
            matched.value = if match_only {
                shown_span(redactions, &own, &cut)
            } else {
                shown_line(window[line - lo], line - lo, clean, source, security)
            };
        }
    }
    verified
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

/// The line at `at` of a window `clean` is the scan of: as scanned alone, or when
/// only the lines around it make it secret, as the window's scan left it.
fn shown_line(
    line: &str,
    at: usize,
    clean: &str,
    source: &std::path::Path,
    security: &ContentSecurity,
) -> String {
    let alone = security.sanitize_text(line, Some(source)).content;
    if alone != line {
        return alone.into_owned();
    }
    clean
        .split('\n')
        .nth(at)
        .filter(|scanned| !scanned.trim().is_empty())
        .unwrap_or("[REDACTED]")
        .to_owned()
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

/// The whole source's text as a read of it shows it, for a source whose secrets a
/// window of it cannot show ([`VerificationLines::source_wide`]) or with a key's
/// armor in it. `None` when it could not be scanned whole.
pub(super) fn whole_source_scan(
    read: &VerificationLines,
    source: &std::path::Path,
    security: &ContentSecurity,
) -> Option<Scanned> {
    if !read.whole_retained {
        return None;
    }
    let text = read
        .retained
        .values()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join("\n");
    let (keyed, keys) = crate::security::private_key_blocks_hidden(&text);
    let scanned = security.sanitize_text(&keyed, Some(source));
    let changed = !keys.is_empty() || scanned.has_secrets;
    let redacted = keys.then(&scanned.edits);
    let clean = scanned.content.into_owned();
    Some(Scanned {
        redacted: if changed {
            fail_closed(redacted, text.len(), &clean)
        } else {
            redacted
        },
        clean,
    })
}

/// Check `matched`, which showed `cut` of its lines, against the whole source's
/// `scan` of `read`'s lines: a value that overlapped a redaction there becomes
/// the lines it came from as the scan left them, or a bare placeholder for a
/// span, or where a multi-line redaction emptied them. False when no scan could
/// run, and the value is withheld.
fn settle_with_whole_source(
    matched: &mut octocode_engine::types::TextSearchMatch,
    cut: &Cut<'_>,
    read: &VerificationLines,
    scan: &Option<Scanned>,
    match_only: bool,
) -> bool {
    let Some(Scanned { clean, redacted }) = scan else {
        matched.value = OVERSIZED_PLACEHOLDER.to_owned();
        return false;
    };
    let lines = read
        .retained
        .values()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let at = read.retained.range(..matched.line as usize).count();
    let own = (at < lines.len()).then(|| placed(&lines, at, cut));
    let shown = own
        .as_ref()
        .map_or(!redacted.is_empty(), |own| exposes(redacted, own));
    if shown {
        let span = matched.value.lines().count().max(1);
        let lines = clean
            .split('\n')
            .skip((matched.line as usize).saturating_sub(1))
            .take(span)
            .collect::<Vec<_>>()
            .join("\n");
        matched.value = if match_only {
            own.map_or_else(
                || REDACTED.to_owned(),
                |own| shown_span(redacted.runs(clean), &own, cut),
            )
        } else if lines.trim().is_empty() {
            "[REDACTED]".to_owned()
        } else {
            lines
        };
    }
    true
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
        settle_rows(
            &mut rows,
            &[(1, 1)],
            &judged,
            std::path::Path::new("/src/a.txt"),
            &ContentSecurity::new(),
            (true, Some((known, [7; 32]))),
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
        settle_rows(
            &mut rows,
            &[(2, 5)],
            &judged,
            std::path::Path::new("/src/key.txt"),
            &ContentSecurity::new(),
            (true, None),
        );
        assert!(!rows[0].value.contains("Z9"), "{}", rows[0].value);
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
