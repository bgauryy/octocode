use super::{Scope, carried, raise, raise_among};
use std::sync::Arc;

#[tokio::test]
async fn repeated_diagnostics_are_scoped_and_unscoped_ones_are_not_broadcast() {
    let a = Scope::default();
    let b = Scope::default();
    a.run(async {
        super::emit("trace-repeat", Scope::current().as_ref());
        super::emit("trace-repeat", Scope::current().as_ref());
    })
    .await;
    b.run(async { super::emit("trace-unscoped", None) }).await;
    assert_eq!(of(&a, "trace-repeat"), ["trace-repeat", "trace-repeat"]);
    assert!(of(&a, "trace-unscoped").is_empty());
    assert!(of(&b, "trace-repeat").is_empty());
    assert!(of(&b, "trace-unscoped").is_empty());
}

fn of(scope: &Scope, kind: &str) -> Vec<String> {
    scope
        .notices()
        .into_iter()
        .filter(|text| text.starts_with(kind))
        .collect()
}

/// One test: a process-level raise outside every scope reaches every
/// live scope, so separate tests running at once would see each other's.
#[tokio::test]
async fn a_notice_belongs_to_the_call_that_raised_it() {
    let (a, b) = (Scope::default(), Scope::default());
    // In a task's scope: that call's only, the first of a kind once.
    a.run(async {
        raise("notice-test-task", "notice-test-task: one");
        tokio::task::yield_now().await;
        raise("notice-test-task", "notice-test-task: two");
    })
    .await;
    assert_eq!(of(&a, "notice-test-task"), ["notice-test-task: one"]);
    assert!(of(&b, "notice-test-task").is_empty(), "not another call's");

    // Work carried to another thread keeps its caller's scope.
    let carried = b
        .run(async {
            std::thread::spawn(carried(|| {
                raise("notice-test-thread", "notice-test-thread: b");
            }))
        })
        .await;
    carried.join().expect("thread");
    assert_eq!(of(&b, "notice-test-thread"), ["notice-test-thread: b"]);
    assert!(of(&a, "notice-test-thread").is_empty());

    // Leaving the guard restores the thread's previous scope.
    {
        let _in = a.enter();
        {
            let _inner = b.enter();
        }
        raise("notice-test-restored", "notice-test-restored: a");
    }
    assert_eq!(of(&a, "notice-test-restored").len(), 1);
    assert!(of(&b, "notice-test-restored").is_empty());

    // A process-level kind raised outside every scope: every call in
    // flight relays it.
    std::thread::spawn(|| {
        raise_among(
            "notice-test-unscoped",
            "notice-test-unscoped: all",
            &["notice-test-unscoped"],
        );
    })
    .join()
    .expect("thread");
    assert_eq!(of(&a, "notice-test-unscoped").len(), 1);
    assert_eq!(of(&b, "notice-test-unscoped").len(), 1);

    // A scope that ended takes no more; a later call that raises the
    // kind again relays it again, though the process says it once.
    drop(b);
    let later = Scope::default();
    later
        .run(async { raise("notice-test-task", "notice-test-task: one") })
        .await;
    assert_eq!(of(&later, "notice-test-task"), ["notice-test-task: one"]);
}

/// Two calls in flight: a worker one of them spawns raises in that call
/// alone when it carries the scope, and a worker that carries nothing
/// cannot name its call, so a notice about a request reaches neither.
#[tokio::test]
async fn a_spawned_workers_request_notice_never_reaches_the_other_call() {
    let (a, b) = (Scope::default(), Scope::default());
    let (carrying, bare) = a
        .run(async {
            let carrying = std::thread::spawn(carried(|| {
                raise(
                    "notice-test-worker-carried",
                    "notice-test-worker-carried: a",
                );
            }));
            let bare = std::thread::spawn(|| {
                raise("notice-test-worker-bare", "notice-test-worker-bare: lost");
            });
            (carrying, bare)
        })
        .await;
    carrying.join().expect("carrying worker");
    bare.join().expect("bare worker");
    assert_eq!(
        of(&a, "notice-test-worker-carried"),
        ["notice-test-worker-carried: a"]
    );
    assert!(of(&b, "notice-test-worker-carried").is_empty());
    for call in [&a, &b] {
        assert!(
            of(call, "notice-test-worker-bare").is_empty(),
            "an unscoped request notice is in no call"
        );
    }
}

/// A note for the result is the call's own, carried to its workers,
/// said once per kind and never among the scope's stderr notices.
#[test]
fn a_result_note_stays_with_its_call_and_off_stderr() {
    let (call, other) = (super::ResultNotes::default(), super::ResultNotes::default());
    let scope = Scope::default();
    {
        let _in = scope.enter();
        let _noting = call.enter();
        super::raise_in_result("notice-test-result", "notice-test-result: one");
        std::thread::spawn(carried(|| {
            super::raise_in_result("notice-test-result", "notice-test-result: two");
            super::raise_in_result("notice-test-result-b", "notice-test-result-b: b");
        }))
        .join()
        .expect("worker");
    }
    assert_eq!(
        call.take(),
        ["notice-test-result: one", "notice-test-result-b: b"]
    );
    assert!(call.take().is_empty(), "taken once");
    assert!(other.take().is_empty(), "not another call's");
    assert!(
        of(&scope, "notice-test-result").is_empty(),
        "{:?}",
        scope.notices()
    );
}

/// Progress lines go to the call's listener, else its notices (a warm
/// server relays those), and nowhere when nothing listens.
#[test]
fn progress_goes_to_the_listener_else_the_notices_else_nowhere() {
    let heard = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = Arc::clone(&heard);
    let listening = Scope::with_progress(move |line| {
        sink.lock().expect("lines").push(line.to_owned());
    });
    let quiet = Scope::default();
    for scope in [&listening, &quiet] {
        let _in = scope.enter();
        super::progress_sink().expect("a sink")("waiting 3s");
    }
    assert_eq!(*heard.lock().expect("lines"), ["waiting 3s"]);
    // Another test's process-level notice reaches these too.
    let own = |scope: &Scope| {
        let mut notices = scope.notices();
        notices.retain(|notice| !notice.starts_with("notice-test-"));
        notices
    };
    assert_eq!(own(&quiet), ["waiting 3s"]);
    assert!(own(&listening).is_empty());
}

/// A scope with a progress listener hands it to the code running in it,
/// on its thread and on work carried elsewhere; other scopes have none.
#[test]
fn a_scope_carries_its_progress_listener() {
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let scope = Scope::with_progress(move |line| {
        sink.lock().expect("lines").push(line.to_owned());
    });
    let _in = scope.enter();
    let carried = carried(|| Scope::current().and_then(|scope| scope.progress()));
    let listener = std::thread::spawn(carried)
        .join()
        .expect("thread")
        .expect("listener");
    listener("parsed 1/2 files");
    assert_eq!(*seen.lock().expect("lines"), ["parsed 1/2 files"]);
    assert!(Scope::default().progress().is_none());
}
