//! A special file swapped in where a searched file was (a FIFO that would
//! block an open, or a symlink) is refused by every re-read of a search's
//! sources: the digest of a stored page, the stored page's source hash, the
//! key-block scan, the clipped-value secret check, and the reads that
//! outline a shown or priced file.

use super::*;

/// `run` on its own thread; a call that blocks (a FIFO open with no
/// writer) fails the test instead of hanging it.
fn within_deadline<T: Send + 'static>(run: impl FnOnce() -> T + Send + 'static) -> T {
    let (sent, received) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sent.send(run());
    });
    received
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("the re-read returned instead of blocking on the FIFO")
}

#[test]
fn a_fifo_in_place_of_a_source_is_refused_without_blocking() {
    let root = tempfile::tempdir().expect("fixture directory");
    let fifo = root.path().join("swapped.txt");
    crate::tools::test_support::make_fifo(&fifo);
    let digested = fifo.clone();
    let digest = within_deadline(move || sources::digest_file(&digested).is_err());
    assert!(digest, "a FIFO has no digest");
    let source = fifo.clone();
    let verified = within_deadline(move || {
        let value = "…token = ghp_abcdefghijklmnop";
        let mut file = octocode_engine::types::TextSearchFile {
            path: "swapped.txt".into(),
            match_count: 1,
            matches: vec![octocode_engine::types::TextSearchMatch {
                deferred: None,
                line: 1,
                last_line: None,
                column: 0,
                byte_offset: 0,
                value: value.into(),
                value_digest: <sha2::Sha256 as sha2::Digest>::digest(value.as_bytes()).into(),
                count: None,
                rank: None,
                original_chars: Some(400),
                original_bytes: Some(400),
            }],
            source: None,
        };
        let verified = verify::guard_clipped_secrets(
            &mut file,
            &source,
            crate::tools::local_search::sources::Expected::Fresh,
            0..10,
            &verify::ValueCheck {
                security: &ContentSecurity::new(),
                match_only: false,
                cancel: &NeverCancel,
            },
        )
        .expect("not cancelled");
        (verified, file.matches[0].value.clone())
    });
    assert_eq!(verified.0, verify::Verification::Unverified);
    assert!(!verified.1.contains("ghp_"), "{}", verified.1);
}

#[test]
fn a_fifo_or_link_in_place_of_a_source_is_never_reread() {
    let root = tempfile::tempdir().expect("fixture directory");
    let real = root.path().join("real.txt");
    std::fs::write(&real, "needle\n").expect("fixture");
    let link = root.path().join("link.txt");
    std::os::unix::fs::symlink(&real, &link).expect("link");
    let fifo = root.path().join("swapped.txt");
    crate::tools::test_support::make_fifo(&fifo);
    for swapped in [link, fifo] {
        let read = swapped.clone();
        let (stored, key_scan, shown, entry, budget) = within_deadline(move || {
            let mut budget = 1024;
            let stored =
                manifest::read_source(read.to_string_lossy().into_owned(), &[], &mut budget);
            (
                stored.is_none(),
                verify::read_key_scan_bytes(&read).is_none(),
                enclosing::shown_bytes(&read, crate::tools::local_search::sources::Expected::Fresh)
                    .is_none(),
                sources::read_entry(&read, 1024).is_none(),
                budget,
            )
        });
        assert!(stored, "{} has no stored source", swapped.display());
        assert!(key_scan, "{} has no key-block scan", swapped.display());
        assert!(shown, "{} has no shown outline", swapped.display());
        assert!(entry, "{} is not re-read", swapped.display());
        assert_eq!(budget, 1024, "a refused read spends no budget");
    }
    let mut budget = 1024;
    assert!(manifest::read_source(real.to_string_lossy().into_owned(), &[], &mut budget).is_some());
    assert!(verify::read_key_scan_bytes(&real).is_some());
    assert_eq!(
        enclosing::shown_bytes(&real, crate::tools::local_search::sources::Expected::Fresh)
            .as_deref(),
        Some(b"needle\n".as_slice())
    );
    assert!(sources::read_entry(&real, 1024).is_some());
}

/// A match in a file that is not UTF-8 shows the text localFetch reads
/// (Latin-1 here, not U+FFFD), and the row names the file as decoded.
#[test]
fn a_non_utf8_match_reads_as_localfetch_decodes_it_and_says_so() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::write(root.path().join("latin1.txt"), b"caf\xe9 needle\n").expect("latin-1");
    fs::write(root.path().join("plain.txt"), "needle\n").expect("utf-8");
    let result = search_root(root.path(), serde_json::json!({"matchString":["needle"]}));
    let body = serde_json::to_value(&result).expect("serialize");
    let latin = body["files"]
        .as_array()
        .expect("files")
        .iter()
        .find(|file| file["path"] == "latin1.txt")
        .unwrap_or_else(|| panic!("latin1.txt listed: {body}"));
    assert_eq!(latin["matches"][0]["value"], "café needle", "{body}");
    let warnings = body["warnings"].to_string();
    assert!(
        warnings.contains("latin1.txt") && warnings.contains("not valid UTF-8"),
        "{body}"
    );
    assert!(!warnings.contains("plain.txt"), "{body}");
}
