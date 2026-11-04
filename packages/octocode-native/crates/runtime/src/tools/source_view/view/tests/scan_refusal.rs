//! A read its own input refuses says so before the whole-source scan, with the
//! row it would have got after it.
use super::scan_memo::{Counted, ranged, read};
use super::*;
use crate::tools::source_view::retained;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A read refused for its input refuses before the scan, with the row the
/// extraction of the scanned text gives.
#[test]
fn a_refused_read_does_not_scan_and_refuses_the_same() {
    let source = (1..=50).map(|i| format!("line {i}\n")).collect::<String>();
    let facts = SourceFacts::default();
    for fields in [
        json!({"matchString":["(unclosed"],"regexEngine":"rust"}),
        json!({"matchString":["(unclosed"],"regexEngine":"pcre2"}),
        json!({"matchString":["line","[bad"],"regexEngine":"rust"}),
        json!({"matchString":["(unclosed"],"regexEngine":"rust","ranges":[{"startLine":90,"endLine":99}]}),
        json!({"ranges":[{"startLine":51,"endLine":60}]}),
        json!({"ranges":[{"startLine":51,"endLine":60},{"startLine":70,"endLine":71}]}),
        json!({"matchString":["line"],"ranges":[{"startLine":900,"endLine":901}]}),
        json!({"matchString":["line"],"regexEngine":"rust","ranges":[{"startLine":900,"endLine":901}]}),
    ] {
        let q = query(fields.clone());
        let lines = crate::tools::source_view::extraction::line_count(&source);
        let error = extract(&q, &source).err().expect("a refused selection");
        let expected = serde_json::to_value(extraction_error(&q, error, lines)).expect("row");
        let scan = Counted::of(&source);
        let refused = read(&q, &source, "/fixture/refused.rs", &facts, &scan);
        assert_eq!(scan.scans(), 0, "{fields}");
        assert_eq!(refused, expected, "{fields}");
        // The same with the scan kept by an earlier read of the bytes.
        read(&ranged(), &source, "/fixture/refused.rs", &facts, &scan);
        let refused = read(&q, &source, "/fixture/refused.rs", &facts, &scan);
        assert_eq!(refused, expected, "{fields}");
        retained::forget_scan(&octocode_engine::digest::sha256(source.as_bytes()));
    }
}

/// A partly valid range list is no refusal: it reads what exists.
#[test]
fn a_range_inside_the_file_is_not_refused() {
    let source = "one\ntwo\n# partly valid\n";
    let q = query(json!({"ranges":[{"startLine":2,"endLine":2},{"startLine":90,"endLine":91}]}));
    let row = read(
        &q,
        source,
        "a.rs",
        &SourceFacts::default(),
        &ContentSecurity::new(),
    );
    assert_eq!(row["content"], "two\n", "{row}");
    assert!(row["errorCode"].is_null(), "{row}");
}

/// A scanner that refuses the source still comes first.
#[test]
fn a_scanner_refusal_comes_before_an_input_refusal() {
    struct Limited;
    impl ContentScan for Limited {
        fn sanitize(
            &self,
            _: &str,
            _: &Path,
        ) -> Result<(String, Vec<String>), (ErrorCode, String)> {
            Err((ErrorCode::ContentSecurityLimit, "limited".into()))
        }
    }
    let source = "one\ntwo\n# scanner first\n";
    for fields in [
        json!({"matchString":["(unclosed"],"regexEngine":"rust"}),
        json!({"ranges":[{"startLine":90,"endLine":91}]}),
        json!({"ranges":[{"startLine":1,"endLine":1}]}),
    ] {
        let row = read(
            &query(fields.clone()),
            source,
            "a.rs",
            &SourceFacts::default(),
            &Limited,
        );
        assert_eq!(row["errorCode"], "contentSecurityLimit", "{fields}: {row}");
        assert_eq!(row["error"], "limited", "{fields}: {row}");
    }
}

/// A scan the scanner refused is not kept: the next read asks the scanner.
#[test]
fn a_refused_scan_keeps_nothing() {
    struct Limited(AtomicUsize);
    impl ContentScan for Limited {
        fn sanitize(
            &self,
            _: &str,
            _: &Path,
        ) -> Result<(String, Vec<String>), (ErrorCode, String)> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Err((ErrorCode::ContentSecurityLimit, "limited".into()))
        }
    }
    let source = "# refused scan\n";
    let scan = Limited(AtomicUsize::new(0));
    read(&ranged(), source, "a.rs", &SourceFacts::default(), &scan);
    let once = scan.0.load(Ordering::SeqCst);
    assert!(once > 0);
    read(&ranged(), source, "a.rs", &SourceFacts::default(), &scan);
    assert_eq!(scan.0.load(Ordering::SeqCst), once * 2);
}
