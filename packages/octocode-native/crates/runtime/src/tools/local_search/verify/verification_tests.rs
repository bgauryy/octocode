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
        window_start: None,
    }
}

/// Files redact on parallel workers, yet the record lists key-block
/// reads and replaced values in file and match order, every time, and
/// each replaced value is marked redacted on its row. Only a key block's
/// rows are replaced here, by what the whole file says of them: every other
/// value is judged with its whole source when a page shows it, never alone.
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
    assert!(record.key_files.iter().map(|key| key.index).is_sorted());
    assert_eq!(record.key_files.len(), 7, "{:?}", record.key_files);
    assert_eq!(record.changes.len(), 7, "{:?}", record.changes);
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
        false,
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
        let read =
            read_verification_lines(&mut open(&source), &[], usize::MAX, false, &NeverCancel)
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
    // Longer than a whole read keeps: a line that fits is read and judged.
    let giant = format!(
        "{}PRIVATE KEY{}",
        " ".repeat(octocode_engine::security::LONG_LINE_BYTES + 3),
        "-".repeat(10)
    );
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
            hidden: &HiddenRows::default(),
        },
    )
    .expect("not cancelled");
    assert_eq!(verified, Verification::Unverified);
    // The row is shown as the scan read it: a cut run of text is no known
    // key shape, and nothing hides the whole row.
    assert_eq!(file.matches[0].value, "…interiorKeyBo");
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
                hidden: &HiddenRows::default(),
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
                hidden: &HiddenRows::default(),
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
fn a_row_of_a_line_past_what_a_search_holds_whole_stands() {
    let dir = tempfile::tempdir().expect("fixture");
    let source = dir.path().join("source.txt");
    let giant = "a".repeat(octocode_engine::security::LONG_LINE_BYTES + 3);
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
            hidden: &HiddenRows::default(),
        },
    )
    .expect("not cancelled");
    assert_eq!(verified, Verification::Verified);
    assert_eq!(file.matches[0].value, "…aaaa…");
}

/// Lines the search showed whole are checked however many bytes a page's
/// rows hold together: no retained-byte budget withholds a row.
#[test]
fn rows_of_lines_the_search_showed_whole_are_checked_whatever_they_sum_to() {
    let dir = tempfile::tempdir().expect("fixture");
    let source = dir.path().join("source.txt");
    let line = "a".repeat(octocode_engine::security::LONG_LINE_BYTES * 4 / 5);
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
            hidden: &HiddenRows::default(),
        },
    )
    .expect("not cancelled");
    assert_eq!(verified, Verification::Verified);
    assert_eq!(file.matches[1].value, "…aaaa…");
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
            hidden: &HiddenRows::default(),
        },
    )
    .expect_err("cancelled during the stream");
    assert_eq!(reason, "Cancelled");
}

/// Context lines beside a key block stay readable, and so does its armor;
/// only the key text inside the block is redacted.
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
    assert_eq!(redact_key_block_lines(2, &value, &ranges, at), value);
    let value = format!("{}\n{}\n{}", source[2], source[3], source[4]);
    assert_eq!(
        redact_key_block_lines(4, &value, &ranges, at),
        format!("{}\n{placeholder}\n{}", source[2], source[4])
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
    let read = read_verification_lines(&mut open(&source), &[(1, 1)], 1, false, &NeverCancel)
        .unwrap_or_else(|_| panic!("readable"));
    assert_eq!(
        read.retained[&1],
        octocode_engine::text::Decoded::interior(line).text
    );
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
                hidden: &HiddenRows::default(),
            },
        )
        .expect("not cancelled");
        assert_eq!(verified, Verification::Unverified, "{first}: {value:?}");
        // Its lines are placed nowhere in the source, so it is shown as it
        // was scanned: only a known secret span would be replaced.
        assert_eq!(file.matches[0].value, value, "{first}: {value:?}");
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
            hidden: &HiddenRows::default(),
        },
    )
    .expect("not cancelled");
    assert_eq!(verified, Verification::Verified);
    assert_eq!(file.matches[0].value, "l3\nl4 marker\nl5");
}

/// In a source too large to scan whole, a row touching a private key's block
/// judges its other lines in their window, not alone: a signify key secret only
/// by the comment above it stays hidden beside the block it precedes.
#[test]
fn a_key_block_row_of_a_large_source_judges_its_other_lines_in_their_window() {
    const KEY: &str = "q8Zr4LmT2vXw9KpN7sYd3HfJ6gBc1QeRu5WaVo0MiExDzlUt";
    let pad = format!(
        "# {}\n",
        "x".repeat(octocode_engine::security::LONG_LINE_BYTES + 3)
    );
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
            hidden: &HiddenRows::default(),
        },
    )
    .expect("not cancelled");
    let shown = &file.matches[0].value;
    assert!(!shown.contains("LmT2"), "{shown:?}");
    assert!(!shown.contains("BEGIN PRIVATE"), "{shown:?}");
    assert_eq!(shown.split('\n').count(), 2, "{shown:?}");
}

/// `rows` of `source` settled as a page settles them: the outcome and each
/// row's value.
fn settled(
    source: &std::path::Path,
    expected: Expected,
    rows: Vec<octocode_engine::types::TextSearchMatch>,
) -> (Verification, Vec<String>) {
    let mut file = file_with(rows);
    let outcome = guard_source_secrets(
        &mut file,
        source,
        expected,
        0..10,
        &ValueCheck {
            security: &ContentSecurity::new(),
            shape: RowShape::Lines(Default::default()),
            cancel: &NeverCancel,
            judged: None,
            hidden: &HiddenRows::default(),
        },
    )
    .expect("not cancelled");
    let values = file.matches.into_iter().map(|row| row.value).collect();
    (outcome, values)
}

/// Text bytes the whole-source pass scanned, and text bytes a page asked a
/// whole-source judgment of (scanned or kept), while `work` ran.
fn judged_while<T>(work: impl FnOnce() -> T) -> (T, usize, usize) {
    let scanned = crate::security::scanned_on_this_thread;
    let asked = super::judged_on_this_thread;
    let (scanned_before, asked_before) = (scanned(), asked());
    let done = work();
    (done, scanned() - scanned_before, asked() - asked_before)
}

/// A source a search read whole and judged clean is proved by the digest of
/// its bytes on the next search of them: no line of it is read for a scan,
/// and every row settles as the read that judged it settled it, a row past
/// its end and a stored digest included.
#[test]
fn a_clean_source_is_proved_by_its_digest_on_the_next_search() {
    let dir = tempfile::tempdir().expect("fixture");
    let source = dir.path().join("clean.ts");
    let text = "const a = 1;\nexport const b = 2;\nconst c = 3;\n";
    std::fs::write(&source, text).expect("fixture");
    let digest: super::super::sources::Digest =
        octocode_engine::digest::sha256_digest(text.as_bytes());
    let rows = || vec![hit(2, "export const b = 2;"), hit(9, "no such line")];
    let (first, scanned, asked) = judged_while(|| settled(&source, Expected::Fresh, rows()));
    assert!(
        scanned > 0 && asked > 0,
        "the first search judges the source"
    );
    assert_eq!(first.0, Verification::Unverified);
    assert_eq!(first.1, ["export const b = 2;", "no such line"]);
    for expected in [Expected::Fresh, Expected::Stored(Some(digest))] {
        let (again, scanned, asked) = judged_while(|| settled(&source, expected, rows()));
        assert_eq!(again, first, "{expected:?}");
        assert_eq!((scanned, asked), (0, 0), "{expected:?}: no line is read");
    }
    assert_eq!(
        settled(&source, Expected::Stored(Some([7; 32])), rows()).0,
        Verification::Changed
    );
    assert_eq!(
        settled(&source, Expected::Stored(None), rows()).0,
        Verification::Changed
    );
}

/// What is kept of a source answers for its bytes alone: the same path
/// holding other bytes is read and judged again, and a source the pass
/// changed shows on every search what its first search showed.
#[test]
fn a_changed_source_is_read_again_and_a_redacted_one_is_cut_the_same() {
    let dir = tempfile::tempdir().expect("fixture");
    let source = dir.path().join("changing.ts");
    let clean = "const a = 1;\nexport const b = 2;\n";
    let token = format!("ghp_{}", "m4N8x2C6".repeat(5));
    let secret = format!("const a = 1;\nexport const b = \"{token}\";\n");
    let clean_row = || vec![hit(2, "export const b = 2;")];
    let secret_line = format!("export const b = \"{token}\";");
    let secret_row = || vec![hit(2, &secret_line)];
    std::fs::write(&source, clean).expect("fixture");
    let shown = settled(&source, Expected::Fresh, clean_row());
    assert_eq!(shown.1, ["export const b = 2;"]);
    // Other bytes at the same path: judged for themselves, and hidden.
    std::fs::write(&source, &secret).expect("fixture");
    let (hidden, scanned, _) = judged_while(|| settled(&source, Expected::Fresh, secret_row()));
    assert_eq!(
        scanned,
        secret.trim_end().len(),
        "the new bytes are scanned"
    );
    assert!(!hidden.1[0].contains(&token), "{hidden:?}");
    assert!(hidden.1[0].contains("[REDACTED"), "{hidden:?}");
    // A source the pass changed is proved the same way: its rows are cut
    // from the pass kept of its text, and no line is read for a scan.
    let (again, scanned, asked) = judged_while(|| settled(&source, Expected::Fresh, secret_row()));
    assert_eq!(again, hidden);
    assert_eq!((scanned, asked), (0, 0));
    // The same text with other line breaks is other bytes, and its lines are
    // not what its bytes spell: it is read line by line, and cut the same.
    let crlf = dir.path().join("crlf.ts");
    std::fs::write(&crlf, secret.replace('\n', "\r\n")).expect("fixture");
    let first = settled(&crlf, Expected::Fresh, secret_row());
    assert_eq!(first, hidden);
    let (again, scanned, asked) = judged_while(|| settled(&crlf, Expected::Fresh, secret_row()));
    assert_eq!(again, hidden);
    assert_eq!(scanned, 0, "its pass is kept by its text");
    assert!(asked > 0, "its lines are read for the cut");
    // The first bytes again: what was kept of them still answers for them.
    std::fs::write(&source, clean).expect("fixture");
    let (back, scanned, asked) = judged_while(|| settled(&source, Expected::Fresh, clean_row()));
    assert_eq!(back, shown);
    assert_eq!((scanned, asked), (0, 0));
}

/// A search that takes a source's judgment from what an earlier one kept
/// notes it as judged whole for its own request, as the search that judged
/// it did: the output gate honors its rows as cuts of that judgment either
/// way.
#[test]
fn a_source_proved_by_its_digest_is_noted_as_judged_whole() {
    let dir = tempfile::tempdir().expect("fixture");
    let token = format!("ghp_{}", "r5T9v3B7".repeat(5));
    for (name, text, row) in [
        (
            "noted_clean.ts",
            "const a = 1;\nexport const b = 2;\n".to_owned(),
            "export const b = 2;".to_owned(),
        ),
        (
            "noted_redacted.ts",
            format!("const a = 1;\nexport const b = \"{token}\";\n"),
            format!("export const b = \"{token}\";"),
        ),
    ] {
        let source = dir.path().join(name);
        std::fs::write(&source, &text).expect("fixture");
        let search = || {
            let (security, _judged) = ContentSecurity::new().for_request();
            let mut file = file_with(vec![hit(2, &row)]);
            let (outcome, scanned, _) = judged_while(|| {
                guard_source_secrets(
                    &mut file,
                    &source,
                    Expected::Fresh,
                    0..10,
                    &ValueCheck {
                        security: &security,
                        shape: RowShape::Lines(Default::default()),
                        cancel: &NeverCancel,
                        judged: None,
                        hidden: &HiddenRows::default(),
                    },
                )
                .expect("not cancelled")
            });
            assert_eq!(outcome, Verification::Verified, "{name}");
            assert!(security.judged_whole(&source), "{name}");
            (file.matches[0].value.clone(), scanned)
        };
        let (first, scanned) = search();
        assert!(scanned > 0, "{name}: the first search scans");
        assert!(!first.contains(&token), "{name}: {first}");
        assert_eq!(search(), (first, 0), "{name}");
    }
}

/// A context row settled over a source rewritten after the search read it:
/// whatever the row shows of a secret is hidden, and a row that shows text
/// the redaction wrote is reported as one ([`HiddenRows`], or a value that
/// is no longer the one the search made).
#[test]
fn a_context_row_over_a_rewritten_source_reports_what_it_hides() {
    let token = "ghp_Ux7Kq2Lm9Zp4Vw8Rt1Yb6Nc3Hd5Jf0GsAeUoI";
    let plain = "needle 5\nneedle 6\nneedle 7\nneedle 8\n";
    let secret = format!("needle 5\nneedle 6\nneedle 7 {token}\nneedle 8\n");
    let context = octocode_engine::types::SearchContext {
        before: 1,
        after: 2,
    };
    let row = |text: &str| {
        let mut row = hit(6, text);
        row.first_line = 5;
        row
    };
    let dir = tempfile::tempdir().expect("fixture");
    let source = dir.path().join("live.txt");
    // The search read one version, the check reads the other.
    for (read_by_search, on_disk) in [
        (plain.trim_end(), secret.as_str()),
        (secret.trim_end(), plain),
    ] {
        std::fs::write(&source, on_disk).expect("rewrite");
        let mut file = file_with(vec![row(read_by_search)]);
        let before = file.matches[0].value.clone();
        let hidden = HiddenRows::default();
        let outcome = guard_source_secrets(
            &mut file,
            &source,
            Expected::Fresh,
            0..1,
            &ValueCheck {
                security: &ContentSecurity::new(),
                shape: RowShape::Lines(context),
                cancel: &NeverCancel,
                judged: None,
                hidden: &hidden,
            },
        )
        .expect("not cancelled");
        let shown = file.matches[0].value.clone();
        let noted = !hidden
            .into_redacted("source.txt")
            .collect::<Vec<_>>()
            .is_empty();
        assert!(!shown.contains(token), "{outcome:?} {shown:?}");
        if crate::security::placeholders(&shown) > 0 {
            assert!(
                noted || shown != before,
                "a row showing a replacement is reported: {outcome:?} {before:?} -> {shown:?}"
            );
        }
    }
}

/// A row whose file changed after the search matched it is rebuilt from the
/// bytes now on disk and withheld as unverified; the span the redaction
/// replaced in it is still reported as redacted.
#[test]
fn a_withheld_row_that_shows_a_replaced_span_is_reported() {
    let token = "ghp_Ux7Kq2Lm9Zp4Vw8Rt1Yb6Nc3Hd5Jf0GsAeUoI";
    let dir = tempfile::tempdir().expect("fixture");
    let (paths, security) = crate::tools::test_support::simple_policy(dir.path());
    let path = dir.path().join("live.txt");
    std::fs::write(&path, format!("needle 1\nneedle 2 {token}\nneedle 3\n")).expect("fixture");
    let mut matched = hit(2, "");
    matched.deferred = Some(1);
    let mut file = octocode_engine::types::TextSearchFile {
        term_counts: Vec::new(),
        path: path.to_string_lossy().into_owned(),
        match_count: 1,
        matches: vec![matched],
        // The bytes the search hashed are not the bytes now on disk.
        source: Some(octocode_engine::types::SearchedSource {
            size: 1,
            modified: None,
            digest: [7; 32],
            lines: None,
        }),
    };
    let query: LocalSearchQuery = serde_json::from_value(serde_json::json!({
        "path": dir.path(), "matchString": ["needle"]
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
    let mut redacted = Redacted::new();
    let verified = verify_candidate(&scope, &mut file, (&mut redacted, &Withheld::new()))
        .expect("a fresh page withholds rather than fails");
    assert!(!verified, "a withheld row is unverified");
    let shown = &file.matches[0].value;
    assert!(!shown.contains(token), "{shown:?}");
    assert!(shown.contains("[REDACTED-"), "{shown:?}");
    assert!(
        redacted.contains(&(file.path.clone(), 2, 0)),
        "the replaced span is reported: {redacted:?}"
    );
}
