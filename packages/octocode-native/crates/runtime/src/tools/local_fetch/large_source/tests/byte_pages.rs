use super::*;

/// The reference for one byte page: a whole pass, no index.
fn whole_pass(source: &std::fs::File, want: Want) -> Streamed {
    stream(source, &[want], &NeverCancel)
        .expect("stream")
        .0
        .pop()
        .expect("window")
}

fn assert_same_window(a: &Streamed, b: &Streamed) {
    assert_eq!(a.window, b.window);
    assert_eq!(
        (a.first, a.last, a.key_lines, a.origin.base, a.origin.total),
        (b.first, b.last, b.key_lines, b.origin.base, b.origin.total)
    );
    assert_eq!((&a.digest, a.total_lines), (&b.digest, b.total_lines));
}

/// Byte pages of one giant line, once the source is indexed, read about
/// one window each wherever their offset sits, and hold what a whole
/// pass holds: the window, its place in the line and the line's size.
#[test]
fn a_late_byte_page_costs_what_an_early_one_does() {
    let line: Vec<u8> = (0..6 * WINDOW_BYTES)
        .map(|at| b'a' + u8::try_from(at % 26).expect("letter"))
        .collect();
    let file = temp(&line);
    let source = opened(file.path());
    let stamp = crate::tools::source::cache_stamp(&source.metadata().expect("meta"));
    let path = file.path().canonicalize().expect("canonical");
    let page = |from: usize| Want::bytes(1, 1, from);
    let whole = Blocks(Default::default());
    read_windows(&source, &path, stamp, &[page(0)], &whole).expect("index");
    let whole = whole.0.into_inner();
    let mut costs = Vec::new();
    for from in [0, WINDOW_BYTES, 5 * WINDOW_BYTES - 7] {
        let blocks = Blocks(Default::default());
        let paged = read_windows(&source, &path, stamp, &[page(from)], &blocks)
            .expect("page")
            .pop()
            .expect("window");
        assert_same_window(&paged, &whole_pass(&source, page(from)));
        costs.push(blocks.0.into_inner());
    }
    // A page reads its window and at most one view-point gap before it
    // (64 KiB copy blocks, one partial block each), wherever it starts.
    let bound = (WINDOW_BYTES + VIEW_POINT_BYTES) / (64 * 1024) + 2;
    assert!(
        costs.iter().all(|&cost| cost <= bound) && bound * 3 < whole,
        "pages {costs:?}, bound {bound}, whole pass {whole} blocks"
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
    let stamp = crate::tools::source::cache_stamp(&source.metadata().expect("meta"));
    let path = file.path().canonicalize().expect("canonical");
    let page = |from: usize| Want::bytes(3, 38, from);
    read_windows(&source, &path, stamp, &[page(0)], &NeverCancel).expect("index");
    let line_start = text
        .split_inclusive('\n')
        .take(9)
        .map(str::len)
        .sum::<usize>();
    for from in [0, 1, 999_999, line_start, 2 * WINDOW_BYTES + 3, text.len()] {
        let paged = read_windows(&source, &path, stamp, &[page(from)], &NeverCancel)
            .expect("page")
            .pop()
            .expect("window");
        assert_same_window(&paged, &whole_pass(&source, page(from)));
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
    let stamp = crate::tools::source::cache_stamp(&source.metadata().expect("meta"));
    let path = file.path().canonicalize().expect("canonical");
    let page = |from: usize| Want::bytes(1, 5, from);
    read_windows(&source, &path, stamp, &[page(0)], &NeverCancel).expect("index");
    for from in [0, 2 * WINDOW_BYTES, 3 * WINDOW_BYTES, 4 * WINDOW_BYTES + 50] {
        let paged = read_windows(&source, &path, stamp, &[page(from)], &NeverCancel)
            .expect("page")
            .pop()
            .expect("window");
        assert_same_window(&paged, &whole_pass(&source, page(from)));
        assert!(!String::from_utf8_lossy(&paged.window).contains("SECRET"));
    }
}

/// View points that fall inside a key block (a giant body line, then a
/// line of another shape that ends the block), CRLF line ends and an
/// unterminated last line page exactly as a whole pass does, up to and
/// past the range's end.
#[test]
fn byte_pages_with_points_inside_a_key_block_match_a_whole_pass() {
    let mut text = "x".repeat(2 * VIEW_POINT_BYTES + 17);
    text.push_str("\r\n-----BEGIN PRIVATE KEY-----\r\n");
    text.push_str(&"A".repeat(2 * VIEW_POINT_BYTES + 5));
    text.push_str("\r\n!!");
    text.push_str(&"B".repeat(VIEW_POINT_BYTES + 3));
    text.push_str("\r\n-----END PRIVATE KEY-----\r\n\r\n");
    text.push_str(&"€".repeat(VIEW_POINT_BYTES));
    let file = temp(text.as_bytes());
    let source = opened(file.path());
    let stamp = crate::tools::source::cache_stamp(&source.metadata().expect("meta"));
    let path = file.path().canonicalize().expect("canonical");
    let page = |from: usize| Want::bytes(1, 7, from);
    let first = read_windows(&source, &path, stamp, &[page(0)], &NeverCancel)
        .expect("index")
        .pop()
        .expect("window");
    let total = first.origin.total;
    for from in [
        1,
        VIEW_POINT_BYTES + 1,
        2 * VIEW_POINT_BYTES + 18,
        total / 2,
        total - WINDOW_BYTES / 2,
        total - 1,
        total,
        total + 9,
    ] {
        let paged = read_windows(&source, &path, stamp, &[page(from)], &NeverCancel)
            .expect("page")
            .pop()
            .expect("window");
        let whole = whole_pass(&source, page(from));
        assert_eq!(paged.window, whole.window, "from {from}");
        assert_eq!(
            (paged.first, paged.last, paged.key_lines, paged.origin.total),
            (whole.first, whole.last, whole.key_lines, whole.origin.total),
            "from {from}"
        );
        assert_eq!(paged.origin.base, whole.origin.base, "from {from}");
        assert_eq!(
            (&paged.digest, paged.total_lines),
            (&whole.digest, whole.total_lines)
        );
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
    let streamed = whole_pass(&opened(file.path()), Want::bytes(1, 3, 2_000));
    assert!(streamed.window.is_empty());
    assert!(streamed.origin.total < 2_000);
    assert_eq!(streamed.origin.base, 0);
}
