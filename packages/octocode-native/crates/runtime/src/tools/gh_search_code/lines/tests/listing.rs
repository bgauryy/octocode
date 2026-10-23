//! The hit lines a scan lists: numbering, windows, caps, resume points and sanitizing.
use super::*;

/// The resume line is where the next unshown hit can start, only
/// when the shown lines are exactly the first hits and none was cut.
#[test]
fn a_listing_that_is_a_hit_prefix_resumes_after_its_last_line() {
    let text = "x\n".repeat(2) + &"hit()\n".repeat(8);
    let FileHits::Lines {
        lines,
        total,
        resume_after,
        ..
    } = scan(text.as_bytes(), "a.rs", &["hit".into()], "", &Passthrough)
    else {
        panic!("hits")
    };
    assert_eq!((lines.len(), total), (MAX_LINES_PER_FILE, 8));
    assert_eq!(resume_after, Some(7));
    // The last hit is shown: nothing is left to continue.
    let text = "hit()\n".repeat(3);
    let FileHits::Lines { resume_after, .. } =
        scan(text.as_bytes(), "a.rs", &["hit".into()], "", &Passthrough)
    else {
        panic!("hits")
    };
    assert_eq!(resume_after, Some(3));
    // A listing that skipped a hit line (a keyword-only line beside
    // lines holding every keyword) is not a prefix of the hits.
    let text = "alpha\n".to_owned() + &"alpha beta\n".repeat(6);
    let FileHits::Lines { resume_after, .. } = scan(
        text.as_bytes(),
        "a.rs",
        &["alpha".into(), "beta".into()],
        "",
        &Passthrough,
    ) else {
        panic!("hits")
    };
    assert_eq!(resume_after, None);
    // A cut line cannot be resumed after: its tail was not shown.
    let text = format!("hit {}\n{}", "a".repeat(400), "hit\n".repeat(7));
    let FileHits::Lines { resume_after, .. } =
        scan(text.as_bytes(), "a.rs", &["hit".into()], "", &Passthrough)
    else {
        panic!("hits")
    };
    assert_eq!(resume_after, None);
}

#[test]
fn every_keyword_line_is_numbered_case_insensitively() {
    let text = "import x\nawait Wrap_App(app)\nother\n    await wrap_app(s)\n";
    let hits = scan(
        text.as_bytes(),
        "a.py",
        &["wrap_app".into()],
        "",
        &Passthrough,
    );
    assert_eq!(
        hits,
        FileHits::Lines {
            lines: vec![
                "2\tawait Wrap_App(app)".into(),
                "4\t    await wrap_app(s)".into()
            ],
            first: 2,
            last: 4,
            best: 2,
            total: 2,
            line_count: 4,
            clipped: false,
            declaration: None,
            resume_after: Some(4),
        }
    );
    assert_eq!(
        scan(
            text.as_bytes(),
            "a.py",
            &["absent".into()],
            "",
            &Passthrough
        ),
        FileHits::Unmatched
    );
    assert_eq!(
        scan(
            b"bin\0ary needle",
            "a.bin",
            &["needle".into()],
            "",
            &Passthrough
        ),
        FileHits::Unavailable
    );
    // A NUL past the first 8 KiB is binary too (the whole-file rule).
    let mut late_nul = "a\n".repeat(5_000).into_bytes();
    late_nul.extend_from_slice(b"needle\0");
    assert_eq!(
        scan(&late_nul, "a.bin", &["needle".into()], "", &Passthrough),
        FileHits::Unavailable
    );
}

/// GitHub file bytes read as local files do: a BOM-marked UTF-16 file is
/// text and is scanned, its lines numbered from 1.
#[test]
fn a_utf16_file_scans_like_a_local_read() {
    let utf16: Vec<u8> = [0xff, 0xfe]
        .into_iter()
        .chain(
            "first\nthe needle\n"
                .encode_utf16()
                .flat_map(u16::to_le_bytes),
        )
        .collect();
    match scan(&utf16, "a.txt", &["needle".into()], "", &Passthrough) {
        FileHits::Lines { first, lines, .. } => {
            assert_eq!(first, 2);
            assert!(
                lines.iter().any(|line| line.contains("the needle")),
                "{lines:?}"
            );
        }
        other => panic!("scanned as text: {other:?}"),
    }
}

#[test]
fn long_lines_keep_a_window_around_the_keyword() {
    let line = format!("{}needle{}", "a".repeat(500), "b".repeat(500));
    let FileHits::Lines { lines, clipped, .. } = scan(
        line.as_bytes(),
        "a.js",
        &["needle".into()],
        "",
        &Passthrough,
    ) else {
        panic!("hit");
    };
    // The cut is flagged so the page carries a read of the whole line.
    assert!(clipped);
    assert!(lines[0].starts_with("1\t…"), "{}", lines[0]);
    assert!(lines[0].contains("needle"));
    assert!(lines[0].chars().count() <= MAX_LINE_CHARS + 4);
}

/// Several keywords list the lines holding every keyword; lines with
/// only some stay counted for the readHits continuation. A file where no
/// line holds them all lists each keyword's first line.
#[test]
fn lines_with_every_keyword_are_listed_first() {
    let keywords = ["octocode".to_owned(), "mcp".to_owned()];
    let text = "# Octocode\nOctocode MCP server\nmcp only\noctocode only\nthe octocode mcp tools\n";
    let FileHits::Lines { lines, total, .. } =
        scan(text.as_bytes(), "README.md", &keywords, "", &Passthrough)
    else {
        panic!("hit");
    };
    assert_eq!(
        lines,
        vec!["2\tOctocode MCP server", "5\tthe octocode mcp tools"]
    );
    assert_eq!(total, 5);
    let spread = "octocode one\noctocode two\nmcp three\nmcp four\n";
    let FileHits::Lines { lines, total, .. } =
        scan(spread.as_bytes(), "a.md", &keywords, "", &Passthrough)
    else {
        panic!("hit");
    };
    // The window anchoring the read (lines 2-3) is listed too.
    assert_eq!(
        lines,
        vec!["1\toctocode one", "2\toctocode two", "3\tmcp three"]
    );
    assert_eq!(total, 4);
}

/// A hit line whose lowercase form moves char boundaries, even at an
/// equal byte length, lists verbatim rather than panicking, and a long
/// line keeps its keyword in the window.
#[test]
fn hit_lines_that_lowercase_to_other_boundaries_list_verbatim() {
    let lines = |text: &str, keyword: &str| match scan(
        text.as_bytes(),
        "a.js",
        &[keyword.into()],
        "",
        &Passthrough,
    ) {
        FileHits::Lines { lines, .. } => lines,
        other => panic!("{other:?}"),
    };
    let text = "const s = \"İéxẞ\";\n";
    assert_eq!(text.len(), text.to_lowercase().len());
    assert_eq!(lines(text, "éx"), ["1\tconst s = \"İéxẞ\";"]);
    assert_eq!(lines(text, "ẞ"), ["1\tconst s = \"İéxẞ\";"]);
    assert_eq!(lines("x = \"STRAẞE\"\n", "straße"), ["1\tx = \"STRAẞE\""]);
    for prefix in ["İ", "ẞ", "e\u{301}", "👍🏽"] {
        let line = format!("{}needle{}", prefix.repeat(300), "b".repeat(300));
        let shown = &lines(&line, "needle")[0];
        assert!(shown.starts_with("1\t…"), "{prefix}: {shown}");
        assert!(shown.contains("needle"), "{prefix}: {shown}");
    }
}

#[test]
fn many_hits_are_capped_but_counted() {
    let text = "needle\n".repeat(30);
    let FileHits::Lines {
        lines,
        total,
        last,
        clipped,
        ..
    } = scan(
        text.as_bytes(),
        "a.rs",
        &["needle".into()],
        "",
        &Passthrough,
    )
    else {
        panic!("hit");
    };
    assert_eq!(lines.len(), MAX_LINES_PER_FILE);
    assert_eq!(total, 30);
    assert_eq!(last, 30);
    assert!(!clipped);
}

/// A long hit line is cut to a window around its keyword; a secret the cut
/// splits (its `ghp_` prefix outside the window) must not show in part:
/// the line is sanitized whole before the window is cut.
#[test]
fn a_cut_long_line_never_shows_part_of_a_secret() {
    let secret = format!("ghp_{}", "Zq7".repeat(12));
    let security = crate::security::ContentSecurity::new();
    for gap in 0..8 {
        let line = format!(
            "{}const t = \"{secret}\";{}needle{}",
            "a".repeat(300),
            " ".repeat(gap),
            "b".repeat(300)
        );
        let FileHits::Lines { lines, .. } =
            scan(line.as_bytes(), "a.js", &["needle".into()], "", &security)
        else {
            panic!("hit");
        };
        let shown = lines.join("\n");
        assert!(shown.contains("needle"), "{shown}");
        assert!(!shown.contains(&secret[8..28]), "gap {gap}: {shown}");
    }
}
