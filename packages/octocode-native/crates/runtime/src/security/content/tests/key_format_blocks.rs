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

/// A body can be double spaced. The blank lines show as blank, and the block
/// goes on past them.
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
        format!("a\n{placeholder}\n{placeholder}\n\n{placeholder}\n\n{placeholder}\n\nb\n")
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
        let out = security.sanitize_leaf(&leaf, None).unwrap_or(leaf);
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
/// text; a line that only holds a long run further along is not.
#[test]
fn a_cut_body_line_is_hidden_and_a_line_quoting_a_long_value_is_not() {
    const BODY: &str = "MIIEpAIBAAKCAQEAfakebodylinefakebodylinefakebodyline00000";
    let security = ContentSecurity::new();
    let cut = format!("const k = \"-----BEGIN RSA PRIVATE KEY-----\n{BODY}…\"\n");
    let shown = security.sanitize_leaf(&cut, None).expect("redacted");
    assert!(!shown.contains(BODY), "{shown}");
    let quoted = format!("-----BEGIN RSA PRIVATE KEY-----\nconst sum = \"{BODY}\";\n");
    let shown = security.sanitize_leaf(&quoted, None).expect("redacted");
    assert!(shown.contains(BODY), "{shown}");
}
