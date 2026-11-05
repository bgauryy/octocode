use super::*;
use crate::execution::cancel::CancellationCheck;
use octocode_contract_types::ErrorCode;

/// Line counting has the walk's time budget: past the deadline no file
/// but the first is counted, and the counts are the prefix before the
/// first uncounted file (the walk is cut there); a directory is never
/// counted.
#[test]
fn line_counts_stop_at_the_deadline_as_a_prefix() {
    let root = tempfile::tempdir().expect("root");
    let file = root.path().join("a.rs");
    std::fs::write(&file, "x\ny\n").expect("file");
    let paths = crate::tools::test_support::workspace_policy(root.path());
    let files = [None, Some(file.as_path()), None, Some(file.as_path())];
    let never = &crate::execution::cancel::NeverCancel;
    let (counted, cut) = count_lines(&files, &paths, never, None).expect("count");
    assert_eq!(
        counted,
        [None, Some(Lines::Counted(2)), None, Some(Lines::Counted(2))]
    );
    assert_eq!(cut, None);
    // The first file is counted whatever the deadline: a cut walk
    // still advances.
    let (late, cut) = count_lines(
        &files,
        &paths,
        never,
        Some(octocode_engine::scan_budget::walk_now()),
    )
    .expect("count");
    assert_eq!(late, [None, Some(Lines::Counted(2)), None]);
    assert_eq!(cut, Some(3));
}

/// Line counting opens files on a bounded pool: at most
/// [`FILE_OPEN_WORKERS`] at once whatever the core count, never more
/// than there are entries, and always one.
#[test]
fn line_count_workers_are_bounded() {
    assert_eq!(line_count_workers(0), 1);
    assert_eq!(line_count_workers(1), 1);
    let many = line_count_workers(10_000);
    assert!((1..=FILE_OPEN_WORKERS).contains(&many), "{many}");
}

#[test]
fn line_count_can_be_cancelled_between_bounded_reads() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Cancel(AtomicUsize);
    impl CancellationCheck for Cancel {
        fn check(&self) -> Result<(), String> {
            if self.0.fetch_add(1, Ordering::SeqCst) >= 3 {
                Err("stop line count".into())
            } else {
                Ok(())
            }
        }
    }
    let root_dir = crate::tools::test_support::Fixture::new();
    let root = root_dir.0.clone();
    std::fs::create_dir_all(&root).expect("fixture");
    let file = root.join("large.rs");
    std::fs::write(&file, "line\n".repeat(20_000)).expect("large source");
    let paths = crate::tools::test_support::workspace_policy(&root);
    let error =
        line_count(&file, &paths, &Cancel(AtomicUsize::new(0))).expect_err("cancel while reading");
    assert_eq!(error.code, ErrorCode::Cancelled);
    std::fs::remove_dir_all(root).expect("cleanup");
}

/// A FIFO counts as unreadable: the open neither blocks on it (no writer)
/// nor reads it.
#[cfg(unix)]
#[test]
fn a_fifo_line_count_is_unreadable_without_blocking() {
    let root = tempfile::tempdir().expect("root");
    let fifo = root.path().join("pipe.rs");
    crate::tools::test_support::make_fifo(&fifo);
    let paths = crate::tools::test_support::workspace_policy(root.path());
    let (sent, received) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sent.send(line_count(
            &fifo,
            &paths,
            &crate::execution::cancel::NeverCancel,
        ));
    });
    let counted = received
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("the line count returned instead of blocking on the FIFO");
    assert!(
        matches!(counted, Ok(Some(Lines::Unreadable))),
        "{counted:?}"
    );
}

/// UTF-16 text has lines, as a read of it shows: counted in code units, with or
/// without a final newline, in either byte order. A NUL code unit stays binary.
#[test]
fn utf16_text_counts_its_lines() {
    let root = tempfile::tempdir().expect("root");
    let paths = crate::tools::test_support::workspace_policy(root.path());
    let never = &crate::execution::cancel::NeverCancel;
    let encode = |bom: [u8; 2], units: &[u16], big: bool| {
        let mut bytes = bom.to_vec();
        for unit in units {
            bytes.extend(if big {
                unit.to_be_bytes()
            } else {
                unit.to_le_bytes()
            });
        }
        bytes
    };
    let text = |s: &str| s.encode_utf16().collect::<Vec<_>>();
    for (name, bytes, expected) in [
        (
            "le.txt",
            encode([0xff, 0xfe], &text("one\ntwo\n"), false),
            Lines::Counted(2),
        ),
        (
            "be.txt",
            encode([0xfe, 0xff], &text("one\ntwo"), true),
            Lines::Counted(2),
        ),
        // U+010A and U+0A00 hold the byte 0x0A and end no line.
        (
            "wide.txt",
            encode([0xff, 0xfe], &text("\u{10a}\u{a00}\n"), false),
            Lines::Counted(1),
        ),
        (
            "nul.txt",
            encode([0xff, 0xfe], &text("a\0b\n"), false),
            Lines::Binary,
        ),
    ] {
        let file = root.path().join(name);
        std::fs::write(&file, bytes).expect("file");
        let (counted, _) =
            count_lines(&[Some(file.as_path())], &paths, never, None).expect("count");
        assert_eq!(counted, [Some(expected)], "{name}");
    }
    // Past one read buffer, where the BOM is long gone and a code unit may split.
    let long = encode([0xff, 0xfe], &text(&"line of text\n".repeat(4000)), false);
    let file = root.path().join("long.txt");
    std::fs::write(&file, long).expect("file");
    let (counted, _) = count_lines(&[Some(file.as_path())], &paths, never, None).expect("count");
    assert_eq!(counted, [Some(Lines::Counted(4000))]);
}
