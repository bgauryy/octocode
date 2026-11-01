//! Complete hit lines: numbering, Unicode, selection and sanitizing.
use super::*;

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
            declaration: None,
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

/// Multiple keywords retain every matching source line, including partial keyword matches.
#[test]
fn all_keyword_lines_are_listed_in_source_order() {
    let keywords = ["octocode".to_owned(), "mcp".to_owned()];
    let text = "# Octocode\nOctocode MCP server\nmcp only\noctocode only\nthe octocode mcp tools\n";
    let FileHits::Lines { lines, total, .. } =
        scan(text.as_bytes(), "README.md", &keywords, "", &Passthrough)
    else {
        panic!("hit");
    };
    assert_eq!(
        lines,
        text.lines()
            .enumerate()
            .map(|(i, line)| format!("{}\t{line}", i + 1))
            .collect::<Vec<_>>()
    );
    assert_eq!(total, 5);
    let spread = "octocode one\noctocode two\nmcp three\nmcp four\n";
    let FileHits::Lines { lines, total, .. } =
        scan(spread.as_bytes(), "a.md", &keywords, "", &Passthrough)
    else {
        panic!("hit");
    };
    // Anchoring the optional read does not change which hit lines are returned.
    assert_eq!(
        lines,
        spread
            .lines()
            .enumerate()
            .map(|(i, line)| format!("{}\t{line}", i + 1))
            .collect::<Vec<_>>()
    );
    assert_eq!(total, 4);
}

/// A hit line whose lowercase form moves char boundaries, even at an
/// equal byte length, lists verbatim rather than panicking.
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
        assert_eq!(shown, &format!("1\t{line}"), "{prefix}");
        assert!(shown.contains("needle"), "{prefix}: {shown}");
    }
}

#[test]
fn many_hits_are_all_retained() {
    let text = "needle\n".repeat(30);
    let FileHits::Lines {
        lines, total, last, ..
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
    assert_eq!(lines.len(), total);
    assert_eq!(total, 30);
    assert_eq!(last, 30);
}

/// A long hit line is sanitized whole, including a secret far from the keyword.
#[test]
fn a_complete_long_line_never_shows_a_secret() {
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

#[test]
fn complete_long_unicode_hit_values_reach_response_pagination() {
    let line = format!("{}needle{} END", "λ🦀 ".repeat(1000), " x".repeat(300));
    let FileHits::Lines { lines, .. } = scan(
        line.as_bytes(),
        "a.js",
        &["needle".into()],
        "",
        &Passthrough,
    ) else {
        panic!("hit")
    };
    assert_eq!(lines, vec![format!("1\t{line}")]);
}

#[test]
fn every_keyword_hit_reaches_response_pagination_once() {
    let text = "alpha\nalpha beta\nbeta\n".repeat(9);
    let FileHits::Lines { lines, total, .. } = scan(
        text.as_bytes(),
        "a.txt",
        &["alpha".into(), "beta".into()],
        "",
        &Passthrough,
    ) else {
        panic!("hits")
    };
    let expected = text
        .lines()
        .enumerate()
        .map(|(i, line)| format!("{}\t{line}", i + 1))
        .collect::<Vec<_>>();
    assert_eq!(total, expected.len());
    assert_eq!(lines, expected);
}
