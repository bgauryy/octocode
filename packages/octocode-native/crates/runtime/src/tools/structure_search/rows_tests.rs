use super::*;
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
    assert!(matches!(counted, Ok(Lines::Unreadable)), "{counted:?}");
}
