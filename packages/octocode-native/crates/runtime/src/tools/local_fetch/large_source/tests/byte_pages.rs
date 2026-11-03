use super::*;

/// Independent selected-source oracle: whole source lines and security classification,
/// with no streaming/window implementation in its construction.
fn assert_source_window(source: &str, want: Want, actual: &Streamed) {
    let mut keys = crate::security::KeyBlockState::default();
    let lines = source
        .split_inclusive('\n')
        .map(|line| {
            let key = keys.push_line(line.trim_end_matches(['\r', '\n']));
            let text = if key {
                format!("{}\n", crate::security::key_fragment_placeholder())
            } else {
                line.to_owned()
            };
            (text, key)
        })
        .collect::<Vec<_>>();
    let selected = lines
        .iter()
        .skip(want.first - 1)
        .take(want.last - want.first + 1)
        .map(|(line, _)| line.as_str())
        .collect::<String>();
    let mut base = want.from.min(selected.len());
    while base < selected.len() && !selected.is_char_boundary(base) {
        base += 1;
    }
    let mut end = (base + WINDOW_BYTES).min(selected.len());
    if end < selected.len() {
        if let Some(newline) = selected.as_bytes()[base..end]
            .iter()
            .rposition(|byte| *byte == b'\n')
        {
            end = base + newline + 1;
        } else {
            while !selected.is_char_boundary(end) {
                end -= 1;
            }
        }
    }
    let expected = &selected.as_bytes()[base..end];
    assert!(
        actual.window == expected,
        "exact selected bytes at {}: actual {} bytes, expected {} bytes, first difference {:?}",
        want.from,
        actual.window.len(),
        expected.len(),
        actual
            .window
            .iter()
            .zip(expected)
            .position(|(actual, expected)| actual != expected)
    );
    assert_eq!(actual.origin.total, selected.len());
    assert_eq!(
        actual.origin.base,
        if expected.is_empty() { 0 } else { base }
    );
    let first = want.first
        + selected.as_bytes()[..base]
            .iter()
            .filter(|&&b| b == b'\n')
            .count();
    let last = if expected.is_empty() {
        want.last.min(lines.len())
    } else {
        first + expected.iter().filter(|&&b| b == b'\n').count()
            - usize::from(expected.ends_with(b"\n"))
    };
    assert_eq!(
        actual.first,
        if expected.is_empty() {
            want.first
        } else {
            first
        }
    );
    assert_eq!(actual.last, last);
    assert_eq!(
        actual.key_lines,
        lines
            .iter()
            .skip(want.first - 1)
            .take(last.saturating_sub(want.first - 1))
            .filter(|(_, key)| *key)
            .count()
    );
    assert_eq!(actual.total_lines, lines.len());
    assert_eq!(
        actual.digest,
        octocode_engine::digest::sha256(source.as_bytes())
    );
}

/// Every byte page proves the whole source and selects exactly its requested bytes.
#[test]
fn a_late_byte_page_costs_what_an_early_one_does() {
    let line: Vec<u8> = (0..6 * WINDOW_BYTES)
        .map(|at| b'a' + u8::try_from(at % 26).expect("letter"))
        .collect();
    let text = String::from_utf8(line.clone()).expect("ASCII");
    let file = temp(&line);
    let source = opened(file.path());
    let page = |from: usize| Want::bytes(1, 1, from);
    let whole = Blocks(Default::default());
    stream(&source, &[page(0)], &whole).expect("source pass");
    let whole = whole.0.into_inner();
    let mut costs = Vec::new();
    for from in [0, WINDOW_BYTES, 5 * WINDOW_BYTES - 7] {
        let blocks = Blocks(Default::default());
        let paged = stream(&source, &[page(from)], &blocks)
            .expect("page")
            .pop()
            .expect("window");
        assert_source_window(&text, page(from), &paged);
        costs.push(blocks.0.into_inner());
    }
    assert!(
        costs.iter().all(|&cost| cost == whole),
        "each page proves the source once: {costs:?}, whole {whole}"
    );
}

/// Byte pages of a many-line range no key block reaches match a whole
/// pass wherever they start: mid-line, on a line start, or past the
/// range's last byte.
#[test]
fn byte_pages_over_many_lines_match_a_whole_pass() {
    let text: String = (0..40)
        .map(|line| format!("{}\n", "q".repeat(150_000 + line * 7_919)))
        .collect();
    let file = temp(text.as_bytes());
    let source = opened(file.path());
    let page = |from: usize| Want::bytes(3, 38, from);
    stream(&source, &[page(0)], &NeverCancel).expect("source pass");
    let line_start = text
        .split_inclusive('\n')
        .take(9)
        .map(str::len)
        .sum::<usize>();
    for from in [0, 1, 999_999, line_start, 2 * WINDOW_BYTES + 3, text.len()] {
        let paged = stream(&source, &[page(from)], &NeverCancel)
            .expect("page")
            .pop()
            .expect("window");
        assert_source_window(&text, page(from), &paged);
    }
}

/// A range a private-key block reaches pages exactly as a whole pass
/// does: the key lines stay replaced and counted, and the view keeps
/// the placeholder's size.
#[test]
fn byte_pages_over_a_key_block_match_a_whole_pass() {
    let mut text = "x".repeat(3 * WINDOW_BYTES);
    text.push('\n');
    text.push_str(
        "-----BEGIN RSA PRIVATE KEY-----\nSECRETSECRETSECRET\n-----END RSA PRIVATE KEY-----\n",
    );
    text.push_str(&"y".repeat(2 * WINDOW_BYTES));
    text.push('\n');
    let file = temp(text.as_bytes());
    let source = opened(file.path());
    let page = |from: usize| Want::bytes(1, 5, from);
    stream(&source, &[page(0)], &NeverCancel).expect("source pass");
    for from in [0, 2 * WINDOW_BYTES, 3 * WINDOW_BYTES, 4 * WINDOW_BYTES + 50] {
        let paged = stream(&source, &[page(from)], &NeverCancel)
            .expect("page")
            .pop()
            .expect("window");
        assert_source_window(&text, page(from), &paged);
        assert!(!String::from_utf8_lossy(&paged.window).contains("SECRET"));
    }
}

/// View points that fall inside a key block (a giant body line, then a
/// line of another shape that ends the block), CRLF line ends and an
/// unterminated last line page exactly as a whole pass does, up to and
/// past the range's end.
#[test]
fn byte_pages_with_points_inside_a_key_block_match_a_whole_pass() {
    let mut text = "x".repeat(2 * BYTE_LOOKBEHIND + 17);
    text.push_str("\r\n-----BEGIN PRIVATE KEY-----\r\n");
    text.push_str(&"A".repeat(2 * BYTE_LOOKBEHIND + 5));
    text.push_str("\r\n!!");
    text.push_str(&"B".repeat(BYTE_LOOKBEHIND + 3));
    text.push_str("\r\n-----END PRIVATE KEY-----\r\n\r\n");
    text.push_str(&"€".repeat(BYTE_LOOKBEHIND));
    let file = temp(text.as_bytes());
    let source = opened(file.path());
    let page = |from: usize| Want::bytes(1, 7, from);
    let first = stream(&source, &[page(0)], &NeverCancel)
        .expect("source pass")
        .pop()
        .expect("window");
    let total = first.origin.total;
    for from in [
        1,
        BYTE_LOOKBEHIND + 1,
        2 * BYTE_LOOKBEHIND + 18,
        total / 2,
        total - WINDOW_BYTES / 2,
        total - 1,
        total,
        total + 9,
    ] {
        let paged = stream(&source, &[page(from)], &NeverCancel)
            .expect("page")
            .pop()
            .expect("window");
        assert_source_window(&text, page(from), &paged);
        // The base64 body stays replaced; the `!!` line is not base64,
        // so it ends the block (`LineShape::Other`) and shows as text.
        assert!(
            !String::from_utf8_lossy(&paged.window).contains("AAAA"),
            "from {from}"
        );
    }
}

/// A byte page whose start lies past the view a key block shrank to holds
/// no byte, so it sits at no offset of the view.
#[test]
fn an_empty_window_after_a_key_block_shrank_the_view_has_no_base() {
    let mut text = String::from("-----BEGIN PRIVATE KEY-----\n");
    text.push_str(&"A".repeat(5_000));
    text.push_str("\n-----END PRIVATE KEY-----\n");
    let file = temp(text.as_bytes());
    let want = Want::bytes(1, 3, 2_000);
    let streamed = stream(&opened(file.path()), &[want], &NeverCancel)
        .expect("stream")
        .pop()
        .expect("window");
    assert_source_window(&text, want, &streamed);
    assert!(streamed.window.is_empty());
    assert!(streamed.origin.total < 2_000);
    assert_eq!(streamed.origin.base, 0);
}

/// A key line whose placeholder no longer fits behind the lines held ends
/// the window before it, uncounted: the next page reads it.
#[test]
fn a_key_line_whose_placeholder_does_not_fit_ends_the_window_before_it() {
    let mut text = "x".repeat(WINDOW_BYTES - 10);
    text.push_str("\n-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----\ntail\n");
    let file = temp(text.as_bytes());
    let want = Want::lines(1, 5);
    let streamed = stream(&opened(file.path()), &[want], &NeverCancel)
        .expect("stream")
        .pop()
        .expect("window");
    assert_source_window(&text, want, &streamed);
    assert_eq!(streamed.last, 1);
    assert_eq!(streamed.key_lines, 0);
}
