use super::*;

#[test]
fn only_an_https_url_without_credentials_is_accepted() {
    for (address, needle) in [
        ("http://docs.example.org/guide", "HTTPS"),
        ("ftp://docs.example.org/guide", "HTTPS"),
        ("https://user:secret@docs.example.org/guide", "credentials"),
        ("https://token@docs.example.org/guide", "credentials"),
    ] {
        let error = parse(address).unwrap_err();
        assert_eq!(error.code, ErrorCode::SecurityValidationFailed, "{address}");
        assert!(
            error.message.contains(needle),
            "{address}: {}",
            error.message
        );
        assert!(!error.message.contains("secret"), "{}", error.message);
    }
    assert_eq!(
        parse("not a url").unwrap_err().code,
        ErrorCode::InvalidClassificationContext
    );
    assert!(parse("https://docs.example.org/guide?q=1#rotation").is_ok());
}

#[test]
fn a_fragment_selects_its_heading_section_and_names_it() {
    let fake = Fake::default().page(
        "https://docs.example.org/guide",
        "text/html; charset=utf-8",
        PAGE,
    );
    let selection = run(&fake, "https://docs.example.org/guide#rotation", 10_000).unwrap();
    assert_eq!(
        selection.text,
        "## Rotation\nUpdate the configuration, then restart.\n### Check\nRun status.\n"
    );
    assert_eq!(selection.section.as_deref(), Some("rotation"));
    assert_eq!(
        selection.url.as_str(),
        "https://docs.example.org/guide#rotation"
    );
    assert_eq!(
        *fake.requested.borrow(),
        ["https://docs.example.org/guide"],
        "the fragment is never sent"
    );
}

#[test]
fn without_a_fragment_the_whole_page_is_captured() {
    let fake = Fake::default().page("https://docs.example.org/guide", "text/html", PAGE);
    let selection = run(&fake, "https://docs.example.org/guide", 10_000).unwrap();
    assert_eq!(
        selection.text,
        "# Guide\nIntro.\n## Rotation\nUpdate the configuration, then restart.\n### Check\nRun status.\n## FAQ\nNone.\n"
    );
    assert_eq!(selection.section, None);
}

#[test]
fn a_percent_encoded_fragment_matches_its_decoded_anchor() {
    let page = r#"<h2 id="über uns">Über uns</h2><p>Team.</p>"#;
    let fake = Fake::default().page("https://docs.example.org/a", "text/html", page);
    let selection = run(&fake, "https://docs.example.org/a#%C3%BCber%20uns", 1_000).unwrap();
    assert_eq!(selection.text, "## Über uns\nTeam.\n");
    assert_eq!(selection.section.as_deref(), Some("über uns"));
}

#[test]
fn a_missing_or_ambiguous_anchor_fails_naming_the_fragment() {
    let fake = Fake::default()
        .page("https://docs.example.org/guide", "text/html", PAGE)
        .page(
            "https://docs.example.org/twice",
            "text/html",
            r#"<h2 id="x">A</h2><p>a</p><p id="x">b</p>"#,
        );
    let missing = run(&fake, "https://docs.example.org/guide#absent", 10_000)
        .err()
        .unwrap();
    assert_eq!(missing.code, ErrorCode::ClassificationContextEmpty);
    assert!(missing.message.contains("#absent"), "{}", missing.message);
    assert!(
        missing.message.contains("No element"),
        "{}",
        missing.message
    );
    let ambiguous = run(&fake, "https://docs.example.org/twice#x", 10_000)
        .err()
        .unwrap();
    assert_eq!(ambiguous.code, ErrorCode::ClassificationContextEmpty);
    assert!(
        ambiguous.message.contains("2 elements") && ambiguous.message.contains("#x"),
        "{}",
        ambiguous.message
    );
}

#[test]
fn text_over_the_state_limit_fails_instead_of_being_cut() {
    let fake = Fake::default().page("https://docs.example.org/guide", "text/html", PAGE);
    let whole = run(&fake, "https://docs.example.org/guide", 10_000).unwrap();
    let limit = whole.text.chars().count();
    assert!(run(&fake, "https://docs.example.org/guide", limit).is_ok());
    let error = run(&fake, "https://docs.example.org/guide", limit - 1)
        .err()
        .unwrap();
    assert_eq!(error.code, ErrorCode::ClassificationContextTooLarge);
    assert!(error.message.contains(&format!("{}-character", limit - 1)));
    // The section alone fits where the page does not.
    assert!(run(&fake, "https://docs.example.org/guide#faq", 20).is_ok());
}

#[test]
fn plain_text_and_markdown_are_captured_whole_even_with_a_fragment() {
    let fake = Fake::default()
        .page(
            "https://docs.example.org/notes.txt",
            "text/plain; charset=UTF-8",
            "one\r\ntwo",
        )
        .page(
            "https://docs.example.org/notes.md",
            "text/markdown",
            "# Title\n\n## Part\nbody\n",
        );
    let text = run(&fake, "https://docs.example.org/notes.txt#part", 1_000).unwrap();
    assert_eq!(text.text, "one\ntwo\n");
    assert_eq!(
        text.section, None,
        "a fragment does not apply to plain text"
    );
    let markdown = run(&fake, "https://docs.example.org/notes.md", 1_000).unwrap();
    assert_eq!(markdown.text, "# Title\n\n## Part\nbody\n");
}

#[test]
fn unsupported_content_is_refused_not_judged() {
    let fake = Fake::default()
        .bytes(
            "https://e.example.org/doc.pdf",
            Some("application/pdf"),
            b"%PDF-1.7".to_vec(),
        )
        .bytes("https://e.example.org/untyped", None, b"text".to_vec())
        .bytes(
            "https://e.example.org/latin1",
            Some("text/html; charset=ISO-8859-1"),
            b"<p>caf\xe9</p>".to_vec(),
        )
        .bytes(
            "https://e.example.org/invalid",
            Some("text/plain"),
            b"caf\xe9 ok".to_vec(),
        )
        .bytes(
            "https://e.example.org/binary",
            Some("text/plain"),
            b"a\0b\0c\0".to_vec(),
        )
        .page(
            "https://e.example.org/app",
            "text/html",
            r#"<div id="root"></div><script src="/app.js"></script>"#,
        );
    for (path, needle) in [
        ("doc.pdf", "application/pdf"),
        ("untyped", "untyped"),
        ("latin1", "iso-8859-1"),
        ("invalid", "UTF-8"),
        ("binary", "binary"),
        ("app", "scripts"),
    ] {
        let error = run(&fake, &format!("https://e.example.org/{path}"), 1_000)
            .err()
            .unwrap();
        assert_eq!(
            error.code,
            ErrorCode::ClassificationContextUnavailable,
            "{path}"
        );
        assert!(error.message.contains(needle), "{path}: {}", error.message);
    }
    let empty = Fake::default().page("https://e.example.org/blank", "text/html", "<p> </p>");
    assert_eq!(
        run(&empty, "https://e.example.org/blank", 1_000)
            .err()
            .unwrap()
            .code,
        ErrorCode::ClassificationContextEmpty
    );
}

#[test]
fn a_byte_order_mark_is_not_page_text() {
    let fake = Fake::default().page("https://e.example.org/t", "text/plain", "\u{feff}hello");
    assert_eq!(
        run(&fake, "https://e.example.org/t", 100).unwrap().text,
        "hello\n"
    );
}

#[test]
fn captured_text_is_redacted_like_every_reader_output() {
    let secret = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";
    let page = format!("<p>token {secret} here</p>");
    let fake = Fake::default().bytes(
        "https://a.example.org/t",
        Some("text/html"),
        page.into_bytes(),
    );
    let text = run(&fake, "https://a.example.org/t", 1_000).unwrap().text;
    assert!(!text.contains(secret), "{text}");
    assert!(
        text.starts_with("token ") && text.ends_with(" here\n"),
        "{text}"
    );
}

#[test]
fn a_stopped_call_ends_html_extraction_with_its_own_reason() {
    let fake = Fake::default().page("https://docs.example.org/guide", "text/html", PAGE);
    let security = ContentSecurity::new();
    let redaction = Redaction {
        security: &security,
        emails: false,
    };
    for reason in [
        crate::execution::ExecutionError::Timeout,
        crate::execution::ExecutionError::Cancelled,
    ] {
        let expected = if reason == crate::execution::ExecutionError::Timeout {
            ErrorCode::Timeout
        } else {
            ErrorCode::Cancelled
        };
        let check = || Err(stopped(reason));
        let error = select(
            &fake,
            parse("https://docs.example.org/guide").unwrap(),
            &redaction,
            &check,
            10_000,
        )
        .err()
        .unwrap();
        assert_eq!(error.code, expected);
    }
}
