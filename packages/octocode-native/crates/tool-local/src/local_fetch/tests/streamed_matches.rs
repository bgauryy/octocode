//! A match read of a line too long for one byte page, in a source past the
//! whole-file ceiling: the line's matches lead its byte walk, as they do in a
//! source held whole, so the read a search row hands off for a hit in a huge
//! one-line file opens at the hit, at every size.
use super::*;
use octocode_contract_types::ReadProgressUnit;
use octocode_test_support::json_fixture;

/// `len` bytes of one line whose every 9-byte record is its own position (a
/// page read from the wrong place cannot equal the line's slice), with each of
/// `marks` written over the filler at its byte.
fn positional_line(len: usize, marks: &[(usize, &str)]) -> String {
    let mut line = (0..len.div_ceil(9))
        .map(|at| format!("{at:08};"))
        .collect::<String>()
        .into_bytes();
    line.truncate(len);
    for &(at, mark) in marks {
        line[at..at + mark.len()].copy_from_slice(mark.as_bytes());
    }
    String::from_utf8(line).expect("ascii")
}

/// A fake token the real scanner hides, standing alone as an assignment's
/// value does.
const TOKEN: &str = "ghp_aB3dE5gH7jK9mN1pQ3sT5vX7zA9cE1gI3kM5";
const PLACEHOLDER: &str = "[REDACTED-GITHUBTOKENS]";

/// The scanner a read of these sources runs: the real one, judging sources at
/// [`octocode_runtime_base::security::SourceLimits::SMALL`].
fn scanner() -> octocode_runtime_base::security::ContentSecurity {
    octocode_runtime_base::security::ContentSecurity::with_limits(
        octocode_runtime_base::security::SourceLimits::SMALL,
    )
}

/// What a read shows of `line`: its tokens hidden, and its line break.
fn shown(line: &str) -> String {
    format!("{line}\n").replace(TOKEN, PLACEHOLDER)
}

/// Hits far apart, two close together, one near the line's end, and a token on
/// each side of every hit, its placeholder across the edge of the hit's match
/// window ([`MATCH_MARGIN`] around the match: 70 shown bytes between the
/// placeholder and the hit puts it over that edge); more tokens every 4,099
/// bytes, so page and window edges of the rest of the line meet them.
fn fixture_line(len: usize) -> (String, usize) {
    use octocode_tools_shared::source_view::extraction::MATCH_MARGIN;
    const GAP: usize = 70;
    const {
        assert!(GAP < MATCH_MARGIN && GAP + PLACEHOLDER.len() > MATCH_MARGIN);
    }
    let token = format!(" token={TOKEN} ");
    // The token's bytes before and after its value.
    let (lead, trail) = (" token=".len(), 1);
    let hits = [len / 3, len / 3 + 1_000, 2 * len / 3, len - 400];
    let mut marks = Vec::new();
    for &hit in &hits {
        marks.push((hit - GAP + trail - token.len(), token.as_str()));
        marks.push((hit, "NEEDLE"));
        marks.push((hit + "NEEDLE".len() + GAP - lead, token.as_str()));
    }
    let near = |at: usize| hits.iter().any(|&hit| at + 400 > hit && at < hit + 400);
    let periodic = (1..len / 4_099)
        .map(|step| step * 4_099)
        .filter(|&at| !near(at))
        .collect::<Vec<_>>();
    marks.extend(periodic.iter().map(|&at| (at, token.as_str())));
    (positional_line(len, &marks), hits.len())
}

/// A source past the whole-file ceiling is one line: a read of its matches
/// shows the first hit on page one, every hit before the rest of the line,
/// and every byte of the line exactly once, no token shown and no
/// placeholder cut by a match window, a page or a streamed window.
#[test]
fn a_streamed_long_line_shows_its_matches_first_and_every_byte_once() {
    let limits = octocode_runtime_base::security::SourceLimits::SMALL;
    let len = limits.whole_source + limits.whole_source / 4;
    let (line, hits) = fixture_line(len);
    let fixture = Fixture::new();
    let path = fixture.write("bundle.min.js", format!("{line}\n"));
    let paths = Paths(fixture.0.clone());
    let view = shown(&line);
    let first_hit = view.find("NEEDLE").expect("a hit");
    for query in [
        qj(&path, json_fixture(r#"{"matchString": ["NEEDLE"]}"#)),
        // The read a search row hands off for a window row: the hit's line.
        qj(
            &path,
            json_fixture(
                r#"{
                    "matchString": ["NEEDLE"],
                    "ranges": [{"startLine": 1, "endLine": 1}]
                }"#,
            ),
        ),
    ] {
        let read = fetch_with(&query, &paths, &scanner());
        assert_eq!(read.status, ReadStatus::Success, "{read:?}");
        let page = read.pagination.clone().expect("a byte page");
        assert_eq!(page.unit, ReadProgressUnit::Bytes, "{page:?}");
        let text = read.content.as_deref().expect("content");
        assert!(
            text.starts_with(&view[page.offset..page.offset + 20])
                && page.offset <= first_hit
                && first_hit < page.offset + text.len(),
            "page one opens at the first hit, byte {first_hit}: {page:?}"
        );
        assert!(
            read.warnings.iter().any(|w| w.starts_with("Large source")),
            "the read is streamed: {:?}",
            read.warnings
        );
        let pages = walk_exactly_once_with(&query, &paths, &scanner(), None, &view);
        assert!(
            pages.iter().all(|(_, text)| !text.contains(&TOKEN[8..16])),
            "no page shows a token's part"
        );
        let leading = pages
            .iter()
            .take_while(|(_, text)| text.contains("NEEDLE"))
            .map(|(_, text)| text.matches("NEEDLE").count())
            .sum::<usize>();
        assert_eq!(
            leading, hits,
            "every hit is shown before the rest of the line"
        );
    }
}

/// A later page of a settled source comes from its kept pass and hits: the
/// same pages a fresh scan of the same bytes gives.
#[test]
fn a_streamed_match_walk_from_kept_hits_is_the_fresh_walk() {
    let limits = octocode_runtime_base::security::SourceLimits::SMALL;
    let (line, _) = fixture_line(limits.whole_source + limits.whole_source / 8);
    let fixture = Fixture::new();
    let settled = fixture.write("settled.min.js", format!("{line}\n"));
    octocode_engine::stamp::settle_written_so_far();
    let fresh = fixture.write("fresh.min.js", format!("{line}\n"));
    let paths = Paths(fixture.0.clone());
    let view = shown(&line);
    let walk = |path: &Path| {
        let query = qj(path, json_fixture(r#"{"matchString": ["NEEDLE"]}"#));
        walk_exactly_once_with(&query, &paths, &scanner(), None, &view)
    };
    let kept = walk(&settled);
    assert!(kept[0].1.contains("NEEDLE"), "the walk opens at a hit");
    assert_eq!(kept, walk(&fresh));
}

/// The read a search row hands off for a hit deep in a one-line file past the
/// whole-file ceiling opens at the hit, and goes on.
#[test]
fn the_lead_of_a_hit_deep_in_a_one_line_file_past_the_ceiling_opens_at_the_hit() {
    let ceiling = SmallSafe.limits().whole_source;
    let len = ceiling + ceiling / 16;
    let hit = len - len / 10;
    let line = positional_line(len, &[(hit, "NEEDLE")]);
    let fixture = Fixture::new();
    let path = fixture.write("dump.json", format!("{line}\n"));
    let paths = Paths(fixture.0.clone());
    let query = qj(
        &path,
        json_fixture(
            r#"{
                "matchString": ["NEEDLE"],
                "ranges": [{"startLine": 1, "endLine": 1}]
            }"#,
        ),
    );
    let read = fetch_with(&query, &paths, &SmallSafe);
    assert_eq!(read.status, ReadStatus::Success, "{read:?}");
    let text = read.content.as_deref().expect("content");
    assert!(text.contains("NEEDLE"), "page one shows the hit");
    let page = read.pagination.clone().expect("a byte page");
    assert_eq!(&line[page.offset..page.offset + text.len()], text);
    let next = read
        .next
        .and_then(|next| next.continue_window)
        .expect("the rest of the line");
    let rest = fetch_with(&next.query, &paths, &SmallSafe);
    assert_eq!(rest.status, ReadStatus::Success, "{rest:?}");
    let rest_page = rest.pagination.expect("a byte page");
    assert_eq!(
        rest_page.offset, 0,
        "the rest starts at the line's first byte"
    );
}

/// A scanner whose placeholder no placeholder pattern knows (the test scan's
/// `[REDACTED]`): a match read of a one-line source past the ceiling still
/// counts every hit, opens at the first, and walks every byte once.
#[test]
fn a_streamed_match_read_finds_every_hit_whatever_placeholder_its_scanner_writes() {
    let limits = SmallSafe.limits();
    let len = limits.whole_source + limits.whole_source / 4;
    let hits = [len / 5, 2 * len / 5, 3 * len / 5, 4 * len / 5, len - 300];
    let near = |at: usize| hits.iter().any(|&hit| at + 400 > hit && at < hit + 400);
    let marks = (1..len / 4_099)
        .map(|step| step * 4_099)
        .filter(|&at| !near(at))
        .map(|at| (at, "SECRET"))
        .chain(hits.iter().map(|&at| (at, "NEEDLE")))
        .collect::<Vec<_>>();
    let line = positional_line(len, &marks);
    let fixture = Fixture::new();
    let path = fixture.write("bundle.min.js", format!("{line}\n"));
    let paths = Paths(fixture.0.clone());
    let view = format!("{line}\n").replace("SECRET", "[REDACTED]");
    let query = qj(&path, json_fixture(r#"{"matchString": ["NEEDLE"]}"#));
    let read = fetch_with(&query, &paths, &SmallSafe);
    assert_eq!(read.status, ReadStatus::Success, "{read:?}");
    assert_eq!(
        read.selected_match_count,
        Some(hits.len()),
        "every hit is found"
    );
    let pages = walk_exactly_once_with(&query, &paths, &SmallSafe, None, &view);
    assert!(pages[0].1.contains("NEEDLE"), "the walk opens at a hit");
}
