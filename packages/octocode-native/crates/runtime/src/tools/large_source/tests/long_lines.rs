//! One line of a source walked in byte pages: every byte reached once, with a
//! secret's bytes hidden wherever a page or a window edge falls. Every value is fake.
use super::*;
use octocode_contract_types::ReadProgressUnit;

const TOKEN: &str = "ghp_aB3dE5gH7jK9mN1pQ3sT5vX7zA9cE1gI3kM5";

fn line_of(size: usize, tokens: &[usize]) -> Vec<u8> {
    let mut text = b"word ".repeat(size / 5 + 1);
    text.truncate(size);
    for &at in tokens {
        // A token stands alone, as an assignment's value does.
        text[at..at + 7].copy_from_slice(b" token=");
        text[at + 7..at + 7 + TOKEN.len()].copy_from_slice(TOKEN.as_bytes());
        text[at + 7 + TOKEN.len()] = b' ';
    }
    text.push(b'\n');
    text
}

/// Walk `file`'s first line by `nextPage`, checking every page; its text and pages.
fn walk(file: &tempfile::NamedTempFile) -> (String, usize) {
    let mut next =
        query(serde_json::json!({"path":file.path(),"ranges":[{"startLine":1,"endLine":1}]}));
    let mut shown = String::new();
    let mut expected = 0;
    let mut pages = 0;
    loop {
        let page = fetch(file, &next, None);
        assert_eq!(page.status, ReadStatus::Success, "{page:?}");
        let content = page.content.expect("a page");
        assert!(
            !content.contains(&TOKEN[8..16]),
            "a secret's part at raw byte {expected}"
        );
        assert!(!content.contains("CONTENT-REDACTED"));
        if let Some(bytes) = page.pagination {
            assert_eq!(bytes.offset, expected, "no gap and no repeat");
            expected += bytes.length;
        }
        shown.push_str(&content);
        pages += 1;
        let Some(call) = page.next.and_then(|next| next.continue_window) else {
            return (shown, pages);
        };
        next = call.query;
    }
}

/// With no secret, the walk is the line, byte for byte, in pages of any place.
#[test]
fn a_long_line_is_reached_once_over_every_page() {
    let page = crate::tools::source_view::pagination::LINE_PAGE_BYTES;
    for size in [LIMITS.whole_source, past_whole()] {
        let text = line_of(size, &[]);
        let file = temp(&text);
        let (shown, pages) = walk(&file);
        assert!(pages >= size / page, "{pages} pages of {size} bytes");
        assert_eq!(shown.len(), text.len(), "{size}");
        assert_eq!(shown.as_bytes(), text.as_slice(), "{size}");
    }
}

/// Tokens placed across every page edge and window edge a walk meets: none shows,
/// all the words between them do, and the pages stay contiguous.
#[test]
fn no_value_leaks_at_a_page_or_window_edge() {
    let page = crate::tools::source_view::pagination::LINE_PAGE_BYTES;
    let mut tokens = (1..40).map(|at| at * page - 20 + at).collect::<Vec<_>>();
    tokens.extend((1..3).map(|at| at * LIMITS.window - 21));
    // And every edge the pass that records the redactions judges at.
    let judged = super::super::sealed::Limits::REAL;
    tokens.extend((1..3).map(|at| at * judged.chunk - 21));
    tokens.extend((1..3).map(|at| at * judged.chunk + judged.overlap - 21));
    tokens.extend((1..4).map(|at| at * super::super::BYTE_WINDOW_BYTES - 21));
    // Far enough apart for each to stand alone.
    tokens.sort_unstable();
    tokens.dedup_by(|later, kept| *later - *kept < 2 * TOKEN.len());
    // Long enough to hold every edge the tokens sit on.
    let size = past_whole()
        .max(4 * super::super::BYTE_WINDOW_BYTES)
        .max(2 * judged.chunk + judged.overlap + TOKEN.len());
    let text = line_of(size, &tokens);
    let file = temp(&text);
    let (shown, _) = walk(&file);
    // Words and placeholders only: the rest is the source's, once.
    let raw = String::from_utf8(text).expect("utf8");
    let expected = raw.replace(TOKEN, "[REDACTED-GITHUBTOKENS]");
    if shown != expected {
        let at = shown
            .bytes()
            .zip(expected.bytes())
            .position(|(a, b)| a != b)
            .unwrap_or(shown.len().min(expected.len()));
        let from = at.saturating_sub(60);
        panic!(
            "at {at} of {}/{}: shown {:?} expected {:?}",
            shown.len(),
            expected.len(),
            &shown[from..(at + 60).min(shown.len())],
            &expected[from..(at + 60).min(expected.len())]
        );
    }
}

/// A source with no secret costs each later page only the bytes around it: a walk
/// of one line reads it about once, whatever its length.
#[test]
fn a_later_page_reads_only_around_itself() {
    let text = line_of(past_whole(), &[]);
    let file = temp(&text);
    // A settled file's pass is kept, so a later page proves nothing again.
    octocode_engine::stamp::settle_written_so_far();
    let first = fetch(
        &file,
        &query(serde_json::json!({"path":file.path(),"ranges":[{"startLine":1,"endLine":1}]})),
        None,
    );
    let mut next = first
        .next
        .and_then(|next| next.continue_window)
        .expect("more")
        .query;
    for _ in 0..20 {
        let page = fetch(&file, &next, None);
        next = page
            .next
            .and_then(|next| next.continue_window)
            .expect("more")
            .query;
    }
    let before = source_bytes_read();
    let page = fetch(&file, &next, None);
    assert_eq!(page.status, ReadStatus::Success);
    let read = source_bytes_read() - before;
    assert!(
        read <= super::super::proven::CHECKPOINT_BYTES as usize
            + super::super::BYTE_WINDOW_BYTES
            + super::super::proven::READ_BYTES,
        "{read} bytes read for one page"
    );
}

/// The recorder's overlap is sized by the detectors: a bounded secret that starts
/// before a judged edge ends inside the text judged past it.
#[test]
fn the_recorder_overlap_outreaches_every_bounded_secret() {
    const {
        assert!(
            super::super::sealed::Limits::REAL.overlap
                >= octocode_engine::security::LONGEST_BOUNDED_MATCH
        );
    };
}

/// A secret far longer than any page, its value across many page and window
/// edges, in a 12 MiB source: the walk shows its placeholder once and none of its
/// bytes, every other byte exactly once, on contiguous sealed offsets.
#[test]
fn a_secret_longer_than_a_window_never_shows_across_its_pages() {
    let token: String = (0..600 * 1024)
        .map(|at| char::from(b"aZ3kQ9xV7mP2wR5t"[at % 16]))
        .collect();
    let header = format!("h=\"Authorization: Bearer {token}\";");
    let mut text = b"word ".repeat(12 * 1024 * 1024 / 5);
    let at = text.len() / 2;
    text.splice(at..at, header.bytes());
    text.push(b'\n');
    let file = temp(&text);
    // A settled file's pass is kept: later pages read from it.
    octocode_engine::stamp::settle_written_so_far();
    let mut next =
        query(serde_json::json!({"path":file.path(),"ranges":[{"startLine":1,"endLine":1}]}));
    let (mut shown, mut expected, mut pages) = (String::new(), 0, 0);
    loop {
        let page = fetch(&file, &next, Some(LIMITS.window));
        assert_eq!(page.status, ReadStatus::Success, "page {pages}: {page:?}");
        let content = page.content.expect("a page");
        assert!(
            !content.contains(&token[..64]),
            "page {pages} shows the token"
        );
        let bytes = page.pagination.expect("byte pages");
        assert_eq!(bytes.offset, expected, "page {pages}: no gap and no repeat");
        expected += bytes.length;
        shown.push_str(&content);
        pages += 1;
        let Some(call) = page.next.and_then(|next| next.continue_window) else {
            break;
        };
        next = call.query;
    }
    let raw = String::from_utf8(text).expect("ASCII");
    let sealed = raw.replace(
        &token,
        &octocode_engine::security::redaction_placeholder("bearerAuthHeader"),
    );
    assert!(pages > 2, "{pages} pages");
    assert_eq!(shown.len(), sealed.len(), "the sealed view, once");
    assert!(shown == sealed, "the sealed view, byte for byte");
}

/// Short secrets (a one-character session id shows as a longer placeholder)
/// on one streamed line: the pages walk the sealed view, grown past the
/// source's length, contiguously and to its end, as an in-memory read seals
/// it (review: a growing edit was counted as none, so totals and
/// continuation offsets fell short of the sealed bytes).
#[test]
fn growing_placeholders_are_walked_once_to_the_sealed_end() {
    let object = "JSESSIONID=v; ";
    let mut raw = String::from("Cookie: ");
    while raw.len() < past_whole() {
        raw.push_str(object);
    }
    raw.push_str("end é日🦀\n");
    let file = temp(raw.as_bytes());
    let sealed = crate::tools::source_view::prepare::sealed_view(
        raw.as_bytes(),
        file.path(),
        &crate::security::ContentSecurity::new(),
    )
    .expect("sealed")
    .text
    .into_owned();
    assert!(sealed.len() > raw.len(), "the placeholders grow the view");
    assert!(!sealed.contains("JSESSIONID=v;"));
    let (shown, pages) = walk(&file);
    assert!(pages > 2, "{pages} pages");
    assert_eq!(shown.len(), sealed.len(), "the sealed view, once");
    assert!(shown == sealed, "the sealed view, byte for byte");
}

/// Walk lines `first..=last` of `file` by every emitted continuation to the
/// end: the pages' text joined, with each byte page's offsets contiguous inside
/// its line. `between` runs before each later page and may change the file.
fn walk_lines(
    file: &tempfile::NamedTempFile,
    (first, last): (usize, usize),
    mut between: impl FnMut(usize),
) -> Result<String, Box<LocalFetchResult>> {
    let mut next = query(
        serde_json::json!({"path":file.path(),"ranges":[{"startLine":first,"endLine":last}]}),
    );
    let mut shown = String::new();
    let mut expected: Option<usize> = None;
    for at in 0.. {
        if at > 0 {
            between(at);
        }
        let page = fetch(file, &next, None);
        if page.status != ReadStatus::Success {
            return Err(Box::new(page));
        }
        let content = page.content.clone().expect("a page");
        match &page.pagination {
            Some(bytes) if bytes.unit == ReadProgressUnit::Bytes => {
                if let Some(expected) = expected {
                    assert_eq!(bytes.offset, expected, "page {at}: no gap and no repeat");
                }
                expected = bytes.has_more.then_some(bytes.offset + bytes.length);
            }
            _ => expected = None,
        }
        shown.push_str(&content);
        let Some(call) = page.next.and_then(|next| next.continue_window) else {
            return Ok(shown);
        };
        next = call.query;
        // A byte page that ends its line hands back to the line walk: the next
        // byte page counts in the bytes of the lines it starts at.
        if local_fetch_query::unit(&next) == ReadProgressUnit::Lines {
            expected = None;
        }
    }
    unreachable!("the walk ends")
}

/// `size` bytes of text that mixes 1- to 4-byte characters, so every page
/// edge a walk meets falls near a character's inner byte.
fn multibyte(size: usize) -> String {
    let unit = "aé日🦀 ";
    let mut text = unit.repeat(size / unit.len() + 1);
    text.truncate(text.floor_char_boundary(size));
    text
}

/// B86: a long line of multibyte characters, walked in byte pages to its end,
/// joins back to the source byte for byte: no page splits a character.
#[test]
fn a_multibyte_long_line_walks_to_its_end_byte_exact() {
    let text = format!("{}\n", multibyte(past_whole()));
    let file = temp(text.as_bytes());
    let shown = walk_lines(&file, (1, 1), |_| {}).expect("every page served");
    assert_eq!(shown.len(), text.len());
    assert!(shown == text, "the line, byte for byte");
}

/// B86: CRLF lines, two of them long, walked by line and by byte page to the
/// read's end: every `\r\n` arrives whole and once.
#[test]
fn crlf_long_lines_walk_to_their_end_byte_exact() {
    let long = |tag: &str| format!("{tag} {}\r\n", multibyte(past_whole() / 2 + 4097));
    let text = format!(
        "{}{}short\r\n{}end\r\n",
        long("one"),
        long("two"),
        long("three")
    );
    let file = temp(text.as_bytes());
    let total = text.matches('\n').count();
    let shown = walk_lines(&file, (1, total), |_| {}).expect("every page served");
    assert_eq!(shown.matches("\r\n").count(), total);
    assert_eq!(shown.len(), text.len());
    assert!(shown == text, "the lines, byte for byte");
}

/// B86: the source changes between two pages of a walk (rewritten in place
/// at the same length, or grown). The next page is refused as stale: no page
/// after the change shows a byte of the new source beside the old.
#[test]
fn a_source_changed_mid_walk_is_refused_as_stale_never_mixed() {
    let before = format!("{}\n", multibyte(past_whole()));
    let after_same = before.replace('a', "b");
    let after_grown = format!("{before}{}\n", "c".repeat(64));
    for after in [after_same, after_grown] {
        let file = temp(before.as_bytes());
        let mut changed = false;
        let walked = walk_lines(&file, (1, 1), |at| {
            if at == 2 {
                std::fs::write(file.path(), after.as_bytes()).expect("rewrite");
                changed = true;
            }
        });
        assert!(changed, "the walk reached its third page");
        let refused = walked.expect_err("a stale page is refused");
        assert_eq!(
            refused.error_code,
            Some(octocode_contract_types::ErrorCode::StaleSnapshot),
            "{refused:?}"
        );
        assert!(refused.content.is_none(), "{refused:?}");
    }
}

/// S3: a whole-file read (no `ranges`) of a streamed source with a line too long
/// for a line page inside a later window, walked by every emitted continuation:
/// the long line's byte pages hand back to the line walk, so the pages join to
/// the source byte for byte and every line is reached exactly once, to the
/// file's last line (review: the byte pages took the window's lines as their
/// range and ended there, `hasMore:false`, with every later line unreachable).
#[test]
fn a_whole_file_walk_reaches_every_line_once_across_a_long_line() {
    let mut text = String::new();
    for line in 1..=7 {
        text.push_str(&format!("head line {line}\n"));
    }
    text.push_str(&"x".repeat(BYTE_WINDOW_BYTES + STRETCH));
    text.push('\n');
    let mut line = 9;
    while text.len() < past_whole() || line < 2 * WINDOW_LINES + 9 {
        text.push_str(&format!("row {line:07} {}\n", "y".repeat(40)));
        line += 1;
    }
    let total = line - 1;
    let file = temp(text.as_bytes());
    // A settled file's pass is kept: later pages read from it.
    octocode_engine::stamp::settle_written_so_far();
    let mut next = query(serde_json::json!({"path":file.path()}));
    let (mut shown, mut lines_paged, mut bytes_paged) = (String::new(), 0, 0);
    let mut last_line = 0;
    for at in 0.. {
        let page = fetch(&file, &next, None);
        assert_eq!(page.status, ReadStatus::Success, "page {at}: {page:?}");
        let content = page.content.clone().expect("a page");
        let pagination = page.pagination.as_ref().expect("paged");
        if pagination.unit == ReadProgressUnit::Lines {
            lines_paged += 1;
            let (start, end) = (
                page.start_line.expect("a line page's first line"),
                page.end_line.expect("a line page's last line"),
            );
            assert_eq!(start, last_line + 1, "page {at}: lines follow on, once");
            last_line = end;
            assert_eq!(page.total_lines, Some(total), "page {at}");
        } else {
            bytes_paged += 1;
            assert_eq!(page.total_lines, Some(total), "page {at}");
            last_line = 8;
        }
        shown.push_str(&content);
        let Some(call) = page.next.and_then(|next| next.continue_window) else {
            assert!(!pagination.has_more, "page {at}: no hasMore without a call");
            break;
        };
        assert!(pagination.has_more, "page {at}: a call only with hasMore");
        next = call.query;
    }
    assert!(
        lines_paged > 2 && bytes_paged > 2,
        "{lines_paged} line pages, {bytes_paged} byte pages"
    );
    assert_eq!(last_line, total, "the walk reaches the file's last line");
    assert_eq!(shown.len(), text.len(), "every byte once");
    assert!(shown == text, "the source, byte for byte");
}
