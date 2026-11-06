//! One line of a source walked in byte pages: every byte reached once, with a
//! secret's bytes hidden wherever a page or a window edge falls. Every value is fake.
use super::*;

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
    tokens.extend((1..4).map(|at| at * super::super::BYTE_LOOKBEHIND - 21));
    tokens.extend((1..4).map(|at| at * super::super::BYTE_WINDOW_BYTES - 21));
    // Far enough apart for each to stand alone.
    tokens.sort_unstable();
    tokens.dedup_by(|later, kept| *later - *kept < 2 * TOKEN.len());
    // Long enough to hold every edge the tokens sit on.
    let size = past_whole().max(4 * super::super::BYTE_WINDOW_BYTES);
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
    let first = fetch(
        &file,
        &query(serde_json::json!({"path":file.path(),"ranges":[{"startLine":1,"endLine":1}]})),
        None,
    );
    let digest = first.source_sha256.clone().expect("digest");
    // Past the settle window the pass is kept, so a later page proves nothing again.
    std::thread::sleep(std::time::Duration::from_millis(2_100));
    let first = fetch(
        &file,
        &query(
            serde_json::json!({"path":file.path(),"ranges":[{"startLine":1,"endLine":1}],"snapshot":digest}),
        ),
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
        read < 2 * super::super::BYTE_WINDOW_BYTES,
        "{read} bytes read for one page"
    );
}

/// A page's margin is sized by the detectors: no bounded secret can reach past it.
#[test]
fn a_page_margin_outreaches_every_bounded_secret() {
    const { assert!(super::super::BYTE_LOOKBEHIND >= octocode_engine::security::LONGEST_BOUNDED_MATCH) };
}
