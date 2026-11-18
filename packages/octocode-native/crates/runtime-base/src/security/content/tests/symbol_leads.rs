//! Key text behind a line's lead: a comment mark, a quote, a bullet, a
//! box-drawing tree, an ellipsis, any run of characters that are no letter,
//! digit or base64 byte. All key bytes are fake.

use super::key_markers::after_line_lead;
use super::*;

const BODY: &str = "WzHvuy3fEWJr5musirUr5JYe5CMpm8DI2mKvqGsxgG5oDTudHZGh0Z7tcHL5kVeq";

/// Leads a key's lines stand behind: the ASCII comment marks, and runs of
/// punctuation, symbols and spaces of other scripts.
const LEADS: [&str; 18] = [
    "# ",
    "// ",
    "> ",
    " * ",
    "• ",
    "│ ",
    "…",
    "▸ ",
    "★ ",
    "├── ",
    "│   └── ",
    "\u{1F511} ",
    "% ",
    "! ",
    "· ",
    "\u{3000}",
    "\u{00A0}\u{00A0}",
    "— ",
];

/// A key whose every line stands behind `lead`, between two lines of prose.
fn key_behind(lead: &str) -> String {
    format!(
        "intro\n{lead}-----BEGIN RSA PRIVATE KEY-----\n{lead}{BODY}\n{lead}{BODY}\n{lead}QUJD==\n{lead}-----END RSA PRIVATE KEY-----\ntail\n"
    )
}

/// A key behind any lead reads as a `# `-commented one does: its armor lines
/// as written, each line of key text hidden from its indentation, in the
/// whole-file pass, a read of a file, a leaf and the range a search reports.
#[test]
fn a_key_behind_any_lead_is_hidden_as_a_commented_one_is() {
    let placeholder = key_fragment_placeholder();
    let security = ContentSecurity::new();
    for lead in LEADS {
        let text = key_behind(lead);
        let indent: String = lead.chars().take_while(|ch| *ch == ' ').collect();
        let hidden = format!("{indent}{placeholder}");
        let expected = format!(
            "intro\n{lead}-----BEGIN RSA PRIVATE KEY-----\n{hidden}\n{hidden}\n{hidden}\n{lead}-----END RSA PRIVATE KEY-----\ntail\n"
        );
        assert_eq!(redact_private_key_blocks(&text).0, expected, "{lead:?}");
        let read = security.sanitize_text(&text, Some(Path::new("notes.md")));
        assert_eq!(read.content, expected, "{lead:?}");
        assert!(read.has_secrets, "{lead:?}");
        assert_eq!(
            security.sanitize_leaf(&text).as_deref(),
            Some(expected.as_str()),
            "{lead:?}"
        );
        assert_eq!(
            private_key_block_line_ranges(&text),
            vec![(2, 6)],
            "{lead:?}"
        );
        // A body line cut from its block is still key text, and a snippet
        // of one asks for the whole-file scan.
        let cut = format!("{lead}{BODY}\n{lead}-----END RSA PRIVATE KEY-----");
        let shown = security.sanitize_leaf(&cut).unwrap_or(cut);
        assert!(!shown.contains(BODY), "{lead:?}: {shown}");
        assert!(
            snippet_may_hold_key_material(&format!("{lead}{BODY}")),
            "{lead:?}"
        );
    }
}

/// Beside a key block, bulleted prose, a box-drawing tree of file names and
/// a bullet that names an armor read as written: only key text is hidden.
#[test]
fn bulleted_prose_and_a_file_tree_beside_a_key_read_as_written() {
    let placeholder = key_fragment_placeholder();
    let prose = "Notes\n• This release improves the parser and keeps every option readable.\n• A key opens with -----BEGIN RSA PRIVATE KEY----- on a line of its own.\n• The body follows on lines of its own, wrapped at 64 characters.\n★ Highlight: nothing secret here at all\n";
    let tree =
        "src\n├── main.rs\n├── lib.rs\n│   ├── security.rs\n│   └── key_blocks.rs\n└── README.md\n";
    let key = format!(
        "• -----BEGIN RSA PRIVATE KEY-----\n• {BODY}\n• {BODY}\n• -----END RSA PRIVATE KEY-----\n"
    );
    let text = format!("{prose}{tree}{key}{prose}{tree}");
    let expected = format!(
        "{prose}{tree}• -----BEGIN RSA PRIVATE KEY-----\n{placeholder}\n{placeholder}\n• -----END RSA PRIVATE KEY-----\n{prose}{tree}"
    );
    assert_eq!(redact_private_key_blocks(&text).0, expected);
    let security = ContentSecurity::new();
    assert_eq!(
        security
            .sanitize_text(&text, Some(Path::new("NOTES.md")))
            .content,
        expected
    );
    assert_eq!(
        security.sanitize_leaf(&text).as_deref(),
        Some(expected.as_str())
    );
    // With no key in it, the same text is no redaction at all.
    let plain = format!("{prose}{tree}{prose}");
    assert_eq!(redact_private_key_blocks(&plain).0, plain);
    assert_eq!(security.sanitize_leaf(&plain), None);
    assert!(
        !security
            .sanitize_text(&plain, Some(Path::new("NOTES.md")))
            .has_secrets
    );
}

/// [`LineShapeScan`] fed one byte at a time.
fn shape_by_bytes(line: &str) -> super::super::key_blocks::BlockLine {
    let mut scan = LineShapeScan::default();
    for byte in line.as_bytes() {
        scan.feed(std::slice::from_ref(byte));
    }
    scan.finish(line.as_bytes())
}

/// [`LineShapeScan`] fed the whole line.
fn shape_whole(line: &str) -> super::super::key_blocks::BlockLine {
    let mut scan = LineShapeScan::default();
    scan.feed(line.as_bytes());
    scan.finish(line.as_bytes())
}

/// Over every character: one that is no letter or digit of any script and no
/// base64 byte is a lead, and a lead stops at the key's first byte, so it
/// never holds key text; a letter or digit of any script is no lead (a word
/// before a key's run is its label). A line split anywhere, inside a
/// character too, reads as the whole line does.
#[test]
fn a_lead_never_holds_key_text_and_a_letter_of_any_script_is_none() {
    let mut checked = 0_u32;
    for ch in (0..=u32::from(char::MAX)).filter_map(char::from_u32) {
        if matches!(ch, '\n' | '\r') {
            continue;
        }
        let line = format!("{ch}{BODY}");
        let rest = after_line_lead(line.as_bytes());
        assert!(rest.ends_with(BODY.as_bytes()), "{ch:?} took key bytes");
        let base64 = ch.is_ascii_alphanumeric() || matches!(ch, '+' | '/' | '=');
        if ch.is_alphanumeric() || base64 || ch == '\\' {
            assert_eq!(rest, line.as_bytes(), "{ch:?} read as a lead");
        } else {
            assert_eq!(rest, BODY.as_bytes(), "{ch:?} is no lead");
        }
        if !ch.is_ascii() {
            assert_eq!(shape_by_bytes(&line), shape_whole(&line), "{ch:?}");
            checked += 1;
        }
    }
    assert!(checked > 1_000_000, "{checked}");
    for lead in LEADS {
        let line = format!("{lead}{BODY}");
        assert_eq!(
            after_line_lead(line.as_bytes()),
            BODY.as_bytes(),
            "{lead:?}"
        );
        assert_eq!(shape_by_bytes(&line), shape_whole(&line), "{lead:?}");
    }
}

/// A word in any script before a key's run is a label a block keeps: only
/// the run is hidden, as `logo: <run>` keeps `logo: `.
#[test]
fn a_word_in_any_script_before_a_run_is_kept() {
    let placeholder = key_fragment_placeholder();
    for label in ["ключ ", "鍵 ", "clé: ", "ⓐ ", "² "] {
        let line = format!("{label}{BODY}");
        assert_eq!(
            hide_key_line(&line),
            format!("{label}{placeholder}"),
            "{label:?}"
        );
    }
}
