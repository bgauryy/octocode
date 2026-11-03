use super::super::scan::{Limits, Scan, scan, scan_with};
use super::*;

/// The streamed scan of `text` under `limits`.
fn streamed(q: &LocalFetchQuery, text: &str, limits: Limits) -> Scan {
    let file = temp(text.as_bytes());
    scan_with(q, &opened(file.path()), 0, limits, &NeverCancel).expect("scan")
}

/// The scan counts exactly the hit lines the whole-file matcher selects, and
/// the source's lines, with a window within its bound.
fn assert_whole_file_answer(q: &LocalFetchQuery, text: &str, limits: Limits) {
    let found = streamed(q, text, limits);
    let (want, _) = crate::tools::source_view::extraction::match_lines(q, text).expect("whole");
    assert!(
        found.lines == want,
        "{} lines, want {}: {:?} vs {:?} under chunk {} overlap {} read {}",
        found.lines.len(),
        want.len(),
        found.lines.iter().take(8).collect::<Vec<_>>(),
        want.iter().take(8).collect::<Vec<_>>(),
        limits.chunk,
        limits.overlap,
        limits.read
    );
    assert_eq!(found.count, want.len());
    assert_eq!(found.total_lines, text.split_inclusive('\n').count());
    assert!(found.peak <= limits.text_bound(), "{}", found.peak);
}

fn limits(chunk: usize, overlap: usize, read: usize) -> Limits {
    Limits {
        chunk,
        overlap,
        read,
    }
}

const TINY: [(usize, usize, usize); 4] = [(64, 24, 7), (50, 32, 3), (100, 24, 64), (33, 30, 1)];

fn matching(patterns: &[&str], extra: serde_json::Value) -> LocalFetchQuery {
    let mut json = serde_json::json!({"path":"/unused","matchString":patterns});
    json.as_object_mut()
        .expect("object")
        .extend(extra.as_object().cloned().unwrap_or_default());
    query(json)
}

/// A match anywhere in a line far longer than the window is found, even
/// when it straddles a scan edge or a multi-byte character.
#[test]
fn a_match_in_one_giant_line_is_found_at_every_offset() {
    let q = matching(&["needle"], serde_json::json!({}));
    for filler in ["x", "é"] {
        for tail in ["", "\nneedle\n"] {
            for at in 0..=150 {
                let mut text = filler.repeat(at);
                text.push_str("needle");
                text.push_str(&filler.repeat(150 - at));
                text.push_str(tail);
                for (chunk, overlap, read) in TINY {
                    assert_whole_file_answer(&q, &text, limits(chunk, overlap, read));
                }
            }
        }
    }
}

/// A line far larger than the window never buffers whole: the window stays
/// within its bound while the match inside the line is still counted.
#[test]
fn a_giant_line_is_scanned_within_the_window_bound() {
    let line = 6 * 1024 * 1024;
    let mut text = "x".repeat(line / 2);
    text.push_str("needle");
    text.push_str(&"x".repeat(line / 2));
    let q = matching(&["needle"], serde_json::json!({}));
    let file = temp(text.as_bytes());
    let bound = Limits::of(&q).text_bound();
    assert!(bound < line / 2, "the bound is far below the line");
    let found = scan(&q, &opened(file.path()), 0, &NeverCancel).expect("scan");
    assert_eq!((found.count, found.total_lines), (1, 1));
    assert!(
        found.peak <= bound,
        "peak {} over bound {bound}",
        found.peak
    );
    let read = fetch(&file, &q, None);
    assert_eq!(read.selected_match_count, Some(1), "{read:?}");
}

/// Multiline literal and regex matches that cross scan edges are found once,
/// whichever way the lines fall against the edges.
#[test]
fn matches_across_scan_edges_are_found_exactly_once() {
    let cases = [
        (vec!["ab\nba"], serde_json::json!({})),
        (
            vec!["(?s)b.{0,12}a"],
            serde_json::json!({"regexEngine":"rust"}),
        ),
        (
            vec!["^ba", "ab$"],
            serde_json::json!({"regexEngine":"rust"}),
        ),
    ];
    for (patterns, extra) in cases {
        let q = matching(&patterns, extra);
        for prefix in ["", "zz\n", "z\n", "zzz\n"] {
            let mut text = prefix.to_owned();
            for line in 0..60 {
                text.push_str(if line % 2 == 0 {
                    "xxxxxxxxab\n"
                } else {
                    "baxxxxxxxx\n"
                });
            }
            for (chunk, overlap, read) in TINY {
                assert_whole_file_answer(&q, &text, limits(chunk, overlap, read));
            }
        }
    }
}

/// Random text, lines short or giant, matches the whole-file matcher.
#[test]
fn random_sources_match_the_whole_file_matcher() {
    let cases = [
        (vec!["a\nb"], serde_json::json!({})),
        (vec!["ab", "ba"], serde_json::json!({"wholeWord":true})),
        (
            vec!["(?s)a.{0,6}b"],
            serde_json::json!({"regexEngine":"rust"}),
        ),
        (vec!["^b", "a$"], serde_json::json!({"regexEngine":"rust"})),
    ];
    let mut seed = 0x2545_f491_4f6c_dd1d_u64;
    let mut next = move || {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        usize::try_from(seed >> 33).expect("small")
    };
    for newline_every in [3, 9, 400] {
        let text: String = (0..1_500)
            .map(|_| match next() % newline_every {
                0 => '\n',
                n if n % 5 == 1 => 'a',
                n if n % 5 == 2 => 'b',
                n if n % 5 == 3 => ' ',
                n if n % 5 == 4 => 'é',
                _ => 'x',
            })
            .collect();
        for (patterns, extra) in &cases {
            let q = matching(patterns, extra.clone());
            for (chunk, overlap, read) in TINY {
                assert_whole_file_answer(&q, &text, limits(chunk, overlap, read));
            }
        }
    }
}

/// A match page reads its source once: the scan proves the digest, counts
/// every hit and finds the page's windows, which are then read from the
/// scan's nearest line checkpoint rather than by a second pass from the
/// file's start. Hits sit at both ends, so the windows span the file.
#[test]
fn a_match_page_reads_its_source_once() {
    let lines = 80_000;
    let mut text = String::with_capacity(lines * 100);
    for line in 1..=lines {
        if line == 3 || line == lines - 2 {
            text.push_str(&format!("NEEDLE{}\n", "y".repeat(93)));
        } else {
            text.push_str(&format!("{}\n", "x".repeat(99)));
        }
    }
    let file = temp(text.as_bytes());
    let q = matching(
        &["NEEDLE"],
        serde_json::json!({"path":file.path(),"contextLines":{"before":1,"after":1}}),
    );
    let before = super::super::source_bytes_read();
    let page = fetch(&file, &q, None);
    assert_eq!(page.error_code, None, "{page:?}");
    assert_eq!(page.matched_lines, vec![3, lines - 2]);
    assert_eq!(page.selected_match_count, Some(2));
    assert_eq!(page.total_lines, Some(lines));
    assert_eq!(
        page.source_sha256.as_deref(),
        Some(hex::encode(Sha256::digest(text.as_bytes())).as_str())
    );
    let content = page.content.as_deref().unwrap_or_default();
    assert!(content.contains("NEEDLE"), "{content}");
    let read = super::super::source_bytes_read() - before;
    assert!(
        read <= text.len() + 2 * Limits::of(&q).chunk,
        "read {read} bytes for a {}-byte source",
        text.len()
    );
}

/// Walking every page of a multiline match read over a source larger than
/// the production scan chunk reaches each hit line once, with none missing:
/// one match sits where a line-aligned chunk of the source would end.
#[test]
fn a_page_walk_across_the_scan_chunk_edge_reconstructs_every_hit_line_once() {
    let line_bytes = 1_005;
    let file_lines = 2_600;
    let mut q = matching(
        &["ab\nba"],
        serde_json::json!({"contextLines":{"before":0,"after":0}}),
    );
    let edge_line = Limits::of(&q).chunk.div_ceil(line_bytes);
    let text: String = (0..file_lines)
        .map(|line| {
            let opens = line % 8 == 0 || line + 1 == edge_line;
            let closes = line % 8 == 1 || line == edge_line;
            format!(
                "{}{}{}\n",
                if closes { "ba" } else { "yy" },
                "y".repeat(line_bytes - 5),
                if opens { "ab" } else { "yy" }
            )
        })
        .collect();
    let file = temp(text.as_bytes());
    q = local_fetch_query::updated(&q, serde_json::json!({"path":file.path()})).expect("path");
    let (want, _) =
        crate::tools::source_view::extraction::match_lines(&q, &text).expect("whole-file hits");
    assert!(want.contains(&edge_line) && want.contains(&(edge_line + 1)));
    let mut pending = std::collections::VecDeque::from([q]);
    let (mut seen, mut pages) = (Vec::new(), 0);
    while let Some(call) = pending.pop_front() {
        let page = fetch(&file, &call, Some(64 * 1024));
        assert_eq!(page.error_code, None, "{page:?}");
        if let Some(count) = page.selected_match_count {
            assert_eq!(count, want.len());
        }
        for range in &page.source_line_ranges {
            seen.extend(range.start..=range.end);
        }
        assert!(
            page.matched_lines.iter().all(|line| want.contains(line)),
            "a matched line is a hit line"
        );
        pages += 1;
        assert!(pages < 100, "runaway paging");
        if let Some(next) = page.next {
            for call in [next.continue_window, next.read_other_ranges]
                .into_iter()
                .flatten()
            {
                pending.push_back(call.query);
            }
        }
    }
    assert!(pages > 1, "the walk pages");
    seen.sort_unstable();
    assert_eq!(seen, want, "every hit line served once over {pages} pages");
}
