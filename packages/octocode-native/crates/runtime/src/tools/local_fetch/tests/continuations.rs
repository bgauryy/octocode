//! A continuation page replays the view its cursor's first page built: a
//! walk decodes, redacts and outlines its source once, never per page, and
//! a changed source ends the walk as a stale snapshot.
use super::*;

/// The test scan, counting the whole-source passes a read makes: the
/// private-key block pass and every scan of at least `whole` bytes (the
/// source-wide line redaction; a page scan is far smaller).
struct Counting {
    whole: usize,
    scans: AtomicUsize,
    key_passes: AtomicUsize,
}

impl Counting {
    fn of(source: &str) -> Self {
        // A page scan covers the page and its margins (8 KiB each side):
        // far less than half of these sources.
        assert!(source.len() >= 96 * 1024, "{} bytes", source.len());
        Self {
            // The key-block pass may shorten the source it hands on.
            whole: source.len() / 2,
            scans: AtomicUsize::new(0),
            key_passes: AtomicUsize::new(0),
        }
    }

    fn passes(&self) -> (usize, usize) {
        (
            self.key_passes.load(Ordering::SeqCst),
            self.scans.load(Ordering::SeqCst),
        )
    }
}

impl ContentScan for Counting {
    fn sanitize(
        &self,
        text: &str,
        path: &Path,
    ) -> Result<(String, Vec<String>), (ErrorCode, String)> {
        if text.len() >= self.whole {
            self.scans.fetch_add(1, Ordering::SeqCst);
        }
        Safe.sanitize(text, path)
    }

    fn redact_key_blocks<'a>(&self, content: &'a str) -> (std::borrow::Cow<'a, str>, bool) {
        self.key_passes.fetch_add(1, Ordering::SeqCst);
        crate::security::redact_private_key_blocks(content)
    }
}

/// Every page of `query` at `window`, following each next page.
fn walk(
    query: &LocalFetchQuery,
    paths: &Paths,
    scan: &impl ContentScan,
    window: usize,
) -> Vec<LocalFetchResult> {
    let mut pages = Vec::new();
    let mut query = query.clone();
    loop {
        let page = execute_local_fetch(&query, paths, scan, &NeverCancel, Some(window), None);
        assert_eq!(page.error_code, None, "{page:?}");
        let next = page
            .next
            .as_ref()
            .and_then(|next| next.continue_window.clone());
        pages.push(page);
        assert!(pages.len() < 500, "walk does not end");
        let Some(next) = next else {
            return pages;
        };
        query = next.query;
    }
}

fn joined(pages: &[LocalFetchResult]) -> String {
    pages
        .iter()
        .map(|page| page.content.as_deref().expect("content"))
        .collect()
}

/// The whole view in one page: the walk must reconstruct it exactly.
fn one_page(query: &LocalFetchQuery, paths: &Paths) -> LocalFetchResult {
    let page = execute_local_fetch(query, paths, &Safe, &NeverCancel, Some(50_000_000), None);
    assert_eq!(page.error_code, None, "{page:?}");
    assert!(
        page.next
            .as_ref()
            .and_then(|next| next.continue_window.as_ref())
            .is_none()
    );
    page
}

/// `file` as its cursor names it: relative to the workspace root.
fn named(file: &Path) -> &Path {
    Path::new(file.file_name().expect("file name"))
}

fn rust_source(tag: &str, functions: usize) -> String {
    (0..functions)
        .map(|n| {
            format!(
                "/// {tag} {n}\npub fn {tag}_{n}(value: usize) -> usize {{\n    value + {n}\n}}\n\n"
            )
        })
        .collect()
}

#[test]
fn a_symbols_walk_outlines_its_source_once() {
    let fixture = Fixture::new();
    let source = rust_source("outlined_once", 1_500);
    let file = fixture.write("outlined_once.rs", &source);
    let paths = Paths(fixture.0.clone());
    let query = qj(named(&file), serde_json::json!({"view":"symbols"}));
    let scan = Counting::of(&source);
    let pages = walk(&query, &paths, &scan, 2_000);
    assert!(pages.len() > 2, "{} pages", pages.len());
    assert_eq!(
        scan.passes(),
        (1, 1),
        "key-block passes, whole-source scans"
    );
    assert_eq!(
        joined(&pages),
        one_page(&query, &paths).content.expect("outline")
    );
}

/// A walk started with an absolute path names its file as its cursor does
/// from the first page, so the continuation is the same read.
#[test]
fn a_symbols_walk_started_by_an_absolute_path_outlines_its_source_once() {
    let fixture = Fixture::new();
    let source = rust_source("absolute_once", 1_500);
    let file = fixture.write("absolute_once.rs", &source);
    assert!(file.is_absolute(), "{file:?}");
    let paths = Paths(fixture.0.clone());
    let query = qj(&file, serde_json::json!({"view":"symbols"}));
    let scan = Counting::of(&source);
    let pages = walk(&query, &paths, &scan, 2_000);
    assert!(pages.len() > 2, "{} pages", pages.len());
    assert_eq!(
        scan.passes(),
        (1, 1),
        "key-block passes, whole-source scans"
    );
    assert_eq!(
        joined(&pages),
        one_page(&query, &paths).content.expect("outline")
    );
}

#[test]
fn a_match_walk_scans_its_source_once() {
    let fixture = Fixture::new();
    let source: String = (1..=10_000)
        .map(|n| {
            format!(
                "line {n} {}\n",
                if n % 7 == 0 {
                    "scanned_once_hit"
                } else {
                    "plain"
                }
            )
        })
        .collect();
    let file = fixture.write("scanned_once.txt", &source);
    let paths = Paths(fixture.0.clone());
    let query = qj(
        named(&file),
        serde_json::json!({"matchString":["scanned_once_hit"]}),
    );
    let scan = Counting::of(&source);
    let pages = walk(&query, &paths, &scan, 1_500);
    assert!(pages.len() > 2, "{} pages", pages.len());
    assert_eq!(
        scan.passes(),
        (1, 1),
        "key-block passes, whole-source scans"
    );
    assert_eq!(
        joined(&pages),
        one_page(&query, &paths).content.expect("matches")
    );
    let matched: Vec<usize> = pages
        .iter()
        .flat_map(|page| page.matched_lines.clone())
        .collect();
    assert_eq!(matched, (7..=10_000).step_by(7).collect::<Vec<_>>());
}

/// The key-block pass runs over the whole source before any window is cut,
/// so a block that spans page windows is redacted on every page a
/// continuation replays.
#[test]
fn a_key_block_across_page_windows_stays_redacted_on_every_page() {
    let fixture = Fixture::new();
    let body =
        |n: usize| format!("MIIEpQIBAAKCAQEAspanningKeyBody{n:04}AAAAAAAAAAAAAAAAAAAAAAAAAAAA");
    let mut source: String = (1..=3_000)
        .map(|n| format!("before spanning {n}\n"))
        .collect();
    source.push_str("-----BEGIN RSA PRIVATE KEY-----\n");
    for n in 0..120 {
        source.push_str(&body(n));
        source.push('\n');
    }
    source.push_str("-----END RSA PRIVATE KEY-----\n");
    source.extend((1..=3_000).map(|n| format!("after spanning {n}\n")));
    let file = fixture.write("spanning.txt", &source);
    let file = named(&file);
    let paths = Paths(fixture.0.clone());
    let total = source.lines().count();
    let query = qj(
        file,
        serde_json::json!({"ranges":[{"startLine":1,"endLine":total}]}),
    );
    let scan = Counting::of(&source);
    let pages = walk(&query, &paths, &scan, 1_500);
    let key_pages = pages
        .iter()
        .filter(|page| {
            page.content
                .as_deref()
                .is_some_and(|text| text.contains(&crate::security::key_fragment_placeholder()))
        })
        .count();
    assert!(key_pages > 1, "the block spans {key_pages} pages");
    for page in &pages {
        let text = page.content.as_deref().expect("content");
        assert!(!text.contains("spanningKeyBody"), "key body leaked: {text}");
    }
    assert_eq!(
        scan.passes(),
        (1, 1),
        "key-block passes, whole-source scans"
    );
    assert_eq!(
        joined(&pages),
        one_page(&query, &paths).content.expect("range")
    );
}

/// Only a page that issues a next page keeps its view, and only a view
/// whose texts passed the source-wide scan: a one-page read keeps nothing,
/// nor does a whole-file content walk (its view is the unscanned source).
#[test]
fn only_a_continued_scanned_view_is_kept() {
    let fixture = Fixture::new();
    let paths = Paths(fixture.0.clone());
    let kept = |name: &str, source: &str, fields: serde_json::Value| {
        let file = fixture.write(name, source);
        let page = execute_local_fetch(
            &qj(&file, fields),
            &paths,
            &Safe,
            &NeverCancel,
            Some(2_000),
            None,
        );
        assert_eq!(page.error_code, None, "{page:?}");
        let continues = page
            .next
            .as_ref()
            .is_some_and(|next| next.continue_window.is_some());
        let sha = page.source_sha256.expect("digest");
        (continues, crate::tools::source_view::retained::holds(&sha))
    };
    let symbols = serde_json::json!({"view":"symbols"});
    assert_eq!(
        kept("kept_one.rs", &rust_source("kept_one", 2), symbols.clone()),
        (false, false)
    );
    assert_eq!(
        kept("kept_many.rs", &rust_source("kept_many", 200), symbols),
        (true, true)
    );
    let whole: String = (1..=2_000).map(|n| format!("kept whole {n}\n")).collect();
    assert_eq!(
        kept("kept_whole.txt", &whole, serde_json::json!({})),
        (true, false)
    );
}

/// A source written after its walk began ends the walk as a stale snapshot
/// whatever its first page retained, for an outline and a match read.
#[test]
fn a_changed_source_ends_a_retained_walk_as_stale() {
    let fixture = Fixture::new();
    let source = rust_source("stale_walk", 200);
    let file = fixture.write("stale_walk.rs", &source);
    let paths = Paths(fixture.0.clone());
    for fields in [
        serde_json::json!({"view":"symbols"}),
        serde_json::json!({"matchString":["pub fn"]}),
    ] {
        fs::write(&file, &source).expect("source");
        let first = execute_local_fetch(
            &qj(&file, fields),
            &paths,
            &Safe,
            &NeverCancel,
            Some(2_000),
            None,
        );
        let next = first
            .next
            .and_then(|next| next.continue_window)
            .expect("next page");
        fs::write(&file, source.replace("value + 1", "value + 2")).expect("edit");
        let page = execute_local_fetch(&next.query, &paths, &Safe, &NeverCancel, Some(2_000), None);
        assert_eq!(page.error_code, Some(ErrorCode::StaleSnapshot), "{page:?}");
        assert_eq!(page.content, None);
    }
}
