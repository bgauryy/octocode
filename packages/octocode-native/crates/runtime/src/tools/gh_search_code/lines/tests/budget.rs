//! Parallel scans under the shared byte budget: order, worker counts, panics and stops.
use super::*;

/// Scanning a page's files on parallel threads keeps each file's own
/// hits, in page order.
#[test]
fn parallel_scans_keep_page_order() {
    let texts: Vec<String> = (0..17)
        .map(|file| match file % 3 {
            0 => format!("fn f{file}() {{}}\nneedle {file}\n"),
            1 => "nothing here\n".to_owned(),
            _ => format!("{}needle\n", "x\n".repeat(file)),
        })
        .collect();
    let paths: Vec<String> = (0..texts.len()).map(|file| format!("f{file}.rs")).collect();
    let files: Vec<(&[u8], &str)> = texts
        .iter()
        .zip(&paths)
        .map(|(text, path)| (text.as_bytes(), path.as_str()))
        .collect();
    let keywords = ["needle".to_owned()];
    let serial: Vec<FileHits> = files
        .iter()
        .map(|&(bytes, path)| scan(bytes, path, &keywords, "", &Passthrough))
        .collect();
    assert_eq!(
        scan_all(&files, &keywords, "", &Passthrough, &test_context()).unwrap(),
        serial
    );
    assert!(matches!(serial[1], FileHits::Unmatched));
    assert!(matches!(serial[5], FileHits::Lines { first: 6, .. }));
}

/// Fan-outs running at once (rows, or calls of a long-lived server)
/// share one byte budget: the bytes held by running items never pass it,
/// an item larger than the whole budget runs alone, and each fan-out
/// keeps its items' order.
#[test]
fn concurrent_fan_outs_hold_their_bytes_within_one_budget() {
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    let budget = ScanBudget::new(10);
    let (held, peak) = (AtomicUsize::new(0), AtomicUsize::new(0));
    let mut items: Vec<usize> = (0..24).map(|item| 1 + item % 4).collect();
    items.push(25);
    let work = |&weight: &usize| {
        let charged = weight.min(10);
        peak.fetch_max(held.fetch_add(charged, SeqCst) + charged, SeqCst);
        std::thread::sleep(std::time::Duration::from_millis(2));
        held.fetch_sub(charged, SeqCst);
        weight * 10
    };
    let outs = std::thread::scope(|scope| {
        let calls: Vec<_> = (0..3)
            .map(|_| {
                scope.spawn(|| fan_out(&budget, &test_context(), 8, &items, |&weight| weight, work))
            })
            .collect();
        calls
            .into_iter()
            .map(|call| call.join().expect("fan-out").expect("scan"))
            .collect::<Vec<_>>()
    });
    assert!(peak.load(SeqCst) <= 10, "{}", peak.load(SeqCst));
    let expected: Vec<Option<usize>> = items.iter().map(|weight| Some(weight * 10)).collect();
    for out in outs {
        assert_eq!(out, expected);
    }
    assert_eq!(*budget.held.lock().expect("held"), 0);
}

/// Workers follow the cores for small blobs and the budget for large
/// ones: twelve 1.8 MB blobs scan four at a time on twelve cores.
#[test]
fn scan_workers_are_bounded_by_bytes_then_cores() {
    let budget = ScanBudget::new(SCAN_BUDGET_BYTES);
    assert_eq!(budget.workers(12, 12, 1_800_000), 4);
    assert_eq!(budget.workers(12, 12, 20_000), 12);
    assert_eq!(budget.workers(12, 3, 20_000), 3);
    assert_eq!(budget.workers(12, 12, MAX_SCAN_BYTES), 4);
    assert_eq!(budget.workers(1, 12, 0), 1);
    assert_eq!(scan_weight(&vec![b'x'; MAX_SCAN_BYTES + 1]), 0);
}

/// A fan-out given one core runs on one thread, with the serial output.
#[test]
fn a_fan_out_on_one_core_runs_on_one_thread() {
    let threads = std::sync::Mutex::new(std::collections::HashSet::new());
    let out = fan_out(
        &ScanBudget::new(usize::MAX),
        &test_context(),
        1,
        &[1, 2, 3, 4],
        |_| 0,
        |&item| {
            threads
                .lock()
                .expect("threads")
                .insert(std::thread::current().id());
            item
        },
    );
    assert_eq!(out.unwrap(), [Some(1), Some(2), Some(3), Some(4)]);
    assert_eq!(threads.lock().expect("threads").len(), 1);
}

/// A panicking item is the only one lost, on a worker that took others
/// too and on the single-item path alike, and its bytes go back.
#[test]
fn a_panicking_item_loses_only_itself() {
    let budget = ScanBudget::new(usize::MAX);
    let work = |&item: &u32| {
        assert_ne!(item, 2, "item 2 panics");
        item
    };
    let out = fan_out(&budget, &test_context(), 1, &[1, 2, 3, 4], |_| 1, work);
    assert_eq!(out.unwrap(), [Some(1), None, Some(3), Some(4)]);
    assert_eq!(
        fan_out(&budget, &test_context(), 1, &[2], |_| 1, work).unwrap(),
        [None]
    );
    assert_eq!(*budget.held.lock().expect("held"), 0);
}

fn stopped_budget_waiter(timeout: bool, items: &[usize]) {
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    use std::time::{Duration, Instant};

    let budget = ScanBudget::new(1);
    let holder_context = test_context();
    let holder = budget.admit(1, &holder_context).expect("holder admitted");
    let mut context = test_context();
    if timeout {
        context.budget.deadline = Instant::now() + Duration::from_millis(250);
        context.budget.wait_until = context.budget.deadline;
    }
    let ran = AtomicUsize::new(0);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let result = fan_out(
                &budget,
                &context,
                4,
                items,
                |_| 1,
                |_| {
                    ran.fetch_add(1, SeqCst);
                },
            );
            tx.send(result.map_err(|error| error.kind))
                .expect("result receiver");
        });
        let observe_until = Instant::now() + Duration::from_secs(2);
        let waiting = loop {
            if budget.waiting.load(SeqCst) != 0 {
                break true;
            }
            if Instant::now() >= observe_until {
                break false;
            }
            std::thread::yield_now();
        };
        if !timeout {
            context.budget.cancellation.cancel();
        }
        let result = rx.recv_timeout(Duration::from_secs(2));
        let still_held = *budget.held.lock().expect("budget");
        let work_ran = ran.load(SeqCst);
        // Release even on failure so a broken unbounded wait cannot hang the test.
        drop(holder);
        assert!(waiting, "worker must enter admission wait before stopping");
        assert_eq!(
            result.expect("waiter returns before holder release"),
            Err(if timeout {
                ProviderErrorKind::Timeout
            } else {
                ProviderErrorKind::Cancelled
            })
        );
        assert_eq!(still_held, 1);
        assert_eq!(work_ran, 0);
    });
    assert_eq!(*budget.held.lock().expect("budget"), 0);
}

#[test]
fn cancelled_budget_waiter_returns_before_holder_release_without_work() {
    stopped_budget_waiter(false, &[1]);
    stopped_budget_waiter(false, &[1, 2]);
}

#[test]
fn expired_budget_waiter_returns_before_holder_release_without_work() {
    stopped_budget_waiter(true, &[1]);
    stopped_budget_waiter(true, &[1, 2]);
}

#[test]
fn cancellation_between_files_stops_remaining_work() {
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    let budget = ScanBudget::new(1);
    let context = test_context();
    let ran = AtomicUsize::new(0);
    let result = fan_out(
        &budget,
        &context,
        1,
        &[1, 2, 3],
        |_| 1,
        |_| {
            ran.fetch_add(1, SeqCst);
            context.budget.cancellation.cancel();
        },
    );
    assert_eq!(result.unwrap_err().kind, ProviderErrorKind::Cancelled);
    assert_eq!(ran.load(SeqCst), 1);
    assert_eq!(*budget.held.lock().expect("budget"), 0);
}

#[test]
fn deadline_between_files_stops_remaining_work() {
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    use std::time::{Duration, Instant};
    let budget = ScanBudget::new(1);
    let mut context = test_context();
    context.budget.deadline = Instant::now() + Duration::from_millis(100);
    let ran = AtomicUsize::new(0);
    let result = fan_out(
        &budget,
        &context,
        1,
        &[1, 2, 3],
        |_| 1,
        |_| {
            ran.fetch_add(1, SeqCst);
            while Instant::now() < context.budget.deadline {
                std::thread::park_timeout(
                    context
                        .budget
                        .deadline
                        .saturating_duration_since(Instant::now()),
                );
            }
        },
    );
    assert_eq!(result.unwrap_err().kind, ProviderErrorKind::Timeout);
    assert_eq!(ran.load(SeqCst), 1);
    assert_eq!(*budget.held.lock().expect("budget"), 0);
}

#[test]
fn stopped_scan_all_propagates_errors_instead_of_unavailable_hits() {
    let context = test_context();
    context.budget.cancellation.cancel();
    let result = scan_all(
        &[(b"needle", "a.rs")],
        &["needle".into()],
        "",
        &Passthrough,
        &context,
    );
    assert_eq!(result.unwrap_err().kind, ProviderErrorKind::Cancelled);
    let mut context = test_context();
    context.budget.deadline = std::time::Instant::now();
    let result = scan_all(&[], &[], "", &Passthrough, &context);
    assert_eq!(result.unwrap_err().kind, ProviderErrorKind::Timeout);
}
