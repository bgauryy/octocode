use super::*;

#[test]
fn key_block_ranges_stay_bounded_and_fail_closed_past_the_cap() {
    let block = "-----BEGIN PRIVATE KEY-----\nQUJDQUJD\n-----END PRIVATE KEY-----\n";
    let blocks = MAX_KEY_BLOCK_RANGES + 50;
    let content = format!("{}tail line\n", block.repeat(blocks));
    let ranges = private_key_block_line_ranges(&content);
    assert_eq!(ranges.len(), MAX_KEY_BLOCK_RANGES + 1);
    assert_eq!(ranges[0], (1, 3));
    let saturated_at = u32::try_from(MAX_KEY_BLOCK_RANGES * 3 + 1).expect("small");
    assert_eq!(ranges.last(), Some(&(saturated_at, u32::MAX)));
    // Every later line, key or not, reads as key material.
    let last_line = u32::try_from(blocks * 3 + 1).expect("small");
    assert!(match_window_intersects_key_block(
        last_line,
        "tail line",
        &ranges
    ));
    // Under the cap, ranges stay exact.
    let exact = private_key_block_line_ranges(&format!("{}tail line\n", block.repeat(3)));
    assert_eq!(exact, vec![(1, 3), (4, 6), (7, 9)]);
    assert!(!match_window_intersects_key_block(11, "tail line", &exact));
}
#[test]
fn split_private_key_guard_targets_only_key_markers() {
    // Body-only base64 with no key marker → guard is a no-op (must not
    // redact ordinary base64 that happens to appear in source).
    assert!(redact_split_private_key("aGVsbG8gd29ybGQ=\nc29tZSBkYXRhIGhlcmU=", None).is_none());
    // A window holding only the BEGIN boundary + body (END is in the next
    // window) → the body must be redacted.
    let out = redact_split_private_key(
        "config header line\n-----BEGIN RSA PRIVATE KEY-----\nMIIEpQIBAAKCAQEA7Yn8xK2vJ9qLmN3pQrSt",
        None,
    )
    .expect("split private key must be redacted");
    assert!(!out.contains("MIIEpQIB"), "key body leaked: {out}");
    assert!(
        out.contains("config header line"),
        "non-secret context dropped"
    );
    assert_eq!(out.lines().count(), 3, "line count must be preserved");
    // A public certificate boundary is not a private key → no-op.
    assert!(redact_split_private_key("-----BEGIN CERTIFICATE-----\nMIIBpayload", None).is_none());
}

#[test]
fn full_file_key_block_redaction_preserves_lines_and_surrounding_code() {
    let body = "MIIEpQIBAAKCAQEAsplitKeyBodyAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let body2 = "c3BsaXRLZXlCb2R5VHdvQkFCQUJBQkFCQUJBQkFCQUJBQkFCQUJBQkFCQUJBQkFC";
    let file = format!(
        "fn main() {{}}\n-----BEGIN RSA PRIVATE KEY-----\n{body}\n{body2}\n-----END RSA PRIVATE KEY-----\nlet done = true;\n"
    );
    let (out, changed) = redact_private_key_blocks(&file);
    assert!(changed, "a private-key block must be redacted");
    assert!(
        !out.contains(body) && !out.contains(body2),
        "key body leaked: {out}"
    );
    assert!(out.contains("fn main() {}"), "leading code dropped: {out}");
    assert!(
        out.contains("let done = true;"),
        "trailing code dropped: {out}"
    );
    assert_eq!(
        out.lines().count(),
        file.lines().count(),
        "line count must be preserved for accurate source-line ranges"
    );
    assert!(out.ends_with('\n'), "trailing newline must be preserved");
}

#[test]
fn markers_inside_string_literals_open_and_close_a_key_block() {
    // A key pasted into source as a template string or triple-quoted
    // string: the marker follows code on its line and the END marker is
    // followed by the closing quote.
    let body = "MIIEpQIBAAKCAQEAinlineKeyBodyAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    for (open, close) in [
        (
            "const key = `-----BEGIN RSA PRIVATE KEY-----",
            "-----END RSA PRIVATE KEY-----`;",
        ),
        (
            "KEY = \"\"\"-----BEGIN PRIVATE KEY-----",
            "-----END PRIVATE KEY-----\"\"\"",
        ),
    ] {
        let file = format!("use x;\n{open}\n{body}\n{close}\nlet done = true;\n");
        let (out, changed) = redact_private_key_blocks(&file);
        assert!(changed, "inline marker not recognised: {open}");
        assert!(!out.contains(body), "key body leaked: {out}");
        assert!(
            out.contains("let done = true;"),
            "trailing code dropped: {out}"
        );
        assert_eq!(out.lines().count(), file.lines().count());
    }
    assert!(is_private_key_boundary(
        "  key = `-----BEGIN OPENSSH PRIVATE KEY-----`"
    ));
    assert!(!is_private_key_boundary("-----BEGIN CERTIFICATE-----"));
    assert!(!is_private_key_boundary(
        "PRIVATE KEY----- is how a marker ends"
    ));
}

#[test]
fn full_file_redaction_leaves_innocent_and_certificate_content_byte_identical() {
    // Ordinary base64 with no private-key marker — must not be touched.
    let blob = "header\naGVsbG8gd29ybGQgbG9uZyBiYXNlNjQgYmxvYiBoZXJlIQ==\nfooter\n";
    let (out, changed) = redact_private_key_blocks(blob);
    assert!(
        !changed && out == blob,
        "innocent base64 was altered: {out}"
    );
    // A public certificate is not a private key — untouched.
    let cert = "-----BEGIN CERTIFICATE-----\nMIIBpublicCertBody\n-----END CERTIFICATE-----\n";
    let (out, changed) = redact_private_key_blocks(cert);
    assert!(
        !changed && out == cert,
        "certificate was wrongly redacted: {out}"
    );
}

/// A document that names PuTTY's fields without their values holds no key
/// and reads as written.
#[test]
fn putty_field_names_without_values_hide_nothing() {
    let doc = "//! `Private-Lines:` opens a block, and\n//! `Private-MAC:` closes it.\n";
    let (out, changed) = redact_private_key_blocks(doc);
    assert!(!changed && out == doc, "{out}");
}

#[test]
fn full_file_redaction_covers_unterminated_block_through_eof() {
    // A BEGIN with no matching END must still redact the trailing body.
    let body = "MIIEpQIBAAKCAQEAunterminatedKeyBodyAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let file = format!("ok\n-----BEGIN OPENSSH PRIVATE KEY-----\n{body}\n");
    let (out, changed) = redact_private_key_blocks(&file);
    assert!(changed, "unterminated block must redact");
    assert!(!out.contains(body), "unterminated key body leaked: {out}");
    assert!(out.contains("ok"), "pre-block content dropped: {out}");
}

#[test]
fn unterminated_marker_redacts_only_the_base64_run_after_it() {
    // A literal marker string in ordinary source opens a block that never
    // closes: the marker line is the block's only line, and an armor holds no
    // key text, so the source reads as written, the closing quote included.
    let literal = "let a = 1;\nconst PEM = \"-----BEGIN RSA PRIVATE KEY-----\";\nlet b = compute(a);\nfn main() {}\n";
    assert_eq!(private_key_block_line_ranges(literal), vec![(2, 2)]);
    let (out, changed) = redact_private_key_blocks(literal);
    assert!(!changed, "{out}");
    assert_eq!(out, literal);
    // A BEGIN literal and a far-away END literal do not make the code
    // between them a key body.
    let far = "a\n\"-----BEGIN X PRIVATE KEY-----\"\nlet x = 1;\nlet y = 2;\n\"-----END X PRIVATE KEY-----\"\n";
    assert_eq!(private_key_block_line_ranges(far), vec![(2, 2)]);
}

#[test]
fn unterminated_key_body_stays_redacted_through_its_base64_run() {
    let body = "MIIEpQIBAAKCAQEAunterminatedKeyBodyAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let file = format!(
        "ok\n-----BEGIN RSA PRIVATE KEY-----\nProc-Type: 4,ENCRYPTED\nDEK-Info: AES-128-CBC,0123ABCD\n\n{body}\n{body}\nQUJD==\nfn main() {{}}\n"
    );
    // Armor headers hold no key bytes: only the marker and the body are hidden.
    assert_eq!(private_key_block_line_ranges(&file), vec![(2, 2), (6, 8)]);
    let (out, changed) = redact_private_key_blocks(&file);
    assert!(changed);
    assert!(!out.contains(body) && !out.contains("QUJD"), "{out}");
    assert!(out.contains("DEK-Info"), "{out}");
    assert!(out.contains("fn main() {}"), "{out}");
    assert_eq!(out.lines().count(), file.lines().count());
}

#[test]
fn quoted_and_concatenated_key_bodies_stay_inside_the_block() {
    let body = "MIIEpQIBAAKCAQEAquotedKeyBodyAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let terminated = format!(
        "x\nconst k = \"-----BEGIN RSA PRIVATE KEY-----\\n\" +\n  \"{body}\\n\" +\n  \"{body}\\n\" +\n  \"-----END RSA PRIVATE KEY-----\";\nlet done = true;\n"
    );
    assert_eq!(private_key_block_line_ranges(&terminated), vec![(2, 5)]);
    let unterminated = format!(
        "x\nconst k = \"-----BEGIN RSA PRIVATE KEY-----\\n\" +\n  \"{body}\\n\" +\n  \"{body}\\n\";\nlet done = true;\n"
    );
    assert_eq!(private_key_block_line_ranges(&unterminated), vec![(2, 4)]);
    let (out, _) = redact_private_key_blocks(&unterminated);
    assert!(!out.contains(body), "{out}");
    assert!(out.contains("let done = true;"), "{out}");
}

#[test]
fn key_block_tracker_streams_the_same_ranges_as_the_whole_file_scan() {
    let body = "MIIEpQIBAAKCAQEAstreamedKeyBodyAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    for file in [
        format!("a\n-----BEGIN PRIVATE KEY-----\n{body}\n-----END PRIVATE KEY-----\nb\n"),
        format!("a\n\"-----BEGIN PRIVATE KEY-----\"\nlet b = 2;\n{body}\n"),
        format!("a\n-----BEGIN PRIVATE KEY-----\n{body}\nlet b = 2;\n-----END PRIVATE KEY-----\n"),
    ] {
        let mut tracker = KeyBlockTracker::default();
        for line in file.lines() {
            tracker.push(line);
        }
        assert_eq!(tracker.finish(), private_key_block_line_ranges(&file));
    }
}

/// A line of an open block that only holds a key's run keeps what precedes
/// the run: a label or code is no key text. A body line as a whole (quoted,
/// commented, indented) hides its key text, its quoting read as written, and
/// a line with no run that long is not hidden at all.
#[test]
fn a_line_that_only_holds_a_key_run_keeps_its_label() {
    const RUN: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";
    const BODY: &str = "WzHvuy3fEWJr5musirUr5JYe5CMpm8DI2mKvqGsxgG5oDTudHZGh0Z7tcHL5kVeqJy";
    let placeholder = key_fragment_placeholder();
    let source = format!(
        "logo: {RUN}\n-----BEGIN OPENSSH PRIVATE KEY-----\n{BODY}\n  \"{BODY}\\n\" +\nordinary tail text\nlogo2: {RUN}\n  k += b\"{BODY}\"\nshort: abc123\nlast plain line\n"
    );
    let (hidden, changed) = redact_private_key_blocks(&source);
    assert!(changed);
    assert_eq!(
        hidden,
        format!(
            "logo: {RUN}\n-----BEGIN OPENSSH PRIVATE KEY-----\n{placeholder}\n  \"{placeholder}\" +\nordinary tail text\nlogo2: {placeholder}\n  k += b\"{placeholder}\"\nshort: abc123\nlast plain line\n"
        )
    );
    // One rule for every reader of a line: the row a search shows of it.
    assert_eq!(
        hide_key_line(&format!("logo2: {RUN}")),
        format!("logo2: {placeholder}")
    );
    assert_eq!(
        hide_key_line(&format!("\t{BODY}")),
        format!("\t{placeholder}")
    );
    // Only the run goes: what follows it on the line reads as written.
    assert_eq!(
        hide_key_line(&format!("logo2: {RUN}\"); next();")),
        format!("logo2: {placeholder}\"); next();")
    );
    // A label longer than the head a streamed read keeps leaves the line a
    // body line, in every reader alike.
    let long_label = "x ".repeat(KEY_LINE_HEAD_BYTES / 2);
    assert_eq!(hide_key_line(&format!("{long_label}: {RUN}")), placeholder);
}

/// Hiding a key keeps the source's syntax: a key line's quotes, separators,
/// comment marks and concatenation read as written around the placeholder,
/// so a parse of the shown text pairs the same delimiters the source does.
/// No key byte shows: padding, escapes and base64 symbols beside the key go
/// with it, and a line whose key text sits among code (`"MIIE…", "MIIE…"`)
/// hides whole rather than unpair a delimiter.
#[test]
fn hiding_a_key_keeps_the_delimiters_around_it() {
    const BODY: &str = "MIIEpQIBAAKCAQEA7Yn8xK2vJ9qLmN3pQrStWzHvuy3fEWJr5musirUr5JYe5CMp";
    let ph = key_fragment_placeholder();
    for (line, shown) in [
        // A Rust string: a split key's BEGIN line (the round-3 repro).
        (
            format!("        \"config header line\\n-----BEGIN RSA PRIVATE KEY-----\\n{BODY}\","),
            format!("        \"config header line\\n-----BEGIN RSA PRIVATE KEY-----{ph}\","),
        ),
        (format!("  \"{BODY}\\n\" +"), format!("  \"{ph}\" +")),
        (format!("  + \"{BODY}\""), format!("  + \"{ph}\"")),
        (format!("    '{BODY}==',"), format!("    '{ph}',")),
        (format!("    \"{BODY}\\/+\","), format!("    \"{ph}\",")),
        (format!("    `{BODY}`;"), format!("    `{ph}`;")),
        (format!("/* {BODY} */"), format!("/* {ph} */")),
        (format!("    ({BODY}),"), format!("    ({ph}),")),
        ("        None,".to_owned(), format!("        {ph},")),
        // Code before a key's run is its label, kept as written.
        (format!("  foo(\"{BODY}"), format!("  foo(\"{ph}")),
        // Two runs on a line: hidden from the first through the last, the
        // code between them with them, never half a pair inside the key.
        (
            format!("    \"{BODY}\", \"{BODY}\","),
            format!("    \"{ph}\","),
        ),
    ] {
        let hidden = hide_key_line(&line);
        assert_eq!(hidden, shown, "{line}");
        for piece in BODY.as_bytes().chunks(8) {
            let piece = std::str::from_utf8(piece).unwrap_or_default();
            assert!(!hidden.contains(piece), "key bytes {piece} shown: {hidden}");
        }
        let pairs = |text: &str| {
            text.bytes()
                .filter(|byte| b"\"'`()[]{}".contains(byte))
                .filter(|byte| !ph.as_bytes().contains(byte))
                .count()
        };
        assert_eq!(
            pairs(&line) % 2,
            pairs(&hidden) % 2,
            "delimiters unpaired: {hidden}"
        );
    }
    // A hidden body line hidden again is unchanged.
    for hidden in [
        format!("  \"{ph}\" +"),
        format!("    '{ph}',"),
        format!("/* {ph} */"),
        format!("        {ph},"),
        ph.clone(),
    ] {
        assert_eq!(hide_key_line(&hidden), hidden);
    }
    // A source whose key is hidden parses as its raw text does: the string
    // the BEGIN line opens still closes on that line.
    let source = format!(
        "let out = f(\n    \"head\\n-----BEGIN RSA PRIVATE KEY-----\\n{BODY}\",\n    None,\n)\n.expect(\"x\");\n"
    );
    let (hidden, changed) = redact_private_key_blocks(&source);
    assert!(changed);
    assert!(!hidden.contains(&BODY[..12]), "{hidden}");
    assert_eq!(
        hidden.matches('"').count(),
        source.matches('"').count(),
        "{hidden}"
    );
}

/// D3: a key block written inside one member of a minified document starts
/// at its own `-----BEGIN`, wherever on the line that stands: the members
/// before it (a run of their own among them), its armor and the text after
/// its END stay; only the body between the armors goes.
#[test]
fn a_key_block_in_a_minified_document_starts_at_its_own_begin() {
    let before = format!(
        "{{\"password\":\"Tr0ub4dor&3xK9!zQ\",\"apiKey\":\"{}\",\"name\":\"visible-name\",\"privateKey\":\"",
        "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
    );
    let block = "-----BEGIN PRIVATE KEY-----\\nMIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQC7VJTUt9Us8cKj\\nMzEfYyjiWA4R4/M2bS1GB4t7NXp98C3SC6dVMvDuictGeurT8jNbvJZHtCSuYEvu\\n-----END PRIVATE KEY-----";
    let after = "\"}";
    for lead in ["", &"{\"pad\":\"x\",".repeat(30)] {
        let line = format!("{lead}{before}{block}{after}");
        let source = format!("{line}\n");
        let (hidden, changed) = redact_private_key_blocks(&source);
        assert!(changed, "{line}");
        assert_eq!(
            hidden,
            format!(
                "{lead}{before}-----BEGIN PRIVATE KEY-----{}-----END PRIVATE KEY-----{after}\n",
                key_fragment_placeholder()
            ),
            "{} bytes before the block",
            lead.len() + before.len()
        );
    }
}

/// Owner rule: an armor line names its block and holds none of it. Only key
/// text written beside an armor is a line's key: a body on the BEGIN line,
/// before the END, or between the two of a block on one line; PuTTY's MAC
/// value, not its name. A quote, a comment's tail or code beside the armor
/// is no key text, and every format's armor reads whole.
#[test]
fn only_key_text_beside_an_armor_is_a_lines_key() {
    const BODY: &str = "MIIEowIBAAKCAQEAxyzMIIEowIBAAKCAQEAxyzMIIEowIBAAKCAQEAxyz";
    let placeholder = key_fragment_placeholder();
    for armor in [
        "-----BEGIN RSA PRIVATE KEY-----",
        "-----END RSA PRIVATE KEY-----",
        "  -----BEGIN OPENSSH PRIVATE KEY-----\r",
        "---- BEGIN SSH2 ENCRYPTED PRIVATE KEY ----",
        "-----BEGIN PGP PRIVATE KEY BLOCK-----",
        "-----BEGIN OpenVPN Static key V1-----",
        "-----BEGIN RSA PRIVATE KEY",
        "private_key: \"-----BEGIN RSA PRIVATE KEY-----\"",
        "/// Whether `secret` is one PEM armor line (`-----BEGIN RSA PRIVATE KEY-----`):",
        "const k = \"-----BEGIN RSA PRIVATE KEY-----\\n\" +",
        "-----END PRIVATE KEY----------BEGIN PRIVATE KEY-----",
        "PuTTY-User-Key-File-3: ssh-ed25519",
        "Private-Lines: 1",
    ] {
        assert_eq!(key_line_bytes(armor), None, "{armor}");
        assert_eq!(hide_key_line(armor), armor);
    }
    for (line, shown) in [
        (
            format!("-----BEGIN RSA PRIVATE KEY-----{BODY}"),
            format!("-----BEGIN RSA PRIVATE KEY-----{placeholder}"),
        ),
        (
            format!("  {BODY}-----END RSA PRIVATE KEY-----"),
            format!("  {placeholder}-----END RSA PRIVATE KEY-----"),
        ),
        (
            format!("x=\"-----BEGIN PRIVATE KEY-----\\n{BODY}\\n-----END PRIVATE KEY-----\" y"),
            format!("x=\"-----BEGIN PRIVATE KEY-----{placeholder}-----END PRIVATE KEY-----\" y"),
        ),
        (
            "Private-MAC: 0123456789abcdef".to_owned(),
            format!("Private-MAC: {placeholder}"),
        ),
        (format!("    {BODY}"), format!("    {placeholder}")),
    ] {
        assert_eq!(hide_key_line(&line), shown, "{line}");
        assert!(!hide_key_line(&line).contains(&BODY[..16]), "{line}");
    }
    // A block on lines of its own: armor shown, body hidden, line for line,
    // and one with no END still hides its body.
    let block = format!(
        "{}\n{BODY}\n{BODY}\n{}\n",
        "-----BEGIN RSA PRIVATE KEY-----", "-----END RSA PRIVATE KEY-----"
    );
    let (hidden, _) = redact_private_key_blocks(&block);
    assert_eq!(
        hidden,
        format!(
            "-----BEGIN RSA PRIVATE KEY-----\n{placeholder}\n{placeholder}\n-----END RSA PRIVATE KEY-----\n"
        )
    );
    let open = format!("-----BEGIN RSA PRIVATE KEY-----\n{BODY}\n{BODY}\n");
    let (hidden, _) = redact_private_key_blocks(&open);
    assert_eq!(
        hidden,
        format!("-----BEGIN RSA PRIVATE KEY-----\n{placeholder}\n{placeholder}\n")
    );
}
