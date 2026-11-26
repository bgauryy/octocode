//! The source secret guard on a page's values: a source it cannot read again,
//! or one that no longer holds the matched line, leaves the value unverified
//! and redacted.

use super::*;

#[test]
fn source_secret_guard_fails_closed_when_the_source_cannot_be_reread() {
    let security = ContentSecurity::new();
    let mut file = octocode_engine::types::TextSearchFile {
        sort_time: None,
        term_counts: Vec::new(),
        path: "gone.txt".into(),
        match_count: 1,
        counted: Default::default(),
        matches: vec![octocode_engine::types::TextSearchMatch {
            deferred: None,
            line: 1,
            first_line: 1,
            last_line: None,
            column: 0,
            byte_offset: 0,
            match_len: 0,
            value: "…token = ghp_abcdefghijklmnopqrstuvwxyz0123456789".into(),
            value_digest: <sha2::Sha256 as sha2::Digest>::digest(
                "…token = ghp_abcdefghijklmnopqrstuvwxyz0123456789".as_bytes(),
            )
            .into(),
            count: None,
            rank: None,
            window_start: None,
        }],
        source: None,
    };
    let missing = tempfile::tempdir()
        .expect("fixture directory")
        .path()
        .join("gone.txt");
    let verified = verify::guard_source_secrets(
        &mut file,
        &missing,
        crate::local_search::sources::Expected::Fresh,
        0..10,
        &verify::ValueCheck {
            security: &security,
            shape: verify::RowShape::Lines(Default::default()),
            cancel: &NeverCancel,
            judged: None,
            hidden: &verify::HiddenRows::default(),
        },
    )
    .expect("not cancelled");
    assert_eq!(verified, verify::Verification::Unverified);
    assert!(!file.matches[0].value.contains("ghp_"));
    assert!(file.matches[0].value.contains("REDACTED"));
}

/// A source that shrank or was replaced after the search no longer holds
/// the matched line, so the clipped value cannot be verified.
#[test]
fn source_secret_guard_fails_closed_when_the_match_line_is_gone() {
    let security = ContentSecurity::new();
    let root = tempfile::tempdir().expect("fixture directory");
    let source = root.path().join("shrunk.txt");
    fs::write(&source, "one line now\n").expect("fixture");
    let mut file = octocode_engine::types::TextSearchFile {
        sort_time: None,
        term_counts: Vec::new(),
        path: "shrunk.txt".into(),
        match_count: 1,
        counted: Default::default(),
        matches: vec![octocode_engine::types::TextSearchMatch {
            deferred: None,
            line: 40,
            first_line: 40,
            last_line: None,
            column: 0,
            byte_offset: 0,
            match_len: 0,
            value: "…token = ghp_abcdefghijklmnopqrstuvwxyz0123456789".into(),
            value_digest: <sha2::Sha256 as sha2::Digest>::digest(
                "…token = ghp_abcdefghijklmnopqrstuvwxyz0123456789".as_bytes(),
            )
            .into(),
            count: None,
            rank: None,
            window_start: None,
        }],
        source: None,
    };
    let verified = verify::guard_source_secrets(
        &mut file,
        &source,
        crate::local_search::sources::Expected::Fresh,
        0..10,
        &verify::ValueCheck {
            security: &security,
            shape: verify::RowShape::Lines(Default::default()),
            cancel: &NeverCancel,
            judged: None,
            hidden: &verify::HiddenRows::default(),
        },
    )
    .expect("not cancelled");
    assert_eq!(verified, verify::Verification::Unverified);
    assert!(
        !file.matches[0].value.contains("ghp_"),
        "{}",
        file.matches[0].value
    );
}
