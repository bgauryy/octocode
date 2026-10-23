//! A special file swapped in where a searched file was (a FIFO that would
//! block an open) is refused by every re-read of a search's sources: the
//! digest of a stored page and the clipped-value secret check.

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

#[cfg(unix)]
#[test]
fn a_fifo_in_place_of_a_source_is_refused_without_blocking() {
    let root = tempfile::tempdir().expect("fixture directory");
    let fifo = root.path().join("swapped.txt");
    crate::tools::test_support::make_fifo(&fifo);
    let digested = fifo.clone();
    let digest = within_deadline(move || manifest::digest_file(&digested).is_err());
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
            None,
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
