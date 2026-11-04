use super::key_markers::{MarkerScan, Markers, line_markers};
use super::*;

fn generated() -> Vec<Vec<u8>> {
    marker_lines::lines()
}

/// The rule written out plainly, for every place a run of dashes can open a
/// marker: what the scan, with its skips and its pieces, must decide.
fn plain_markers(line: &[u8]) -> Markers {
    let phrase = regex::bytes::Regex::new("PRIVATE[ \\t]+KEY").expect("static");
    let mut found = Markers::default();
    for at in (0..line.len()).filter(|&at| line[at..].starts_with(b"----")) {
        let rest = &line[at + 4..];
        let blanks = rest
            .iter()
            .take_while(|b| matches!(**b, b' ' | b'\t'))
            .count();
        let rest = &rest[blanks..];
        for (word, begins) in [(&b"BEGIN"[..], true), (&b"END"[..], false)] {
            let Some(label) = rest.strip_prefix(word) else {
                continue;
            };
            let close = label.windows(4).position(|run| run == b"----");
            let held = phrase.find(&label[..close.unwrap_or(label.len())]);
            let marked = held.is_some_and(|key| {
                close.is_some()
                    || label[key.end()..]
                        .iter()
                        .all(|b| matches!(*b, b' ' | b'\t' | b'\r' | b'\n' | b'-'))
            });
            if marked && begins {
                found.begins = true;
                found.opens = true;
            } else if marked {
                found.ends = true;
                found.opens = false;
            }
        }
    }
    found
}

/// The named PuTTY lines are judged at a line's start and proved on their own
/// below; these lines hold no such name there.
fn names_a_putty_line(line: &[u8]) -> bool {
    [&b"PuTTY-User"[..], b"Private-Lines", b"Private-MAC"]
        .iter()
        .any(|name| line.windows(name.len()).any(|at| at == *name))
}

/// The scan decides as the plain rule does, on every generated line, fed whole
/// or in pieces of any size: a marker split across pieces is still a marker.
#[test]
fn the_marker_scan_decides_as_the_plain_rule_does_in_any_pieces() {
    let (mut marked, mut unmarked) = (0, 0);
    for line in generated().iter().filter(|line| !names_a_putty_line(line)) {
        let expected = plain_markers(line);
        assert_eq!(
            line_markers(line),
            expected,
            "{:?}",
            String::from_utf8_lossy(line)
        );
        for size in [1, 2, 3, 5] {
            let mut scan = MarkerScan::default();
            for piece in line.chunks(size) {
                scan.feed(piece);
            }
            assert_eq!(
                scan.finish(line),
                expected,
                "{:?} by {size}",
                String::from_utf8_lossy(line)
            );
        }
        if expected.any() {
            marked += 1;
        } else {
            unmarked += 1;
        }
    }
    assert!(marked > 1_000 && unmarked > 1_000, "{marked} {unmarked}");
}

/// The lines above must be able to catch a scan that looks for too much: each
/// of these strings is absent from a line that holds a marker.
#[test]
fn the_generated_lines_catch_a_scan_that_is_too_narrow() {
    let lines = generated();
    for wrong in [
        "-----",
        "------",
        "---- ",
        "----B",
        "----E",
        "BEGIN ",
        "END ",
        "PRIVATE KEY",
    ] {
        assert!(
            lines.iter().any(|line| {
                !line.windows(wrong.len()).any(|at| at == wrong.as_bytes())
                    && !names_a_putty_line(line)
                    && line_markers(line).any()
            }),
            "no generated line tells {wrong:?} from the rule"
        );
    }
}

/// What the guard recognizes, stated outright, so the equality above cannot
/// hold by both sides recognizing nothing.
#[test]
fn every_private_key_armor_is_a_boundary_and_near_misses_are_not() {
    for marker in [
        "-----BEGIN PRIVATE KEY-----",
        "-----END PRIVATE KEY-----",
        "-----BEGIN RSA PRIVATE KEY-----",
        "-----BEGIN DSA PRIVATE KEY-----",
        "-----BEGIN EC PRIVATE KEY-----",
        "-----BEGIN OPENSSH PRIVATE KEY-----",
        "-----BEGIN ENCRYPTED PRIVATE KEY-----",
        "-----BEGIN SSH2 ENCRYPTED PRIVATE KEY-----",
        "-----BEGIN PGP PRIVATE KEY BLOCK-----",
        "-----END PGP PRIVATE KEY BLOCK-----",
        "---- BEGIN SSH2 ENCRYPTED PRIVATE KEY ----",
        "---- END SSH2 ENCRYPTED PRIVATE KEY ----",
        "---- BEGIN SSH2 PRIVATE KEY ----",
        "----BEGIN RSA PRIVATE KEY----",
        "------BEGIN RSA PRIVATE KEY------",
        "---- BEGIN\tRSA\tPRIVATE\tKEY ----",
        "-----BEGIN RSA PRIVATE KEY",
        "-----END RSA PRIVATE KEY  ",
        "PuTTY-User-Key-File-2: ssh-rsa",
        "PuTTY-User-Key-File-3: ssh-ed25519",
        "Private-Lines: 14",
        "Private-MAC: 0123abcd",
        "> Private-Lines: 14",
        "\"Private-MAC: 0123abcd\"",
        "-----BEGINRSA PRIVATE KEY-----",
        "-----BEGIN\tRSA PRIVATE KEY-----",
        "\u{feff}-----BEGIN RSA PRIVATE KEY-----",
        "\u{fffd}-----BEGIN RSA PRIVATE KEY-----\u{fffd}",
        "   -----END RSA PRIVATE KEY-----",
        "const key = `-----BEGIN RSA PRIVATE KEY-----`;",
    ] {
        assert!(is_private_key_boundary(marker), "{marker:?}");
    }
    for miss in [
        "",
        "PRIVATE KEY",
        "-----",
        "-----BEGIN CERTIFICATE-----",
        "-----BEGIN PUBLIC KEY-----",
        "-----BEGIN RSA PRIVATE KEY\"",
        "Private-Line: 14",
        "the Private-Lines: field",
        "-----begin rsa private key-----",
        "---BEGIN RSA PRIVATE KEY---",
        "---- BEGIN SSH2 PUBLIC KEY ----",
    ] {
        assert!(!is_private_key_boundary(miss), "{miss:?}");
    }
}
