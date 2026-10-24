use super::*;
use crate::execution::cancel::NeverCancel;
use crate::tools::local_search::sources::Expected;
use octocode_contract_types::LocalSearchQuery;

fn hit(line: u32, value: &str) -> octocode_engine::types::TextSearchMatch {
    octocode_engine::types::TextSearchMatch {
        deferred: None,
        line,
        last_line: None,
        column: 0,
        byte_offset: 0,
        value: value.into(),
        value_digest: octocode_engine::types::TextSearchMatch::digest_of(value),
        count: None,
        rank: None,
        original_chars: None,

        original_bytes: None,
    }
}

/// Files redact on parallel workers, yet the record lists key-block
/// reads and replaced values in file and match order, every time, and
/// each replaced value is marked redacted on its row.
#[test]
fn a_parallel_redaction_records_in_file_order() {
    let dir = tempfile::tempdir().expect("fixture");
    let (paths, security) = crate::tools::test_support::simple_policy(dir.path());
    let key = "-----BEGIN RSA PRIVATE KEY-----\nMIIEpQIBAAKCAQEAinteriorKeyBodyAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n-----END RSA PRIVATE KEY-----\n";
    let mut files = Vec::new();
    for index in 0..48 {
        let path = dir.path().join(format!("f{index:02}.ts"));
        let mut body = String::from("const plain = 1;\n");
        let mut matches = vec![hit(1, "const plain = 1;")];
        if index % 3 == 0 {
            let secret = format!("const t = \"ghp_{index:02}ABCDEFGHIJKLMNOPQRSTUVWXYZ01234567\";");
            body.push_str(&secret);
            body.push('\n');
            matches.push(hit(2, &secret));
        }
        if index % 7 == 0 {
            body.push_str(key);
            matches.push(hit(
                4,
                "MIIEpQIBAAKCAQEAinteriorKeyBodyAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            ));
        }
        std::fs::write(&path, body).expect("fixture");
        files.push(octocode_engine::types::TextSearchFile {
            path: path.to_string_lossy().into_owned(),
            match_count: matches.len() as u32,
            matches,
            source: None,
        });
    }
    let scan = octocode_engine::types::TextSearchResult {
        files,
        stats: Default::default(),
    };
    let query: LocalSearchQuery = serde_json::from_value(serde_json::json!({
        "path": dir.path(), "matchString": ["const"]
    }))
    .expect("query");
    let expected = |_: &std::path::Path| Expected::Fresh;
    let scope = PageScope {
        query: &query,
        paths: &paths,
        output_root: dir.path(),
        expected_digest: &expected,
        security: &security,
        cancel: &NeverCancel,
        walk_threads: None,
    };
    let redact = || {
        let mut parsed = scan.clone();
        let (redacted, record) = redact_scan(&scope, &mut parsed, None).expect("redaction");
        (redacted, record, parsed)
    };
    let (redacted, record, parsed) = redact();
    for _ in 0..3 {
        let (again, again_record, again_parsed) = redact();
        assert_eq!(again, redacted);
        assert_eq!(again_record, record);
        assert_eq!(
            format!("{:?}", again_parsed.files),
            format!("{:?}", parsed.files)
        );
    }
    let order = record
        .changes
        .iter()
        .map(|(file, row, _)| (*file, *row))
        .collect::<Vec<_>>();
    assert!(order.is_sorted(), "{order:?}");
    assert!(record.key_files.iter().map(|(file, _)| *file).is_sorted());
    assert_eq!(record.key_files.len(), 7, "{:?}", record.key_files);
    assert!(record.changes.len() >= 16 + 7, "{:?}", record.changes);
    assert_eq!(redacted.len(), record.changes.len());
    assert!(parsed.files.iter().all(|file| !file.path.starts_with('/')));
    for (file, row, value) in &record.changes {
        assert_eq!(&parsed.files[*file].matches[*row].value, value);
    }
}

fn open(source: &std::path::Path) -> std::io::BufReader<std::fs::File> {
    std::io::BufReader::new(std::fs::File::open(source).expect("fixture"))
}

fn file_with(
    matches: Vec<octocode_engine::types::TextSearchMatch>,
) -> octocode_engine::types::TextSearchFile {
    octocode_engine::types::TextSearchFile {
        path: "source.txt".into(),
        match_count: matches.len() as u32,
        matches,
        source: None,
    }
}

#[test]
fn a_late_hit_retains_only_its_neighborhood_with_exact_coordinates() {
    let dir = tempfile::tempdir().expect("fixture");
    let source = dir.path().join("source.txt");
    let filler = "émoji ✓ filler line that is not near the hit\n".repeat(60_000);
    std::fs::write(&source, format!("{filler}before\nthe hit ✓\nafter\ntail\n")).expect("fixture");
    let hit_line = 60_002;
    let read = read_verification_lines(
        &mut open(&source),
        &[(hit_line - 1, hit_line + 1)],
        hit_line + 1,
        &NeverCancel,
    )
    .unwrap_or_else(|_| panic!("readable"));
    assert_eq!(read.lines_read, hit_line + 1);
    assert_eq!(read.retained.len(), 3);
    assert_eq!(read.retained[&hit_line], "the hit ✓");
    assert!(read.retained_bytes < 64, "{}", read.retained_bytes);
    assert!(read.key_ranges.is_empty());
}

#[test]
fn streamed_key_state_matches_the_whole_file_scan() {
    let body = "MIIEpQIBAAKCAQEAinteriorKeyBodyAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    for content in [
        format!("a\n-----BEGIN RSA PRIVATE KEY-----\n{body}\n-----END RSA PRIVATE KEY-----\nb\n"),
        format!("a\n-----BEGIN OPENSSH PRIVATE KEY-----\n{body}\n{body}\n"),
        format!(
            "{}\n-----BEGIN EC PRIVATE KEY-----\n{body}\r\n-----END EC PRIVATE KEY-----\r\n",
            "x".repeat(200_000)
        ),
    ] {
        let dir = tempfile::tempdir().expect("fixture");
        let source = dir.path().join("key.txt");
        std::fs::write(&source, &content).expect("fixture");
        let read = read_verification_lines(&mut open(&source), &[], usize::MAX, &NeverCancel)
            .unwrap_or_else(|_| panic!("readable"));
        assert!(read.retained.is_empty());
        assert!(!read.unclassified);
        assert_eq!(
            read.key_ranges,
            crate::security::private_key_block_line_ranges(&content)
        );
    }
}

#[test]
fn an_unbufferable_line_mentioning_a_private_key_fails_closed() {
    let dir = tempfile::tempdir().expect("fixture");
    let source = dir.path().join("source.txt");
    let giant = format!("{}PRIVATE KEY{}", " ".repeat(200_000), "-".repeat(10));
    std::fs::write(
        &source,
        format!("{giant}\nspacer\nspacer\nspacer\nMIIEpQIBAAKCAQEAinteriorKeyBody\n"),
    )
    .expect("fixture");
    let mut file = file_with(vec![hit(5, "…interiorKeyBo")]);
    let verified = guard_clipped_secrets(
        &mut file,
        &source,
        Expected::Fresh,
        0..10,
        &ValueCheck {
            security: &ContentSecurity::new(),
            match_only: true,
            cancel: &NeverCancel,
        },
    )
    .expect("not cancelled");
    assert_eq!(verified, Verification::Unverified);
    assert_eq!(file.matches[0].value, UNVERIFIED_PLACEHOLDER);
    // The same long line without a key mention is an ordinary line.
    std::fs::write(
        &source,
        format!(
            "{}\nspacer\nspacer\nspacer\nplain text here\n",
            " ".repeat(200_000)
        ),
    )
    .expect("fixture");
    let mut file = file_with(vec![hit(5, "plain text")]);
    assert_eq!(
        guard_clipped_secrets(
            &mut file,
            &source,
            Expected::Fresh,
            0..10,
            &ValueCheck {
                security: &ContentSecurity::new(),
                match_only: true,
                cancel: &NeverCancel,
            },
        )
        .expect("not cancelled"),
        Verification::Verified
    );
    assert_eq!(file.matches[0].value, "plain text");
}

/// Values from a stored scan are checked only against the bytes the scan
/// was stored with; other bytes, or no stored digest, mean `Changed`.
#[test]
fn a_stored_digest_binds_the_check_to_the_stored_bytes() {
    let dir = tempfile::tempdir().expect("fixture");
    let source = dir.path().join("source.txt");
    let stored = "one\nthe hit\nthree\n";
    std::fs::write(&source, stored).expect("fixture");
    let digest: super::super::sources::Digest = Sha256::digest(stored.as_bytes()).into();
    let check = |expected| {
        let mut file = file_with(vec![hit(2, "the hit")]);
        let outcome = guard_clipped_secrets(
            &mut file,
            &source,
            expected,
            0..10,
            &ValueCheck {
                security: &ContentSecurity::new(),
                match_only: false,
                cancel: &NeverCancel,
            },
        )
        .expect("not cancelled");
        (outcome, file.matches[0].value.clone())
    };
    assert_eq!(
        check(Expected::Stored(Some(digest))),
        (Verification::Verified, "the hit".to_owned())
    );
    assert_eq!(check(Expected::Stored(None)).0, Verification::Changed);
    std::fs::write(&source, "one\nthe hit\nthre3\n").expect("fixture");
    assert_eq!(
        check(Expected::Stored(Some(digest))).0,
        Verification::Changed
    );
    assert_eq!(check(Expected::Fresh).0, Verification::Verified);
}

#[test]
fn a_neighborhood_over_the_retained_budget_fails_closed() {
    let dir = tempfile::tempdir().expect("fixture");
    let source = dir.path().join("source.txt");
    let giant = "a".repeat(MAX_VERIFY_RETAINED_BYTES + 1);
    std::fs::write(&source, format!("{giant}\n")).expect("fixture");
    let mut file = file_with(vec![hit(1, "…aaaa…")]);
    let verified = guard_clipped_secrets(
        &mut file,
        &source,
        Expected::Fresh,
        0..10,
        &ValueCheck {
            security: &ContentSecurity::new(),
            match_only: false,
            cancel: &NeverCancel,
        },
    )
    .expect("not cancelled");
    assert_eq!(verified, Verification::Unverified);
    assert_eq!(file.matches[0].value, OVERSIZED_PLACEHOLDER);
}

#[test]
fn verification_stops_when_cancelled() {
    struct Cancelled;
    impl CancellationCheck for Cancelled {
        fn check(&self) -> Result<(), String> {
            Err("Cancelled".into())
        }
    }
    let dir = tempfile::tempdir().expect("fixture");
    let source = dir.path().join("source.txt");
    std::fs::write(&source, "line\n".repeat(VERIFY_CANCEL_EVERY * 2)).expect("fixture");
    let mut file = file_with(vec![hit((VERIFY_CANCEL_EVERY * 2) as u32, "line")]);
    let reason = guard_clipped_secrets(
        &mut file,
        &source,
        Expected::Fresh,
        0..10,
        &ValueCheck {
            security: &ContentSecurity::new(),
            match_only: false,
            cancel: &Cancelled,
        },
    )
    .expect_err("cancelled during the stream");
    assert_eq!(reason, "Cancelled");
}

/// Context lines beside a key block stay readable; only the lines
/// inside the block are redacted.
#[test]
fn key_block_redaction_is_line_precise() {
    let source = [
        "const filler393 = \"match_me 393\";",
        "const filler394 = \"match_me 394\";",
        "-----BEGIN RSA PRIVATE KEY-----",
        "MIIEowIBAAKCAQEAexamplebodyexamplebodyexample",
        "-----END RSA PRIVATE KEY-----",
    ];
    let at = |n: u32| source.get((n as usize).checked_sub(1)?).copied();
    let ranges = [(3, 5)];
    let placeholder = crate::security::key_fragment_placeholder();
    // Match on line 2 with one context line each side (lines 1-3).
    let value = format!("{}\n{}\n{}", source[0], source[1], source[2]);
    assert_eq!(
        redact_key_block_lines(2, &value, &ranges, at),
        format!("{}\n{}\n{placeholder}", source[0], source[1])
    );
    // An already-redacted value still aligns.
    let redacted = format!("{}\n{placeholder}\n{placeholder}", source[1]);
    assert_eq!(redact_key_block_lines(3, &redacted, &ranges, at), redacted);
    // A value no alignment fits fails closed.
    assert_eq!(
        redact_key_block_lines(2, "not in the source\nMIIEowIB", &ranges, at),
        placeholder
    );
}
