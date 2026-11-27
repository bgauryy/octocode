use super::*;

#[tokio::test]
async fn a_pause_that_would_pass_the_deadline_fails_without_sleeping() {
    let token = Cancellation::new();
    let deadline = Instant::now() + Duration::from_secs(30);
    let started = Instant::now();
    assert_eq!(
        pause(Duration::from_secs(60), deadline, &token).await,
        Err(BudgetStop::Deadline)
    );
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[tokio::test]
async fn a_pause_inside_the_deadline_sleeps_the_delay() {
    let token = Cancellation::new();
    let deadline = Instant::now() + Duration::from_secs(30);
    let started = Instant::now();
    assert_eq!(
        pause(Duration::from_millis(40), deadline, &token).await,
        Ok(())
    );
    assert!(started.elapsed() >= Duration::from_millis(40));
    assert_eq!(pause(Duration::ZERO, deadline, &token).await, Ok(()));
}

#[tokio::test]
async fn a_cancelled_token_stops_a_pause_before_and_during_the_sleep() {
    let token = Cancellation::new();
    let deadline = Instant::now() + Duration::from_secs(60);
    let sleeping = pause(Duration::from_secs(30), deadline, &token);
    let cancel = async {
        tokio::time::sleep(Duration::from_millis(20)).await;
        token.cancel();
    };
    let (stopped, ()) = tokio::join!(sleeping, cancel);
    assert_eq!(stopped, Err(BudgetStop::Cancelled));
    assert_eq!(
        pause(Duration::ZERO, deadline, &token).await,
        Err(BudgetStop::Cancelled)
    );
}

/// A reporting wait says its remaining time when it starts and at each
/// beat, then ends on time; it says nothing to a call nobody listens to,
/// and nothing for a wait the deadline refuses.
#[tokio::test]
async fn a_reporting_pause_beats_while_it_waits() {
    let heard = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = std::sync::Arc::clone(&heard);
    let scope = crate::notice::Scope::with_progress(move |line| {
        sink.lock().expect("lines").push(line.to_owned());
    });
    let token = Cancellation::new();
    let deadline = Instant::now() + Duration::from_secs(60);
    let delay = REPORTED_FROM * 2 + Duration::from_millis(100);
    let beat = REPORTED_FROM;
    let line = |remaining: Duration| format!("{}ms", remaining.as_millis());
    let started = Instant::now();
    scope
        .run(pause_beating(delay, beat, deadline, &token, line))
        .await
        .expect("waited");
    assert!(started.elapsed() >= delay);
    assert_eq!(*heard.lock().expect("lines"), ["2100ms", "1100ms", "100ms"]);

    heard.lock().expect("lines").clear();
    let refused = scope.run(pause_beating(delay, beat, started, &token, line));
    assert_eq!(refused.await, Err(BudgetStop::Deadline));
    scope
        .run(pause_beating(
            REPORTED_FROM / 10,
            beat,
            deadline,
            &token,
            line,
        ))
        .await
        .expect("short");
    pause_beating(REPORTED_FROM, beat, deadline, &token, line)
        .await
        .expect("unheard");
    assert!(heard.lock().expect("lines").is_empty());
}

/// Each stalled read gets the stall bound anew, so reads that each finish
/// within it run past it in total; one that outlasts it ends at the
/// stall, far from the deadline.
#[tokio::test]
async fn a_stalled_wait_ends_at_the_stall_bound_not_the_deadline() {
    let stall = Duration::from_millis(60);
    let budget = RequestBudget::with_timeout(Duration::from_secs(30), 0).with_stall(stall);
    let started = Instant::now();
    for _ in 0..4 {
        budget
            .wait_stalled(tokio::time::sleep(stall / 2))
            .await
            .expect("a read inside the stall bound");
    }
    assert!(started.elapsed() >= stall);
    assert_eq!(
        budget.wait_stalled(std::future::pending::<()>()).await,
        Err(BudgetStop::Deadline)
    );
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(budget.check(), Ok(()));
    let unbounded = RequestBudget::with_timeout(Duration::from_millis(80), 0);
    assert_eq!(
        unbounded.wait_stalled(std::future::pending::<()>()).await,
        Err(BudgetStop::Deadline)
    );
}

#[tokio::test]
async fn a_budget_wait_ends_at_its_deadline_or_cancellation() {
    let budget = RequestBudget::with_timeout(Duration::from_millis(30), 0);
    assert_eq!(
        budget.wait(std::future::pending::<()>()).await,
        Err(BudgetStop::Deadline)
    );
    assert_eq!(budget.check(), Err(BudgetStop::Deadline));
    let cancelled = RequestBudget::with_timeout(Duration::from_secs(30), 0);
    cancelled.cancellation.cancel();
    assert_eq!(
        cancelled.wait(async { 1 }).await,
        Err(BudgetStop::Cancelled)
    );
}

/// Work that outlasts the first wait says so then and at each beat,
/// returns its own output, and says nothing when it is quick.
#[tokio::test]
async fn slow_work_reports_while_it_runs() {
    let heard = std::sync::Arc::new(std::sync::Mutex::new(0usize));
    let sink = std::sync::Arc::clone(&heard);
    let scope = crate::notice::Scope::with_progress(move |_| {
        *sink.lock().expect("count") += 1;
    });
    let from = Duration::from_millis(30);
    let beat = Duration::from_millis(20);
    let slow = async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        7
    };
    let out = scope
        .run(report_beating(slow, from, beat, |_| String::new()))
        .await;
    assert_eq!(out, 7);
    assert!(*heard.lock().expect("count") >= 3);
    *heard.lock().expect("count") = 0;
    let quick = scope
        .run(report_beating(async { 1 }, from, beat, |_| String::new()))
        .await;
    assert_eq!(quick, 1);
    assert_eq!(*heard.lock().expect("count"), 0);
}
