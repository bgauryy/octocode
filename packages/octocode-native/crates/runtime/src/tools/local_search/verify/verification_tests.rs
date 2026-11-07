use super::*;
use crate::execution::cancel::NeverCancel;
use crate::tools::local_search::sources::Expected;
use octocode_contract_types::LocalSearchQuery;

fn hit(line: u32, value: &str) -> octocode_engine::types::TextSearchMatch {
    octocode_engine::types::TextSearchMatch {
        deferred: None,
        line,
        first_line: line,
        last_line: None,
        column: 0,
        byte_offset: 0,
        value: value.into(),
        value_digest: octocode_engine::types::TextSearchMatch::digest_of(value),
        count: None,
        rank: None,
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
            term_counts: Vec::new(),
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
        judged: &super::JudgedWindows::default(),
    };
    let redact = || {
        let mut parsed = scan.clone();
        let (redacted, withheld, record) =
            redact_scan(&scope, &mut parsed, None).expect("redaction");
        assert!(withheld.is_empty(), "no file changed");
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
        term_counts: Vec::new(),
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
        None,
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
        let read = read_verification_lines(&mut open(&source), &[], usize::MAX, None, &NeverCancel)
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
    let verified = guard_source_secrets(
        &mut file,
        &source,
        Expected::Fresh,
        0..10,
        &ValueCheck {
            security: &ContentSecurity::new(),
            shape: RowShape::Span,
            cancel: &NeverCancel,
            judged: None,
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
        guard_source_secrets(
            &mut file,
            &source,
            Expected::Fresh,
            0..10,
            &ValueCheck {
                security: &ContentSecurity::new(),
                shape: RowShape::Span,
                cancel: &NeverCancel,
                judged: None,
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
    let digest: super::super::sources::Digest =
        octocode_engine::digest::sha256_digest(stored.as_bytes());
    let check = |expected| {
        let mut file = file_with(vec![hit(2, "the hit")]);
        let outcome = guard_source_secrets(
            &mut file,
            &source,
            expected,
            0..10,
            &ValueCheck {
                security: &ContentSecurity::new(),
                shape: RowShape::Lines(Default::default()),
                cancel: &NeverCancel,
                judged: None,
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

/// A row of a line too long to keep stands as the search judged it.
#[test]
fn a_row_of_a_line_past_the_retained_budget_stands() {
    let dir = tempfile::tempdir().expect("fixture");
    let source = dir.path().join("source.txt");
    let giant = "a".repeat(MAX_VERIFY_RETAINED_BYTES + 1);
    std::fs::write(&source, format!("{giant}\n")).expect("fixture");
    let mut file = file_with(vec![hit(1, "…aaaa…")]);
    let verified = guard_source_secrets(
        &mut file,
        &source,
        Expected::Fresh,
        0..10,
        &ValueCheck {
            security: &ContentSecurity::new(),
            shape: RowShape::Lines(Default::default()),
            cancel: &NeverCancel,
            judged: None,
        },
    )
    .expect("not cancelled");
    assert_eq!(verified, Verification::Verified);
    assert_eq!(file.matches[0].value, "…aaaa…");
}

/// Lines the search showed whole that outrun what one row's check keeps fail closed.
#[test]
fn a_neighborhood_over_the_retained_budget_fails_closed() {
    let dir = tempfile::tempdir().expect("fixture");
    let source = dir.path().join("source.txt");
    let line = "a".repeat(MAX_VERIFY_RETAINED_BYTES / 2 + 1);
    std::fs::write(&source, format!("{line}\n{line}\n")).expect("fixture");
    let mut file = file_with(vec![hit(1, "…aaaa…"), hit(2, "…aaaa…")]);
    let verified = guard_source_secrets(
        &mut file,
        &source,
        Expected::Fresh,
        0..10,
        &ValueCheck {
            security: &ContentSecurity::new(),
            shape: RowShape::Lines(Default::default()),
            cancel: &NeverCancel,
            judged: None,
        },
    )
    .expect("not cancelled");
    assert_eq!(verified, Verification::Unverified);
    assert_eq!(file.matches[1].value, OVERSIZED_PLACEHOLDER);
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
    let reason = guard_source_secrets(
        &mut file,
        &source,
        Expected::Fresh,
        0..10,
        &ValueCheck {
            security: &ContentSecurity::new(),
            shape: RowShape::Lines(Default::default()),
            cancel: &Cancelled,
            judged: None,
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

/// A line that is not UTF-8 is kept as the search decoded it, so a row's byte
/// offset, counted in the search's text, lands on the same bytes here.
#[test]
fn a_non_utf8_line_is_kept_as_the_search_decoded_it() {
    let dir = tempfile::tempdir().expect("fixture");
    let source = dir.path().join("latin1.txt");
    let line = b"caf\xe9 aa ghp_Z9Q8W7E6R5T4Y3U2I1O0P9L8K7J6H5G4F3D2";
    std::fs::write(&source, [&line[..], b"\n"].concat()).expect("fixture");
    let read = read_verification_lines(&mut open(&source), &[(1, 1)], 1, None, &NeverCancel)
        .unwrap_or_else(|_| panic!("readable"));
    assert_eq!(
        read.retained[&1],
        octocode_engine::text::Decoded::interior(line).text
    );
}

/// A walk of a source too large to keep masks a shown line where its secret is,
/// whatever bytes come before the secret on that line: a Latin-1 line counts
/// one character per raw byte, a UTF-8 one its own bytes. Nothing of the value
/// shows, and nothing beside it is lost.
#[test]
fn a_walk_masks_a_non_utf8_line_exactly_where_its_secret_is() {
    const VALUE: &str = "QmFzZTY0VmFsdWVIZXJlWFla";
    let reversed: String = VALUE.chars().rev().collect();
    for key in [vec![0xe9_u8; 12], "\u{e9}".repeat(12).into_bytes()] {
        let mut yaml =
            b"apiVersion: v1\nkind: Secret\nmetadata:\n  name: demo\ndata:\n  other: ".to_vec();
        yaml.extend_from_slice(reversed.as_bytes());
        yaml.extend_from_slice(b"\n  ");
        yaml.extend_from_slice(&key);
        yaml.extend_from_slice(format!(": {VALUE}\ntype: Opaque\n").as_bytes());
        let mut json = b"{\"apiVersion\": \"v1\", \"kind\": \"Secret\", \"data\": {\"".to_vec();
        json.extend_from_slice(&key);
        json.extend_from_slice(
            format!("\": \"{VALUE}\", \"tail\": \"{reversed}\"}},\n\"type\": \"Opaque\"}}\n")
                .as_bytes(),
        );
        for (name, bytes, line, lines) in [("s.yaml", yaml, 7, 8), ("s.json", json, 1, 2)] {
            let walked = super::exposed::walked_pass(
                || Ok(std::io::Cursor::new(bytes.clone())),
                Expected::Fresh,
                &[(line, line)],
                lines,
                std::path::Path::new(name),
                &NeverCancel,
            )
            .ok()
            .flatten()
            .expect("walked");
            let shown = &walked.read.retained[&line];
            for at in 0..VALUE.len() - 3 {
                assert!(!shown.contains(&VALUE[at..at + 4]), "{name}: {shown:?}");
            }
            assert!(walked.masked.contains_key(&line), "{name}: {shown:?}");
            if name == "s.json" {
                assert_eq!(
                    shown,
                    "{\"apiVersion\": \"v1\", \"kind\": \"Secret\", \"data\": {[REDACTED-KUBERNETESSECRETJSON]},"
                );
            } else {
                assert_eq!(shown, "  [REDACTED-KUBERNETESSECRETS]", "{name}");
            }
        }
    }
}

/// On a source too large to scan whole, a content row with context is judged
/// at the lines it starts at: a before-context line holding a Secret's value is
/// masked, and each line shows its own text. That holds too when the search
/// joined fewer lines than the context asks (`joined`): its before-context
/// stopped at a line it left out, such as test code under `testCode`.
#[test]
fn a_walked_context_row_is_judged_at_its_own_lines() {
    const VALUE: &str = "QmFzZTY0VmFsdWVIZXJlWFla";
    let pad = format!("# {}\n", "x".repeat(998)).repeat(MAX_VERIFY_RETAINED_BYTES / 1000 + 64);
    let text = format!(
        "apiVersion: v1\nkind: Secret\ndata:\n  blob: {VALUE}\n# trailer marker line\nlast plain line\n{pad}"
    );
    let source: Vec<&str> = text.lines().collect();
    for (before, joined, after) in [
        (1u32, 1u32, 0u32),
        (0, 0, 1),
        (1, 1, 1),
        (2, 2, 1),
        (2, 1, 1),
        (3, 1, 0),
        (3, 2, 1),
    ] {
        let first = 5 - joined as usize;
        let value = source[first - 1..5 + after as usize].join("\n");
        let mut file = file_with(vec![hit(5, &value)]);
        file.matches[0].first_line = u32::try_from(first).expect("line");
        let verified = guard_read(
            &mut file,
            std::path::Path::new("big.yaml"),
            || Ok(text.as_bytes()),
            Expected::Fresh,
            0..1,
            &ValueCheck {
                security: &ContentSecurity::new(),
                shape: RowShape::Lines(octocode_engine::types::SearchContext { before, after }),
                cancel: &NeverCancel,
                judged: None,
            },
        )
        .expect("not cancelled");
        assert_eq!(verified, Verification::Verified, "{before}/{after}");
        let shown = &file.matches[0].value;
        for at in 0..VALUE.len() - 3 {
            assert!(
                !shown.contains(&VALUE[at..at + 4]),
                "{before}/{after}: {shown:?}"
            );
        }
        let lines: Vec<&str> = shown.split('\n').collect();
        assert_eq!(lines.len(), value.split('\n').count(), "{shown:?}");
        for (at, line) in (first..).zip(&lines) {
            if at == 4 {
                assert_eq!(
                    line.trim(),
                    "[REDACTED-KUBERNETESSECRETS]",
                    "{before}/{after}"
                );
            } else {
                assert_eq!(*line, source[at - 1], "{before}/{after}: line {at}");
            }
        }
    }
}

/// A row whose lines disagree with the window the search reports for it is
/// withheld under its own reason, not as an unreadable source: more lines
/// before its match than the context allows, a value too short for the
/// lines it claims, and a first line past its match.
#[test]
fn a_row_its_window_cannot_place_is_withheld_as_unplaced() {
    let dir = tempfile::tempdir().expect("fixture");
    let path = dir.path().join("a.txt");
    let text = "l1\nl2\nl3\nl4 marker\nl5\nl6\n";
    std::fs::write(&path, text).expect("fixture");
    let context = octocode_engine::types::SearchContext {
        before: 1,
        after: 1,
    };
    for (first, value) in [
        (2, "l2\nl3\nl4 marker\nl5"),
        (3, "l4 marker"),
        (5, "l4 marker\nl5"),
    ] {
        let mut file = file_with(vec![hit(4, value)]);
        file.matches[0].first_line = first;
        let verified = guard_read(
            &mut file,
            &path,
            || std::fs::File::open(&path),
            Expected::Fresh,
            0..1,
            &ValueCheck {
                security: &ContentSecurity::new(),
                shape: RowShape::Lines(context),
                cancel: &NeverCancel,
                judged: None,
            },
        )
        .expect("not cancelled");
        assert_eq!(verified, Verification::Unverified, "{first}: {value:?}");
        assert_eq!(
            file.matches[0].value,
            super::guard::UNPLACED_PLACEHOLDER,
            "{first}: {value:?}"
        );
    }
    // The same row, placed where the search joined it, is shown.
    let mut file = file_with(vec![hit(4, "l3\nl4 marker\nl5")]);
    file.matches[0].first_line = 3;
    let verified = guard_read(
        &mut file,
        &path,
        || std::fs::File::open(&path),
        Expected::Fresh,
        0..1,
        &ValueCheck {
            security: &ContentSecurity::new(),
            shape: RowShape::Lines(context),
            cancel: &NeverCancel,
            judged: None,
        },
    )
    .expect("not cancelled");
    assert_eq!(verified, Verification::Verified);
    assert_eq!(file.matches[0].value, "l3\nl4 marker\nl5");
}

/// A walk masks a last line no newline ends exactly where its secret is, as it
/// masks any other line, whatever its encoding.
#[test]
fn a_walk_masks_an_unterminated_last_line_in_place() {
    const VALUE: &str = "QmFzZTY0VmFsdWVIZXJlWFla";
    for key in [vec![0xe9_u8; 12], "\u{e9}".repeat(12).into_bytes()] {
        let mut yaml = b"apiVersion: v1\nkind: Secret\ndata:\n  ".to_vec();
        yaml.extend_from_slice(&key);
        yaml.extend_from_slice(format!(": {VALUE}").as_bytes());
        let walked = super::exposed::walked_pass(
            || Ok(std::io::Cursor::new(yaml.clone())),
            Expected::Fresh,
            &[(4, 4)],
            4,
            std::path::Path::new("s.yaml"),
            &NeverCancel,
        )
        .ok()
        .flatten()
        .expect("walked");
        assert_eq!(walked.read.retained[&4], "  [REDACTED-KUBERNETESSECRETS]");
    }
}

/// In a source too large to scan whole, a row touching a private key's block
/// judges its other lines in their window, not alone: a signify key secret only
/// by the comment above it stays hidden beside the block it precedes.
#[test]
fn a_key_block_row_of_a_large_source_judges_its_other_lines_in_their_window() {
    const KEY: &str = "q8Zr4LmT2vXw9KpN7sYd3HfJ6gBc1QeRu5WaVo0MiExDzlUt";
    let pad = format!("# {}\n", "x".repeat(998)).repeat(MAX_VERIFY_RETAINED_BYTES / 1000 + 64);
    let text = format!(
        "untrusted comment: signify secret key\n{KEY}\n-----BEGIN PRIVATE KEY-----\nMIIBVQIBADANBgkqhkiG9w0BAQEFAASCAT8wggE7AgEAAkEA\n-----END PRIVATE KEY-----\n{pad}"
    );
    let mut file = file_with(vec![hit(3, &format!("{KEY}\n-----BEGIN PRIVATE KEY-----"))]);
    file.matches[0].first_line = 2;
    guard_read(
        &mut file,
        std::path::Path::new("big.txt"),
        || Ok(text.as_bytes()),
        Expected::Fresh,
        0..1,
        &ValueCheck {
            security: &ContentSecurity::new(),
            shape: RowShape::Lines(octocode_engine::types::SearchContext {
                before: 1,
                after: 0,
            }),
            cancel: &NeverCancel,
            judged: None,
        },
    )
    .expect("not cancelled");
    let shown = &file.matches[0].value;
    assert!(!shown.contains("LmT2"), "{shown:?}");
    assert!(!shown.contains("BEGIN PRIVATE"), "{shown:?}");
    assert_eq!(shown.split('\n').count(), 2, "{shown:?}");
}
