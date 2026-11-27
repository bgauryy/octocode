use super::*;

#[test]
fn unchanged_lines_do_not_allocate_per_line_records() {
    let raw = format!("{}secret=original\r\nעברית🙂", "x\n".repeat(100_000));
    let clean = raw.replace("secret=original", "secret=[REDACTED]");
    let source = SafeSource::from_redacted(&raw, &clean).expect("stable line geometry");
    assert_eq!(source.changed.len(), 1);
    assert_eq!(source.slice(&(0..raw.len())).expect("whole source"), clean);
    let unchanged = SafeSource::from_redacted(&raw, &raw).expect("unchanged source");
    assert!(unchanged.changed.is_empty());
    assert_eq!(unchanged.slice(&(0..4)).expect("prefix"), "x\nx\n");
    assert!(SafeSource::from_redacted("a\nb", "a b").is_err());
}

/// What a section is planned from, by the steps of [`SafeSource::new`]:
/// no part of an RFC 4716 key's body reaches any slice of it.
#[test]
fn an_rfc4716_key_body_reaches_no_section() {
    use octocode_runtime_base::security::scan::ContentScan;
    let body = "UvImZaYMEtKJGF2VDuiBNgkWb2sRPReNbA/TkB/yOaGglfIPk5VlDPk4C47bIkpr";
    let raw = format!(
        "one\n---- BEGIN SSH2 ENCRYPTED PRIVATE KEY ----\nComment: \"fake\"\n{body}\n---- END SSH2 ENCRYPTED PRIVATE KEY ----\nlast\n"
    );
    let security = octocode_runtime_base::security::ContentSecurity::new();
    let (blocks, _) = security.redact_key_blocks(&raw);
    let (safe, _) = security
        .redact_source_lines(&blocks, std::path::Path::new("k.txt"))
        .expect("sanitized");
    let source = SafeSource::from_redacted(&raw, &safe).expect("stable line geometry");
    let whole = source.slice(&(0..raw.len())).expect("whole source");
    assert!(!whole.contains(body), "{whole}");
    assert!(
        whole.starts_with("one\n") && whole.ends_with("last\n"),
        "{whole}"
    );
}

/// Every coordinate and the digest count the text a read shows: a
/// source byte past a changed line moves by what that line grew or
/// shrank, one inside it is placed at the line's shown bounds, and the
/// digest is the shown text's.
#[test]
fn shown_coordinates_and_digest_count_the_shown_text() {
    let raw = "a\nsecret=original-value\nb\n";
    let clean = "a\nsecret=[REDACTED]\nb\n";
    let source = SafeSource::from_redacted(raw, clean).expect("stable line geometry");
    let line = 2..raw.len() - 2;
    assert_eq!(
        source.digest(),
        octocode_engine::digest::sha256(clean.as_bytes())
    );
    assert_eq!(source.shown_start(0), 0);
    assert_eq!(source.shown_start(line.start), 2);
    assert_eq!(source.shown_end(line.end), clean.len() - 2);
    assert_eq!(source.shown_end(raw.len()), clean.len());
    assert_eq!(source.shown_start(line.start + 3), 2);
    assert_eq!(source.shown_end(line.start + 3), clean.len() - 2);
    let unchanged = SafeSource::from_redacted(raw, raw).expect("unchanged");
    assert_eq!(
        unchanged.digest(),
        octocode_engine::digest::sha256(raw.as_bytes())
    );
    assert_eq!(unchanged.shown_end(7), 7);
}

#[test]
fn redaction_is_whole_line_or_explicitly_unavailable_never_a_partial_secret() {
    let raw = "🙂 safe\r\nsecret=original\nעברית\n";
    let first = "🙂 safe\r\n".len();
    let second = first + "secret=original\n".len();
    let source = SafeSource {
        raw,
        changed: vec![(first..second, "secret=[REDACTED]\n".into())],
    };
    assert_eq!(source.slice(&(0..4)).expect("UTF-8 prefix"), "🙂");
    assert_eq!(
        source.slice(&(first..second)).expect("whole redacted line"),
        "secret=[REDACTED]\n"
    );
    assert!(source.slice(&(first + 2..second)).is_err());
    assert!(source.slice(&(0..second - 1)).is_err());
    let complete = source.slice(&(0..raw.len())).expect("whole source");
    assert!(!complete.contains("original"));
    assert!(complete.contains("עברית"));
}
