use super::super::MARKER_PREFIX_BYTES;
use super::{Want, WindowLines};
use crate::security::{KeyBlockState, key_formats, marker_lines};

/// The scan a stream runs on raw bytes, in pieces, decides as a whole-file
/// read does on the decoded line, on every generated line: invalid UTF-8,
/// BOMs, whitespace, every marker and near miss, any marker past the line's
/// head. A line's named PuTTY forms are read at its head only, so one padded
/// past it is out.
#[test]
fn the_streamed_marker_scan_decides_as_the_decoded_line_does() {
    use crate::security::MarkerScan;
    let past_the_head = |line: &[u8]| {
        line.len() > MARKER_PREFIX_BYTES
            && [&b"PuTTY-User"[..], b"Private-Lines", b"Private-MAC"]
                .iter()
                .any(|name| line.windows(name.len()).any(|at| at == *name))
    };
    let (mut marked, mut unmarked) = (0, 0);
    for line in marker_lines::lines() {
        if past_the_head(&line) {
            continue;
        }
        let decoded = String::from_utf8_lossy(&line);
        let mut whole = MarkerScan::default();
        whole.feed(decoded.as_bytes());
        let expected = whole.finish(decoded.as_bytes());
        let mut scan = MarkerScan::default();
        for piece in line.chunks(7) {
            scan.feed(piece);
        }
        let head = &line[..line.len().min(MARKER_PREFIX_BYTES)];
        assert_eq!(
            scan.finish(head),
            expected,
            "{:?}",
            String::from_utf8_lossy(&line)
        );
        if expected.any() {
            marked += 1;
        } else {
            unmarked += 1;
        }
    }
    assert!(marked > 1_000 && unmarked > 1_000, "{marked} {unmarked}");
}

/// A marker line with invalid bytes around it still opens and closes a block
/// in a stream, and every line of the block is replaced.
#[test]
fn invalid_utf8_around_markers_still_replaces_the_block() {
    let source: [&[u8]; 3] = [
        b"\xFF\xEF\xBB\xBF  -----BEGIN RSA PRIVATE KEY-----\xC3\n",
        b"MIIEpQIBAAKCAQEA7Yn8xK2vJ9qLmN3pQrSt\n",
        b"\xE2\x82-----END RSA PRIVATE KEY-----\xFF\n",
    ];
    let mut lines = WindowLines::new(&[Want::lines(1, 4)], KeyBlockState::default());
    for (index, line) in source.iter().enumerate() {
        lines.push(line, index + 1);
        lines.finish(index + 1);
    }
    assert!(!lines.block.is_open());
    lines.push(b"after\n", 4);
    lines.finish(4);
    let held = &lines.held[0];
    assert_eq!(held.key_lines, 3);
    let window = String::from_utf8_lossy(&held.window);
    assert!(!window.contains("MIIE"), "{window}");
    assert!(window.ends_with("after\n"), "{window}");
}

/// What the windows `wants` hold of `text`, the source fed in pieces of
/// `size` bytes as a stream splits lines, each new window from a block state
/// carried across earlier lines.
fn streamed(text: &str, wants: &[Want], size: usize) -> Vec<String> {
    let mut lines = WindowLines::new(wants, KeyBlockState::default());
    for (number, line) in (1..).zip(text.as_bytes().split_inclusive(|byte| *byte == b'\n')) {
        for piece in line.chunks(size) {
            lines.push(piece, number);
        }
        lines.finish(number);
    }
    lines
        .held
        .iter()
        .map(|held| String::from_utf8_lossy(&held.window).into_owned())
        .collect()
}

/// A window starting at any line of a key, or holding the whole source, shows
/// no body line, however the stream splits the lines.
#[test]
fn every_key_format_stays_redacted_in_every_streamed_window() {
    for format in key_formats::formats() {
        let total = format.text.lines().count();
        let wants: Vec<Want> = (1..=total)
            .map(|first| Want::lines(first, total))
            .chain((1..=total).map(|line| Want::lines(line, line)))
            .collect();
        for size in [1, 3, 7, 64 * 1024] {
            for window in streamed(&format.text, &wants, size) {
                for secret in &format.secrets {
                    assert!(
                        !window.contains(secret.as_str()),
                        "{} in pieces of {size}: {window}",
                        format.name
                    );
                }
            }
        }
    }
}

/// The same lines are key material in a stream and in a whole-file read.
#[test]
fn a_stream_finds_the_blocks_a_whole_file_read_finds() {
    for format in key_formats::formats() {
        let (whole, _) = crate::security::redact_private_key_blocks(&format.text);
        let total = format.text.lines().count();
        let [window] = <[String; 1]>::try_from(streamed(&format.text, &[Want::lines(1, total)], 5))
            .expect("one window");
        let placeholder = crate::security::key_fragment_placeholder();
        let replaced = |text: &str| {
            text.lines()
                .map(|line| line == placeholder)
                .collect::<Vec<_>>()
        };
        assert_eq!(replaced(&window), replaced(&whole), "{}", format.name);
    }
}

#[test]
fn a_block_state_carried_to_a_later_line_start_keeps_the_block_open() {
    let format = key_formats::formats()
        .into_iter()
        .find(|format| format.name == "rfc4716 encrypted")
        .expect("format");
    let lines: Vec<&str> = format.text.split_inclusive('\n').collect();
    let at = lines
        .iter()
        .position(|line| line.starts_with(&format.secrets[1]))
        .expect("a body line");
    let mut before = WindowLines::new(&[], KeyBlockState::default());
    for (index, line) in lines[..at].iter().enumerate() {
        before.push(line.as_bytes(), index + 1);
        before.finish(index + 1);
    }
    let mut after = WindowLines::new(&[Want::lines(at + 1, at + 2)], before.block);
    after.push(lines[at].as_bytes(), at + 1);
    after.finish(at + 1);
    after.push(lines[at + 1].as_bytes(), at + 2);
    after.finish(at + 2);
    let window = String::from_utf8_lossy(&after.held[0].window);
    assert!(!window.contains(&format.secrets[1]), "{window}");
    assert!(!window.contains(&format.secrets[2]), "{window}");
}

#[test]
fn a_near_miss_marker_opens_no_block_in_a_stream() {
    let run = "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVowMTIzNDU2Nzg5QUJDREVGR0hJSktM";
    for miss in key_formats::NOT_MARKERS {
        let text = format!("{miss}\n{run}\nafter\n");
        let [window] =
            <[String; 1]>::try_from(streamed(&text, &[Want::lines(2, 3)], 4)).expect("one window");
        assert_eq!(window, format!("{run}\nafter\n"), "{miss:?}");
    }
}
