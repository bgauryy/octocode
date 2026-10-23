use super::*;
use crate::{
    execution::cancel::NeverCancel,
    process_fixture::{Holder, Leader, RETURN_SLACK, Tree, gone_within, kill, until_set_up},
};

const RETURN_WITHIN: Duration = Duration::from_secs(5);

/// One call of a fixture tree that started its holder before the
/// call's timeout (a leader that exits also exited before it): the
/// result, the time the call took, the bound that time must stay under
/// (the timeout plus [`RETURN_SLACK`]), and the holder's pid.
fn run(
    leader: Leader,
    holder: Holder,
    detach: bool,
) -> (Result<BoundedOutput, BoundedError>, Duration, Duration, u32) {
    let timeouts = [2, 4, 8, 16].map(Duration::from_secs);
    until_set_up(&timeouts, |timeout| {
        let (command, tree) = Tree::new(leader, holder, detach);
        let started = Instant::now();
        let result = run_bounded(command, "fixture", timeout, 4 * 1024 * 1024, &NeverCancel);
        let elapsed = started.elapsed();
        let pid = tree.recorded_pid();
        let exited =
            matches!(leader, Leader::Sleep) || !matches!(result, Err(BoundedError::TimedOut(_)));
        if !exited && let Some(pid) = pid {
            kill(pid);
        }
        let pid = pid.filter(|_| exited)?;
        Some((result, elapsed, timeout + RETURN_SLACK, pid))
    })
}

/// A holder that left the process group outlives the call by design: no
/// parent walk or group kill can reach it. It must not hold the call.
#[cfg(unix)]
#[test]
fn a_detached_descendant_holding_the_pipes_does_not_delay_a_finished_command() {
    let (result, elapsed, within, pid) = run(Leader::Exit, Holder::Silent, true);
    let alive = crate::process_status::is_alive(pid);
    kill(pid);
    let output = result.expect("the command itself succeeded");
    assert!(elapsed < within, "returned after {elapsed:?}");
    assert!(alive, "the fixture must still hold the pipes at return");
    assert!(output.status.success());
    // The libtest harness prints its own banner around the fixture's text.
    assert!(
        output.stdout.windows(5).any(|window| window == b"hello"),
        "stdout lost what the command wrote before it ended"
    );
}

#[cfg(unix)]
#[test]
fn a_detached_descendant_holding_the_pipes_does_not_delay_a_timeout() {
    let (result, elapsed, within, pid) = run(Leader::Sleep, Holder::Silent, true);
    kill(pid);
    let error = result.err().expect("the leader outlived the timeout");
    assert!(matches!(error, BoundedError::TimedOut(_)), "{error:?}");
    assert!(elapsed < within, "returned after {elapsed:?}");
}

#[test]
fn a_streaming_descendant_is_released_when_the_call_returns() {
    let (result, elapsed, within, pid) = run(Leader::Exit, Holder::Streaming, true);
    let released = gone_within(pid, RETURN_WITHIN);
    kill(pid);
    // Whether the stream outran the cap or went quiet first is a race; the
    // call must return either way, and must not keep its read ends open.
    drop(result);
    assert!(elapsed < within, "returned after {elapsed:?}");
    assert!(released, "the call kept its read end open after returning");
}

/// Containment, not escape: on Unix the group kill, on Windows the job,
/// takes the holder down with the command.
#[test]
fn a_timeout_leaves_no_descendant_holding_the_pipes() {
    let (result, elapsed, within, pid) = run(Leader::Sleep, Holder::Silent, false);
    let gone = gone_within(pid, RETURN_WITHIN);
    kill(pid);
    let error = result.err().expect("the leader outlived the timeout");
    assert!(matches!(error, BoundedError::TimedOut(_)), "{error:?}");
    assert!(elapsed < within, "returned after {elapsed:?}");
    assert!(gone, "descendant {pid} survived the timeout");
}

#[test]
fn a_finished_command_leaves_no_descendant_holding_the_pipes() {
    let (result, elapsed, within, pid) = run(Leader::Exit, Holder::Silent, false);
    let gone = gone_within(pid, RETURN_WITHIN);
    kill(pid);
    let output = result.expect("the command itself succeeded");
    assert!(output.status.success());
    assert!(elapsed < within, "returned after {elapsed:?}");
    assert!(gone, "descendant {pid} survived the command");
}
