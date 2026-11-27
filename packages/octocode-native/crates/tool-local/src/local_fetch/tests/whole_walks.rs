//! A whole read's walk (no ranges, no matches, content or compact view)
//! replays the view its first page built, and every replayed page is still
//! cut and scanned as a first page is: each page equals a cold read of that
//! page, and only a secret's own bytes are redacted.
use super::continuations::Counting;
use super::*;

const TOKEN: &str = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";

/// A private-key block whose body lines carry `tag`.
fn key_block(tag: &str) -> String {
    let mut block = String::from("-----BEGIN RSA PRIVATE KEY-----\n");
    for n in 0..40 {
        block.push_str(&format!(
            "MIIEpQIBAAKCAQEA{tag}{n:04}AAAAAAAAAAAAAAAAAAAAAAAAAAAA\n"
        ));
    }
    block.push_str("-----END RSA PRIVATE KEY-----\n");
    block
}

/// A token inside a line longer than any page: it reads in byte pages.
fn long_line() -> String {
    format!(
        "{}token={TOKEN};{}\n",
        "x".repeat(40_000),
        "y".repeat(40_000)
    )
}

/// Secrets on the first page and on late pages: a key block at the top,
/// another after thousands of lines, then a long line holding a token.
/// `test` makes each test's bytes its own: a kept view is keyed by them.
fn source(test: &str) -> String {
    let mut source = format!("{test}\n");
    source.push_str(&key_block("earlyKeyBody"));
    source.extend((1..=4_000).map(|n| format!("plain filler line {n}\n")));
    source.push_str(&key_block("lateKeyBody"));
    source.push_str(&long_line());
    source.extend((1..=2_000).map(|n| format!("closing filler line {n}\n")));
    source
}

/// Every page of `query` at `window`, each with the query that read it.
fn walk(
    query: &LocalFetchQuery,
    paths: &Paths,
    scan: &impl ContentScan,
    window: usize,
) -> Vec<(LocalFetchQuery, LocalFetchResult)> {
    let mut pages = Vec::new();
    let mut query = query.clone();
    loop {
        let page = execute_local_fetch(&query, paths, scan, &NeverCancel, Some(window), false);
        assert_eq!(page.error_code, None, "{page:?}");
        let next = page
            .next
            .as_ref()
            .and_then(|next| next.continue_window.clone());
        pages.push((query, page));
        assert!(pages.len() < 2_000, "walk does not end");
        let Some(next) = next else {
            return pages;
        };
        query = next.query;
    }
}

const VIEWS: [&str; 2] = ["content", "compact"];
const WINDOW: usize = 4_000;

/// Each page of a whole walk is the page a cold read builds from nothing:
/// the walk keeps its first page's unscanned view in memory, and the page
/// scan, which runs on every page it cuts, is what protects the output.
#[test]
fn every_replayed_page_of_a_whole_walk_equals_its_cold_read() {
    let fixture = Fixture::new();
    let file = fixture.write("whole_walk.txt", source("cold_read"));
    let paths = Paths(fixture.0.clone());
    let security = octocode_runtime_base::security::ContentSecurity::new();
    for view in VIEWS {
        let query = qj(&file, serde_json::json!({ "view": view }));
        let pages = walk(&query, &paths, &security, WINDOW);
        assert!(pages.len() > 3, "{view}: {} pages", pages.len());
        let sha = pages[0].1.source_sha256.clone().expect("digest");
        for (index, (query, warm)) in pages.iter().enumerate() {
            octocode_tools_shared::source_view::retained::forget_view(&sha);
            let cold =
                execute_local_fetch(query, &paths, &security, &NeverCancel, Some(WINDOW), false);
            assert_eq!(*warm, cold, "{view}: page {index}");
        }
        let text: String = pages
            .iter()
            .map(|(_, page)| page.content.as_deref().expect("content"))
            .collect();
        for (what, secret) in [
            ("the token", TOKEN),
            ("the early key body", "earlyKeyBody"),
            ("the late key body", "lateKeyBody"),
        ] {
            assert!(!text.contains(secret), "{view}: {what} shown");
        }
        // Only the secret's bytes are replaced: the rest of its line stays.
        assert!(text.contains("token="), "{view}");
        assert!(
            text.matches('x').count() >= 40_000,
            "{view}: long line prefix"
        );
        assert!(
            text.matches('y').count() >= 40_000,
            "{view}: long line suffix"
        );
        for line in [
            "plain filler line 1\n",
            "plain filler line 4000",
            "closing filler line 2000",
        ] {
            assert!(text.contains(line), "{view}: {line:?} missing");
        }
    }
}

/// A whole walk prepares its source once: one private-key block pass for
/// the walk, not one per page.
#[test]
fn a_whole_walk_prepares_its_source_once() {
    let fixture = Fixture::new();
    let source = source("prepared_once");
    let paths = Paths(fixture.0.clone());
    for view in VIEWS {
        let file = fixture.write(&format!("prepared_once_{view}.txt"), &source);
        let query = qj(&file, serde_json::json!({ "view": view }));
        let scan = Counting::of(&source);
        let pages = walk(&query, &paths, &scan, WINDOW);
        assert!(pages.len() > 3, "{view}: {} pages", pages.len());
        assert_eq!(scan.passes().0, 1, "{view}: key-block passes");
    }
}

const LINE_BYTES: usize = 200_000;

/// One line far longer than a page (minified code), led by `test` so each
/// test's bytes are its own, with ` TOKEN ` at each source offset of `at`.
fn one_line(test: &str, at: &[usize]) -> String {
    let mut bytes = vec![b'a'; LINE_BYTES];
    bytes[..test.len()].copy_from_slice(test.as_bytes());
    let token = format!(" {TOKEN} ");
    for start in at {
        bytes[*start..*start + token.len()].copy_from_slice(token.as_bytes());
    }
    let mut line = String::from_utf8(bytes).expect("ASCII line");
    line.push('\n');
    line
}

/// Each page's byte length, in walk order.
fn lengths(pages: &[(LocalFetchQuery, LocalFetchResult)]) -> Vec<usize> {
    pages
        .iter()
        .map(|(_, page)| page.pagination.as_ref().expect("pagination").length)
        .collect()
}

/// A one-line walk cuts every byte page from one line: each page equals its
/// cold read, a token cut by a page's end shows whole as its placeholder on
/// the page it starts on, on the first page and on a late one.
#[test]
fn every_byte_page_of_a_one_line_walk_equals_its_cold_read() {
    let fixture = Fixture::new();
    let paths = Paths(fixture.0.clone());
    let security = octocode_runtime_base::security::ContentSecurity::new();
    let token = format!(" {TOKEN} ").len();
    for view in VIEWS {
        // The plain line's pages: where each page's end falls.
        let plain = fixture.write(&format!("plain_{view}.txt"), one_line("cut_probe", &[]));
        let probe = walk(
            &qj(&plain, serde_json::json!({ "view": view })),
            &paths,
            &security,
            WINDOW,
        );
        let first = lengths(&probe)[0];
        // One token across the first page's end, and a run of back-to-back
        // tokens over three pages from the seventh page on: a page end inside
        // the run cuts a token (the separating spaces are one byte of each).
        let late = (7 * first..10 * first).step_by(token).collect::<Vec<_>>();
        let mut at = vec![first - 10];
        at.extend(late.iter().copied());
        let file = fixture.write(&format!("one_line_{view}.txt"), one_line("cut_read", &at));
        let pages = walk(
            &qj(&file, serde_json::json!({ "view": view })),
            &paths,
            &security,
            WINDOW,
        );
        assert!(pages.len() > 9, "{view}: {} pages", pages.len());
        let sha = pages[0].1.source_sha256.clone().expect("digest");
        for (index, (query, warm)) in pages.iter().enumerate() {
            octocode_tools_shared::source_view::retained::forget_view(&sha);
            let cold =
                execute_local_fetch(query, &paths, &security, &NeverCancel, Some(WINDOW), false);
            assert_eq!(*warm, cold, "{view}: page {index}");
        }
        // Offsets count the redacted view: a page end that would split a
        // token's placeholder moves back before it, and the next page
        // starts with the whole placeholder.
        let cut = lengths(&pages);
        let starts_marked =
            |index: usize| {
                pages[index].1.content.as_deref().is_some_and(|text| {
                    text.starts_with(octocode_engine::security::REDACTION_PREFIX)
                })
            };
        assert!(
            cut[0] < first && starts_marked(1),
            "{view}: the first page ends before the token"
        );
        assert!(
            (6..cut.len() - 1).any(|index| cut[index] < first && starts_marked(index + 1)),
            "{view}: a late page ends before a token: {cut:?}"
        );
        let text: String = pages
            .iter()
            .map(|(_, page)| page.content.as_deref().expect("content"))
            .collect();
        assert!(!text.contains(TOKEN), "{view}: token shown");
        assert!(text.contains("cut_read"), "{view}");
    }
}

/// A one-line walk scans its line once: every byte page's scan window is
/// the whole line, and later pages reuse the first page's scan.
#[test]
fn a_one_line_walk_scans_its_line_once() {
    let fixture = Fixture::new();
    let paths = Paths(fixture.0.clone());
    for view in VIEWS {
        let source = one_line(&format!("scanned_once_{view}"), &[]);
        let file = fixture.write(&format!("scanned_once_{view}.txt"), &source);
        let scan = Counting::of(&source);
        let pages = walk(
            &qj(&file, serde_json::json!({ "view": view })),
            &paths,
            &scan,
            WINDOW,
        );
        assert!(pages.len() > 9, "{view}: {} pages", pages.len());
        assert_eq!(
            scan.passes(),
            (1, 1),
            "{view}: key-block passes, whole-line scans"
        );
    }
}
