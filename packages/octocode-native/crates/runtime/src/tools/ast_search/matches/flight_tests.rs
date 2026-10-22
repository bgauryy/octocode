use super::*;
use crate::tools::test_support::{Fixture, simple_policy};
use octocode_contract_types::ErrorCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

struct Flag(AtomicBool);

impl CancellationCheck for Flag {
    fn check(&self) -> Result<(), String> {
        if self.0.load(Ordering::SeqCst) {
            Err("cancelled".into())
        } else {
            Ok(())
        }
    }
}

/// How long a test's held turn waits for the row it holds up; only a row
/// that never answers reaches it, so the test fails instead of hanging.
const HELD_TURN_BOUND: Duration = Duration::from_secs(60);

/// A row as the tool reads it.
fn typed(row: &serde_json::Value) -> super::super::AstSearchQuery {
    serde_json::from_value(row.clone()).expect("typed row")
}

/// Identical match rows running at once (one batch) share one scan: the
/// directory is walked, read and parsed once, and both rows answer alike.
/// The test keeps the rows' flight open (its own ticket) until both have
/// run, so they share it however the threads are scheduled.
#[test]
fn identical_rows_running_at_once_share_one_scan() {
    let root = Fixture::new();
    for index in 0..60 {
        root.write(&format!("f{index:02}.rs"), "fn f() { x.unwrap(); }\n");
    }
    let (paths, _) = simple_policy(&root.0);
    let query = json!({"operation":"match","path":root.0,"pattern":"$X.unwrap()","resultView":"countMatches"});
    let row = typed(&query);
    let super::super::AstSearchQuery::MatchPattern(pattern) = &row else {
        panic!("a pattern row");
    };
    let q = MatchQuery::Pattern(pattern);
    let canonical = root.0.canonicalize().expect("canonical");
    let scans = || {
        DIRECTORY_SCANS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .filter(|scanned| **scanned == canonical)
            .count()
    };
    let before = scans();
    let open = SCAN_FLIGHT.join(flight_key(q, &scan_key(q, &paths)));
    let gate = std::sync::Barrier::new(2);
    let outs = std::thread::scope(|scope| {
        let rows = (0..2)
            .map(|_| {
                scope.spawn(|| {
                    gate.wait();
                    super::super::test_rows::run(&root.0, query.clone()).expect("match")
                })
            })
            .collect::<Vec<_>>();
        rows.into_iter()
            .map(|row| row.join().expect("row"))
            .collect::<Vec<_>>()
    });
    drop(open);
    assert_eq!(outs[0], outs[1]);
    assert_eq!(outs[0]["matchCount"], 60, "{}", outs[0]);
    assert_eq!(scans() - before, 1, "one scan for both rows");
}

/// A row waiting for an identical row's scan (another request's, holding
/// the shared turn) stops on its own cancellation instead of waiting until
/// that scan releases the turn.
#[test]
fn a_cancelled_row_stops_waiting_for_another_requests_scan() {
    let root = Fixture::new();
    std::fs::write(root.0.join("source.ts"), "target(1);\n").expect("source");
    let (paths, security) = simple_policy(&root.0);
    let row = typed(&json!({"operation":"match", "path":root.0, "pattern":"target($A)"}));
    let super::super::AstSearchQuery::MatchPattern(query) = &row else {
        panic!("a pattern row");
    };
    let q = MatchQuery::Pattern(query);
    let holder = SCAN_FLIGHT.join(flight_key(q, &scan_key(q, &paths)));
    let held = holder.blocking_turn();
    let cancel = Flag(AtomicBool::new(false));
    let (answered, answer) = std::sync::mpsc::channel::<()>();
    let (outcome, held_until_answered) = std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(100));
            cancel.0.store(true, Ordering::SeqCst);
        });
        // The holder's scan outlasts the waiter: it keeps the turn until the
        // waiter has answered.
        let holding = scope.spawn(move || {
            let answered_first = answer.recv_timeout(HELD_TURN_BOUND).is_ok();
            drop(held);
            answered_first
        });
        let outcome = execute_match(q, &paths, &security, &cancel, None);
        let _ = answered.send(());
        (outcome, holding.join().expect("holder"))
    });
    assert_eq!(
        outcome.err().map(|error| error.code),
        Some(ErrorCode::Cancelled)
    );
    assert!(
        held_until_answered,
        "the cancelled row waited for the holder's turn"
    );
}
