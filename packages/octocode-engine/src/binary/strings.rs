//! Printable-string extraction over a raw byte buffer.
//!
//! Unlike GNU `strings -a` (ASCII only), this also recovers UTF-16LE/BE runs,
//! which dominate PE/Windows binaries — the correctness win that motivated the
//! native path. A single linear scan per encoding; no allocation until a run is
//! confirmed long enough.
//!
//! **Lossless scan pagination.** Rather than capping the scan at a fixed size
//! and discarding the tail, the caller scans a byte *window* `[base_offset,
//! base_offset+len)` and gets back a `next_scan_offset` cursor. When the window
//! does not reach EOF, runs that touch the window's trailing edge are dropped
//! and the cursor is rewound to a **safe break** — the byte after the last
//! position that cannot be part of any string in any encoding (a byte that is
//! neither printable ASCII nor `0x00`, which breaks ASCII *and* both UTF-16
//! interleavings). The next window re-scans from there, so no run is ever split
//! across a window boundary, with no duplicates.

use super::types::BinaryStrings;

/// Window size for a single scan pass. Generous (native reads are fast); the
/// caller pages the whole file by following `next_scan_offset`.
pub const SCAN_WINDOW: usize = 64 * 1024 * 1024;

/// Hard ceiling on how many runs we materialize, so a buffer that is almost
/// entirely printable cannot blow up memory. The true count is reported
/// separately.
const MAX_RUNS: usize = 200_000;

#[inline]
fn is_printable_ascii(b: u8) -> bool {
    // Graphic ASCII plus space and tab — matches the intent of `strings`.
    b == b'\t' || (0x20..=0x7e).contains(&b)
}

/// A byte that can never be part of a string in *any* supported encoding:
/// not printable ASCII (breaks ASCII runs) and not `0x00` (breaks both UTF-16
/// interleavings, whose only bytes are a printable low/high half or `0x00`).
/// The position just after the last such byte in a window is a safe cut point —
/// no run of any encoding can straddle it.
#[inline]
fn is_hard_break(b: u8) -> bool {
    b != 0 && !is_printable_ascii(b)
}

struct Found {
    offset: usize,
    end: usize,
    text: String,
}

/// Extract ASCII runs of at least `min_len` printable bytes.
fn scan_ascii(buf: &[u8], min_len: usize, out: &mut Vec<Found>) {
    let mut start = 0usize;
    let mut len = 0usize;
    for (i, &b) in buf.iter().enumerate() {
        if is_printable_ascii(b) {
            if len == 0 {
                start = i;
            }
            len += 1;
        } else {
            if len >= min_len {
                push_run(buf, start, i, out);
            }
            len = 0;
        }
        if out.len() >= MAX_RUNS {
            return;
        }
    }
    if len >= min_len {
        push_run(buf, start, buf.len(), out);
    }
}

fn push_run(buf: &[u8], start: usize, end: usize, out: &mut Vec<Found>) {
    let text = String::from_utf8_lossy(&buf[start..end]).into_owned();
    out.push(Found {
        offset: start,
        end,
        text,
    });
}

/// Extract UTF-16 runs. `lead_zero_first == true` scans big-endian (00 XX);
/// otherwise little-endian (XX 00). A run is a sequence of printable ASCII code
/// points encoded as 2-byte units; `min_len` is in characters.
fn scan_utf16(buf: &[u8], min_len: usize, lead_zero_first: bool, out: &mut Vec<Found>) {
    let n = buf.len();
    let mut i = 0usize;
    while i + 1 < n {
        let (hi, lo) = if lead_zero_first {
            (buf[i], buf[i + 1])
        } else {
            (buf[i + 1], buf[i])
        };
        // Candidate start: zero high byte + printable low byte.
        if hi == 0 && is_printable_ascii(lo) {
            let start = i;
            let mut chars = String::new();
            let mut j = i;
            while j + 1 < n {
                let (h, l) = if lead_zero_first {
                    (buf[j], buf[j + 1])
                } else {
                    (buf[j + 1], buf[j])
                };
                if h == 0 && is_printable_ascii(l) {
                    chars.push(l as char);
                    j += 2;
                } else {
                    break;
                }
            }
            if chars.len() >= min_len {
                out.push(Found {
                    offset: start,
                    end: j,
                    text: chars,
                });
                if out.len() >= MAX_RUNS {
                    return;
                }
            }
            i = j.max(i + 2);
        } else {
            i += 1;
        }
    }
}

/// Length of the trailing run of bytes that could still be part of a string
/// continuing past the window. The window can be safely cut at `len - this`.
///
/// A safe break — a position no run of any encoding can straddle — sits right
/// after a hard-break byte, **or** between two consecutive `0x00` bytes: a NUL
/// pair can't occur inside an ASCII run (NUL isn't printable) nor inside either
/// UTF-16 interleaving (which alternates a printable half with a single NUL), so
/// the gap between them breaks every encoding. This is what stops long NUL
/// padding — pervasive in real binaries — from being mistaken for a giant
/// string-in-progress and rewinding the scan cursor to a crawl.
fn live_suffix_len(buf: &[u8]) -> usize {
    let n = buf.len();
    let mut k = 0usize;
    while k < n {
        let idx = n - 1 - k;
        let b = buf[idx];
        if is_hard_break(b) {
            break;
        }
        if b == 0 && idx + 1 < n && buf[idx + 1] == 0 {
            break;
        }
        k += 1;
    }
    k
}

/// Scan one window `buf` (which begins at absolute `base_offset` in the file)
/// for printable ASCII + UTF-16 strings of at least `min_len`, longest-first.
///
/// `at_eof` is true when the window reaches the end of the file. When it does
/// not, runs touching the window's trailing edge are withheld and
/// `next_scan_offset` is set to a safe break so the next window re-scans them
/// whole — lossless, no duplicates. `include_offsets` prefixes each entry with
/// its **absolute** hex byte offset (mirrors `strings -t x`).
pub fn extract(
    buf: &[u8],
    min_len: usize,
    include_offsets: bool,
    base_offset: u32,
    at_eof: bool,
) -> BinaryStrings {
    let min_len = min_len.max(1);

    // Safe cut: drop any run that ends beyond it (it may continue past the
    // window) and rewind the cursor there. At EOF nothing continues, so keep
    // everything. The progress guard handles a window with no hard break at all
    // (e.g. an all-NUL or fully-printable span): advance past the whole window
    // rather than stalling.
    let cut = if at_eof {
        buf.len()
    } else {
        let c = buf.len() - live_suffix_len(buf);
        if c == 0 {
            buf.len()
        } else {
            c
        }
    };

    let next_scan_offset = if at_eof || cut >= buf.len() {
        if at_eof {
            None
        } else {
            // Advanced the whole window (no safe break inside it).
            Some(base_offset.saturating_add(buf.len() as u32))
        }
    } else {
        Some(base_offset.saturating_add(cut as u32))
    };

    let mut found: Vec<Found> = Vec::with_capacity(256);
    scan_ascii(buf, min_len, &mut found);
    if found.len() < MAX_RUNS {
        scan_utf16(buf, min_len, false, &mut found); // UTF-16LE
    }
    if found.len() < MAX_RUNS {
        scan_utf16(buf, min_len, true, &mut found); // UTF-16BE
    }

    // Withhold runs that extend past the safe cut — re-scanned by the next
    // window from `cut`, so they appear exactly once and never split.
    found.retain(|f| f.end <= cut);

    let total_found = found.len() as u32;

    // Longest-first surfaces the most meaningful strings; the byte offset is a
    // stable tiebreaker so output is deterministic.
    found.sort_by(|a, b| {
        b.text
            .len()
            .cmp(&a.text.len())
            .then(a.offset.cmp(&b.offset))
    });

    let strings = found
        .into_iter()
        .map(|f| {
            if include_offsets {
                format!("{:#010x}: {}", base_offset as usize + f.offset, f.text)
            } else {
                f.text
            }
        })
        .collect();

    BinaryStrings {
        strings,
        total_found,
        // `truncated` retained for back-compat = "more of the file remains to
        // scan" (follow `next_scan_offset`). It is now a lossless cursor, not a
        // data-loss flag.
        truncated: next_scan_offset.is_some(),
        next_scan_offset,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_ascii_runs_above_min_length() {
        let buf = b"\x00\x01hello world\x00ab\x00longer_string_here\xff";
        let r = extract(buf, 5, false, 0, true);
        assert!(r.strings.iter().any(|s| s == "hello world"));
        assert!(r.strings.iter().any(|s| s == "longer_string_here"));
        assert!(!r.strings.iter().any(|s| s == "ab"));
        assert_eq!(r.strings[0], "longer_string_here");
    }

    #[test]
    fn recovers_utf16le_strings_that_gnu_strings_misses() {
        let buf = b"\x57\x00\x49\x00\x44\x00\x45\x00\xff\xff";
        let r = extract(buf, 4, false, 0, true);
        assert!(r.strings.iter().any(|s| s == "WIDE"), "got {:?}", r.strings);
    }

    #[test]
    fn recovers_utf16be_strings() {
        let buf = b"\x00\x57\x00\x49\x00\x44\x00\x45\xff\xff";
        let r = extract(buf, 4, false, 0, true);
        assert!(r.strings.iter().any(|s| s == "WIDE"), "got {:?}", r.strings);
    }

    #[test]
    fn offsets_are_prefixed_when_requested() {
        let buf = b"\x00\x00abcdef";
        let r = extract(buf, 4, true, 0, true);
        assert_eq!(r.strings.len(), 1);
        assert!(
            r.strings[0].starts_with("0x00000002: abcdef"),
            "got {:?}",
            r.strings
        );
    }

    #[test]
    fn empty_when_nothing_meets_min_length() {
        let buf = b"\x00\x01\x02ab\x03";
        let r = extract(buf, 8, false, 0, true);
        assert_eq!(r.total_found, 0);
        assert!(r.strings.is_empty());
    }

    // ── scan pagination ─────────────────────────────────────────────────────

    #[test]
    fn at_eof_emits_all_and_no_cursor() {
        let buf = b"\x00complete_run\x01TRAILING";
        let r = extract(buf, 4, false, 0, true);
        assert!(r.strings.iter().any(|s| s == "complete_run"));
        assert!(r.strings.iter().any(|s| s == "TRAILING"));
        assert_eq!(r.next_scan_offset, None);
        assert!(!r.truncated);
    }

    #[test]
    fn mid_file_window_withholds_boundary_run_and_sets_cursor() {
        // 0x01 is a hard break; "TRAILING" touches the window edge and may
        // continue in the next window, so it is withheld and the cursor rewinds
        // to its start (the byte after the hard break).
        let buf = b"complete_run\x01TRAILING";
        let r = extract(buf, 4, false, 0, false);
        assert!(r.strings.iter().any(|s| s == "complete_run"));
        assert!(
            !r.strings.iter().any(|s| s == "TRAILING"),
            "boundary-touching run must be withheld: {:?}",
            r.strings
        );
        // "complete_run\x01" = 13 bytes; cursor rewinds to index 13.
        assert_eq!(r.next_scan_offset, Some(13));
        assert!(r.truncated);
    }

    #[test]
    fn absolute_offsets_include_base() {
        let buf = b"\x00\x00abcdef";
        let r = extract(buf, 4, true, 1000, true);
        assert!(
            r.strings[0].starts_with("0x000003ea: abcdef"),
            "got {:?}",
            r.strings
        );
    }

    #[test]
    fn paging_two_windows_is_lossless_with_no_duplicates() {
        // Full data; a run ("TRAILING_DATA") straddles the first window's edge.
        let full = b"complete_run\x01TRAILING_DATA";
        // Window 1: bytes [0, 21) — ends inside "TRAILING_DATA".
        let w1 = &full[0..21];
        let r1 = extract(w1, 4, false, 0, false);
        let next = r1.next_scan_offset.expect("more to scan") as usize;
        // Window 2: from the cursor to EOF.
        let w2 = &full[next..];
        let r2 = extract(w2, 4, false, next as u32, true);

        let mut all: Vec<String> = r1.strings.clone();
        all.extend(r2.strings.clone());
        // Lossless: both complete strings recovered, the straddling one whole.
        assert!(all.iter().any(|s| s == "complete_run"), "{all:?}");
        assert!(all.iter().any(|s| s == "TRAILING_DATA"), "{all:?}");
        // No duplicates across the two windows.
        assert_eq!(
            all.iter().filter(|s| *s == "complete_run").count(),
            1,
            "{all:?}"
        );
        assert_eq!(
            all.iter().filter(|s| *s == "TRAILING_DATA").count(),
            1,
            "{all:?}"
        );
        assert_eq!(r2.next_scan_offset, None);
    }

    #[test]
    fn nul_padding_does_not_rewind_the_cursor() {
        // Regression: a short head string followed by a long NUL pad (ubiquitous
        // in real binaries) must advance the cursor through the padding, not
        // rewind to just after the head — NUL pairs are a safe break.
        let mut buf = b"HEAD_STRING\n".to_vec();
        buf.resize(4096, 0); // 4KB of NUL padding
        let r = extract(&buf, 4, false, 0, false);
        assert!(r.strings.iter().any(|s| s == "HEAD_STRING"));
        let next = r.next_scan_offset.expect("more to scan");
        // Must advance essentially the whole window, not stall near the head.
        assert!(next >= 4000, "cursor stalled in NUL padding: next={next}");
    }

    #[test]
    fn all_printable_window_makes_progress() {
        // No hard break anywhere: must still advance past the whole window
        // rather than returning a cursor that stalls.
        let buf = b"abcdefghijklmnop";
        let r = extract(buf, 4, false, 0, false);
        assert_eq!(r.next_scan_offset, Some(buf.len() as u32));
    }
}
