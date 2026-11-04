//! The whole-source scan a range or match read keeps for the next such read:
//! never served across anything redaction reads, never changing a page.
use super::*;
use crate::tools::source_view::retained;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The real scanner, counting scans of a whole source.
pub(super) struct Counted {
    whole: usize,
    scans: AtomicUsize,
}

impl Counted {
    pub(super) fn of(source: &str) -> Self {
        Self {
            whole: source.len() / 10 * 9,
            scans: AtomicUsize::new(0),
        }
    }

    pub(super) fn scans(&self) -> usize {
        self.scans.load(Ordering::SeqCst)
    }
}

impl ContentScan for Counted {
    fn sanitize(
        &self,
        text: &str,
        path: &Path,
    ) -> Result<(String, Vec<String>), (ErrorCode, String)> {
        if text.len() >= self.whole {
            self.scans.fetch_add(1, Ordering::SeqCst);
        }
        ContentSecurity::new().sanitize(text, path)
    }
}

pub(super) fn read(
    q: &LocalFetchQuery,
    source: &str,
    path: &str,
    facts: &SourceFacts,
    scan: &impl ContentScan,
) -> serde_json::Value {
    serde_json::to_value(process_fetched_content(
        q,
        source.as_bytes(),
        Path::new(path),
        facts,
        scan,
        &NeverCancel,
    ))
    .expect("row")
}

pub(super) fn ranged() -> LocalFetchQuery {
    query(json!({"ranges":[{"startLine":1,"endLine":3}]}))
}

/// A source whose `password:` line redacts only in a configuration file.
fn config(seed: &str) -> String {
    format!("# {seed}\ndb:\n  password: hunter2hunter2hunter2\n")
}

/// Whichever path scanned the bytes first: a scan made at `a.yaml` must not hide
/// a line `a.rs` shows, and one made at `a.rs` must not answer for `a.yaml`.
#[test]
fn the_same_bytes_at_another_path_keep_their_own_redaction() {
    for (first, second) in [("a.rs", "a.yaml"), ("a.yaml", "a.rs")] {
        let source = config(first);
        let facts = SourceFacts::default();
        let at = |q: &LocalFetchQuery, path: &str| {
            read(q, &source, path, &facts, &ContentSecurity::new())["content"]
                .as_str()
                .expect("content")
                .to_owned()
        };
        for q in [
            ranged(),
            query(json!({"matchString":["db"],"contextLines":{"before":0,"after":2}})),
        ] {
            let opening = at(&q, first);
            assert!(kept_scan(&q, &source, second).is_none());
            let other = at(&q, second);
            let (code, yaml) = if first == "a.rs" {
                (&opening, &other)
            } else {
                (&other, &opening)
            };
            assert!(code.contains("hunter2"), "{code}");
            assert!(!yaml.contains("hunter2"), "{yaml}");
            // Each order, and a repeat at each path, whichever the memo holds.
            for _ in 0..2 {
                assert_eq!(&at(&q, "a.yaml"), yaml);
                assert!(
                    !kept_scan(&q, &source, "a.yaml")
                        .unwrap()
                        .contains("hunter2")
                );
                assert_eq!(&at(&q, "a.yaml"), yaml);
                assert_eq!(&at(&q, "a.rs"), code);
                assert!(kept_scan(&q, &source, "a.rs").unwrap().contains("hunter2"));
                assert_eq!(&at(&q, "a.rs"), code);
            }
        }
    }
}

/// A key path redacts what a code path shows, whichever scanned first.
#[test]
fn a_key_path_does_not_share_a_code_paths_scan() {
    let body = "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVo";
    for (first, second) in [("a.rs", "a.pem"), ("a.pem", "a.rs")] {
        let source = format!("{first}\n{body}0\n{body}1\n");
        let facts = SourceFacts::default();
        let at = |path: &str| read(&ranged(), &source, path, &facts, &ContentSecurity::new());
        at(first);
        at(second);
        let code = at("a.rs");
        let key = at("a.pem");
        assert!(code["content"].as_str().unwrap().contains(body));
        assert!(!key["content"].as_str().unwrap().contains(body), "{key}");
        assert_eq!(at("a.rs"), code);
        assert_eq!(at("a.pem"), key);
    }
}

/// Everything else a scan reads keeps its own entry: the scanner and whether
/// the bytes start inside the file.
#[test]
fn another_scanner_or_interior_bytes_do_not_share_a_scan() {
    struct Other;
    impl ContentScan for Other {
        fn sanitize(
            &self,
            text: &str,
            _: &Path,
        ) -> Result<(String, Vec<String>), (ErrorCode, String)> {
            Ok((text.replace("db", "[OTHER]"), Vec::new()))
        }
    }
    let source = format!("\u{feff}{}", config("another scanner"));
    let plain = SourceFacts::default();
    let interior = SourceFacts {
        interior: true,
        ..SourceFacts::default()
    };
    let ours = read(
        &ranged(),
        &source,
        "a.yaml",
        &plain,
        &ContentSecurity::new(),
    );
    let other = read(&ranged(), &source, "a.yaml", &plain, &Other);
    let inside = read(
        &ranged(),
        &source,
        "a.yaml",
        &interior,
        &ContentSecurity::new(),
    );
    assert!(other["content"].as_str().unwrap().contains("[OTHER]"));
    assert!(other["content"].as_str().unwrap().contains("hunter2"));
    assert!(!ours["content"].as_str().unwrap().contains("[OTHER]"));
    assert!(ours["content"].as_str().unwrap().starts_with("# another"));
    assert!(inside["content"].as_str().unwrap().starts_with('\u{feff}'));
    for _ in 0..2 {
        assert_eq!(read(&ranged(), &source, "a.yaml", &plain, &Other), other);
        assert_eq!(
            read(
                &ranged(),
                &source,
                "a.yaml",
                &interior,
                &ContentSecurity::new()
            ),
            inside
        );
        assert_eq!(
            read(
                &ranged(),
                &source,
                "a.yaml",
                &plain,
                &ContentSecurity::new()
            ),
            ours
        );
    }
}

#[test]
fn a_changed_file_misses() {
    let before = format!(
        "{}{}",
        config("changed before"),
        "const FILLER: u8 = 0;\n".repeat(5_000)
    );
    let after = before.replace("hunter2hunter2hunter2", "another2another2another2");
    let scan = Counted::of(&before);
    let facts = SourceFacts::default();
    let q = query(json!({"ranges":[{"startLine":1,"endLine":4}]}));
    let first = read(&q, &before, "a.rs", &facts, &scan);
    assert_eq!(read(&q, &before, "a.rs", &facts, &scan), first);
    assert_eq!(scan.scans(), 1);
    let changed = read(&q, &after, "a.rs", &facts, &scan);
    assert_eq!(scan.scans(), 2);
    assert!(changed["content"].as_str().unwrap().contains("another2"));
    assert!(!changed["content"].as_str().unwrap().contains("hunter2"));
}

/// A cursor naming other bytes is stale, memo or not.
#[test]
fn a_kept_scan_does_not_answer_a_stale_cursor() {
    let source = config("stale cursor");
    let facts = SourceFacts::default();
    let scan = ContentSecurity::new();
    read(&ranged(), &source, "a.rs", &facts, &scan);
    let mut q = ranged();
    local_fetch_query::set_snapshot(&mut q, Some(&"0".repeat(64))).expect("snapshot");
    let stale = read(&q, &source, "a.rs", &facts, &scan);
    assert_eq!(stale["errorCode"], "staleSnapshot", "{stale}");
}

/// Pages cut from a kept scan equal those cut from a fresh one, for every view
/// a selection can take, and a kept scan is not scanned again.
#[test]
fn pages_are_the_same_with_and_without_the_kept_scan() {
    let mut source = (1..=4_000)
        .map(|i| format!("fn item_{i}() -> u32 {{\n    {i}\n}}\n"))
        .collect::<String>();
    source.push_str("const KEY: &str = \"ghp_q7W8e9R0t1Y2q7W8e9R0t1Y2q7W8e9R0t1Y2\";\n");
    source.push_str("-----BEGIN RSA PRIVATE KEY-----\nQUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVo\n-----END RSA PRIVATE KEY-----\n");
    let sha256 = octocode_engine::digest::sha256(source.as_bytes());
    let queries = [
        json!({"ranges":[{"startLine":1,"endLine":9}]}),
        json!({"ranges":[{"startLine":1,"endLine":1},{"startLine":100,"endLine":120}]}),
        json!({"ranges":[{"startLine":2,"endLine":2}],"block":true}),
        json!({"ranges":[{"startLine":1,"endLine":90000}]}),
        json!({"ranges":[{"startLine":1,"endLine":12005}]}),
        json!({"matchString":["item_7"]}),
        json!({"matchString":["ITEM_7"],"caseMode":"insensitive"}),
        json!({"matchString":["item_[0-9]+\\(\\)"],"regexEngine":"rust"}),
        json!({"matchString":["item_7"],"ranges":[{"startLine":1,"endLine":90}]}),
        json!({"matchString":["item_7"],"block":true}),
        json!({"matchString":["no such text"]}),
        json!({"matchString":["ghp_"]}),
        json!({"matchString":["BEGIN RSA"]}),
    ];
    for fields in queries {
        let q = query(fields.clone());
        let facts = SourceFacts {
            window: Some(6_000),
            ..SourceFacts::default()
        };
        let walk = |scan: &Counted| {
            let mut q = q.clone();
            let mut pages = Vec::new();
            loop {
                let page = process_fetched_content(
                    &q,
                    source.as_bytes(),
                    Path::new("/fixture/lib.rs"),
                    &facts,
                    scan,
                    &NeverCancel,
                );
                let next = page
                    .next
                    .as_ref()
                    .and_then(|next| next.continue_window.clone());
                pages.push(serde_json::to_value(&page).expect("row"));
                match next {
                    Some(next) => q = next.query,
                    None => return pages,
                }
            }
        };
        retained::forget_scan(&sha256);
        let scan = Counted::of(&source);
        let fresh = walk(&scan);
        assert_eq!(scan.scans(), 1, "{fields}");
        let kept = walk(&scan);
        assert_eq!(scan.scans(), 1, "{fields}");
        assert_eq!(kept, fresh, "{fields}");
    }
}

/// The scan `ContentSecurity` kept for `source` read at `path`.
fn kept_scan(q: &LocalFetchQuery, source: &str, path: &str) -> Option<String> {
    retained::ScannedSource::of::<ContentSecurity>(
        q,
        &octocode_engine::digest::sha256(source.as_bytes()),
        (Path::new(path), path),
        &SourceFacts::default(),
    )?
    .get(q)
    .map(|source| source.text.clone().into_owned())
}

/// What the memo holds is what the scan left.
#[test]
fn a_kept_scan_holds_only_redacted_text() {
    let source = format!(
        "# kept text\nconst KEY = \"ghp_{}\";\n",
        "z9X8c7V6b5N4".repeat(3)
    );
    let q = ranged();
    assert!(kept_scan(&q, &source, "a.rs").is_none());
    read(
        &q,
        &source,
        "a.rs",
        &SourceFacts::default(),
        &ContentSecurity::new(),
    );
    let kept = kept_scan(&q, &source, "a.rs").expect("kept");
    assert!(kept.contains("REDACTED"), "{kept}");
    assert!(!kept.contains("ghp_"), "{kept}");
}

/// A whole read is not a selection: it scans its page, and keeps nothing here.
#[test]
fn a_whole_read_keeps_no_scan() {
    let source = config("whole read");
    let whole = query(json!({}));
    read(
        &whole,
        &source,
        "a.rs",
        &SourceFacts::default(),
        &ContentSecurity::new(),
    );
    assert!(kept_scan(&whole, &source, "a.rs").is_none());
    assert!(kept_scan(&ranged(), &source, "a.rs").is_none());
}
