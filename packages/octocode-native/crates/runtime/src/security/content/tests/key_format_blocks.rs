use super::key_formats::{NOT_MARKERS, formats};
use super::*;

#[test]
fn every_key_format_is_redacted_whole_and_the_lines_around_it_stay() {
    for format in formats() {
        let (out, changed) = redact_private_key_blocks(&format.text);
        assert!(changed, "{}: no block found", format.name);
        for secret in &format.secrets {
            assert!(!out.contains(secret.as_str()), "{}: {out}", format.name);
        }
        assert!(out.contains("tail line"), "{}: {out}", format.name);
        assert_eq!(
            out.lines().count(),
            format.text.lines().count(),
            "{}",
            format.name
        );
    }
}

/// A body line searched for on its own is inside a reported block range.
#[test]
fn every_key_format_reports_each_body_line_inside_a_block_range() {
    for format in formats() {
        let ranges = private_key_block_line_ranges(&format.text);
        for (index, line) in format.text.lines().enumerate() {
            if format
                .secrets
                .iter()
                .any(|secret| line.contains(secret.as_str()))
            {
                let number = u32::try_from(index + 1).expect("small");
                assert!(
                    ranges
                        .iter()
                        .any(|&(start, end)| start <= number && number <= end),
                    "{}: line {number} outside {ranges:?}",
                    format.name
                );
            }
        }
    }
}

#[test]
fn a_marker_near_miss_starts_no_block() {
    let run = "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVowMTIzNDU2Nzg5QUJDREVGR0hJSktM";
    for miss in NOT_MARKERS {
        let file = format!("before\n{miss}\n{run}\n{run}\nafter\n");
        let (out, changed) = redact_private_key_blocks(&file);
        assert!(!changed && out == file, "{miss:?}: {out}");
        assert!(private_key_block_line_ranges(&file).is_empty(), "{miss:?}");
        assert!(!is_private_key_boundary(miss), "{miss:?}");
    }
}

/// An unterminated marker keeps today's handling: its line and the key-shaped
/// run after it are key material, and the first ordinary line closes the block.
#[test]
fn an_rfc4716_marker_with_no_end_ends_at_the_first_ordinary_line() {
    let literal = "let a = 1;\nconst M = \"---- BEGIN SSH2 PRIVATE KEY ----\";\nlet b = compute(a);\nfn main() {}\n";
    assert_eq!(private_key_block_line_ranges(literal), vec![(2, 2)]);
    let (out, _) = redact_private_key_blocks(literal);
    assert!(out.contains("let b = compute(a);"), "{out}");
    assert!(out.contains("fn main() {}"), "{out}");
}

/// A body can be double spaced. The blank lines show as blank, the block
/// goes on past them, and its armor lines read as written.
#[test]
fn a_blank_line_inside_a_body_keeps_the_block_open_but_shows_as_blank() {
    let body = "MIIEpQIBAAKCAQEAdoubleSpacedKeyBodyAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let file = format!(
        "a\n-----BEGIN RSA PRIVATE KEY-----\n{body}\n\n{body}\n\n-----END RSA PRIVATE KEY-----\n\nb\n"
    );
    let (out, _) = redact_private_key_blocks(&file);
    assert!(!out.contains(body), "{out}");
    let placeholder = key_fragment_placeholder();
    assert_eq!(
        out,
        format!(
            "a\n-----BEGIN RSA PRIVATE KEY-----\n{placeholder}\n\n{placeholder}\n\n-----END RSA PRIVATE KEY-----\n\nb\n"
        )
    );
}

/// The leaf net every other tool result passes through: a window cut from its
/// block, holding a boundary and a plain body line.
#[test]
fn a_leaf_holding_one_boundary_of_any_format_redacts_its_body() {
    let security = ContentSecurity::new();
    let mut checked = 0;
    for format in formats() {
        let lines: Vec<&str> = format.text.lines().collect();
        let Some(at) = lines.iter().position(|text| *text == format.secrets[0]) else {
            continue;
        };
        if !lines[..at].iter().any(|text| is_private_key_boundary(text)) {
            continue;
        }
        checked += 1;
        let leaf = lines[..=at].join("\n");
        let out = security.sanitize_leaf(&leaf).unwrap_or(leaf);
        assert!(!out.contains(&format.secrets[0]), "{}: {out}", format.name);
    }
    assert!(checked > 10, "{checked}");
}

/// Armors that say `SECRET KEY` or `Static key` rather than `PRIVATE KEY` open a
/// block the same way; their public and certificate siblings do not.
#[test]
fn secret_and_static_key_armors_open_a_block() {
    let hex = (0..4)
        .map(|at| format!("{at}0a1b2c3d4e5f60718293a4b5c6d7e8f"))
        .collect::<Vec<_>>();
    for (open, close) in [
        (
            "-----BEGIN OpenVPN Static key V1-----",
            "-----END OpenVPN Static key V1-----",
        ),
        (
            "-----BEGIN PGP SECRET KEY BLOCK-----",
            "-----END PGP SECRET KEY BLOCK-----",
        ),
        ("-----BEGIN SECRET KEY-----", "-----END SECRET KEY-----"),
    ] {
        let text = format!("before\n{open}\n{}\n{close}\nafter\n", hex.join("\n"));
        let (out, changed) = redact_private_key_blocks(&text);
        assert!(changed, "{open}");
        assert!(!hex.iter().any(|line| out.contains(line.as_str())), "{out}");
        assert!(
            out.starts_with("before\n") && out.ends_with("after\n"),
            "{out}"
        );
    }
    for (open, close) in [
        ("-----BEGIN PUBLIC KEY-----", "-----END PUBLIC KEY-----"),
        (
            "-----BEGIN PGP PUBLIC KEY BLOCK-----",
            "-----END PGP PUBLIC KEY BLOCK-----",
        ),
        ("-----BEGIN CERTIFICATE-----", "-----END CERTIFICATE-----"),
        (
            "-----BEGIN AGE ENCRYPTED FILE-----",
            "-----END AGE ENCRYPTED FILE-----",
        ),
    ] {
        let text = format!("{open}\n{}\n{close}\n", hex.join("\n"));
        assert_eq!(redact_private_key_blocks(&text).0, text, "{open}");
    }
}

/// Beside a marker that survives, a body line something cut off is still key
/// text, and inside the block a line that only holds a long run further
/// along keeps its label and hides the run, as a whole-file read hides it:
/// one rule for every reader. The armor itself is shown.
#[test]
fn a_cut_body_line_is_hidden_and_a_line_quoting_a_long_value_is_not() {
    const BODY: &str = "MIIEpAIBAAKCAQEAfakebodylinefakebodylinefakebodyline00000";
    let security = ContentSecurity::new();
    let cut = format!("const k = \"-----BEGIN RSA PRIVATE KEY-----\n{BODY}…\"\n");
    let shown = security.sanitize_leaf(&cut).expect("redacted");
    assert!(!shown.contains(BODY), "{shown}");
    let quoted = format!("-----BEGIN RSA PRIVATE KEY-----\nconst sum = \"{BODY}\";\n");
    let shown = security.sanitize_leaf(&quoted).expect("redacted");
    let (whole, _) = super::redact_private_key_blocks(&quoted);
    assert_eq!(shown, whole, "{quoted}");
    assert!(
        shown.starts_with("-----BEGIN RSA PRIVATE KEY-----\nconst sum = \"")
            && !shown.contains(BODY),
        "{shown}"
    );
}

/// What a read of a key file shows: each line of key text holds the fragment
/// placeholder in place of its key bytes, PuTTY's MAC its value; every other
/// line (an armor, an armor header, PuTTY's named lines and its public lines,
/// a blank) reads as written.
fn read_form(format: &super::key_formats::KeyFormat) -> String {
    let placeholder = key_fragment_placeholder();
    format
        .text
        .split_inclusive('\n')
        .map(|line| {
            let body = line.trim_end_matches('\n');
            let shown = if format.secrets.iter().any(|secret| secret == body) {
                placeholder.clone()
            } else if body.starts_with("Private-MAC: ") {
                format!("Private-MAC: {placeholder}")
            } else {
                body.to_owned()
            };
            format!("{shown}{}", &line[body.len()..])
        })
        .collect()
}

/// Retest R4 and R5: a PuTTY key file shows its header lines (`Encryption:`,
/// `Comment:`, `Public-Lines:`, a v3 file's key derivation) and its public
/// lines as written, and hides only its private lines and its MAC's value;
/// a PGP armor shows its `Version:` and `Comment:` headers. Every view of the
/// file (the whole-file key pass, the read's judgment with or without a
/// path, a leaf, clasify's inline content) shows that one form: one
/// placeholder per hidden line, never another pattern's marker beside it.
#[test]
fn every_view_of_a_key_file_shows_the_reads_one_form() {
    let security = ContentSecurity::new();
    let mut checked = 0;
    for format in formats() {
        if !matches!(
            format.name,
            "ppk v2" | "ppk v3" | "pgp armor headers" | "rsa" | "openssh" | "pem armor headers"
        ) {
            continue;
        }
        checked += 1;
        let expected = read_form(&format);
        let name = format.name;
        assert_eq!(
            redact_private_key_blocks(&format.text).0,
            expected,
            "{name}"
        );
        for path in [Some(Path::new("key.txt")), Some(Path::new("id.ppk")), None] {
            let read = security.sanitize_text(&format.text, path);
            assert_eq!(read.content, expected, "{name} {path:?}");
        }
        let judged =
            crate::security::JudgedText::judge(&format.text, Path::new("key.txt"), &security);
        assert_eq!(
            judged.redaction().map(|(clean, _)| clean),
            Some(expected.as_str()),
            "{name}"
        );
        assert_eq!(
            security.sanitize_leaf(&format.text).as_deref(),
            Some(expected.as_str()),
            "{name}"
        );
        let mut row = serde_json::json!({ "content": format.text });
        let row = row.as_object_mut().expect("object");
        security.hide_provider_text(row, "content");
        assert_eq!(row["content"], expected.as_str(), "{name}");
    }
    assert_eq!(checked, 6);
}
