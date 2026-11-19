use super::*;
use std::time::Duration;

/// A walked directory's ignore inputs: its present regular ignore files,
/// a nested repository's rule files outside the walk (never the root's:
/// those are the root's external inputs), and a link, a non-file or a
/// withheld ignore file as a dependency no stamp watches.
#[cfg(unix)]
#[test]
fn a_walked_directory_names_its_ignore_inputs() {
    let root = tempfile::tempdir().expect("fixture");
    let root = root.path();
    std::fs::create_dir(root.join(".git")).expect("root repository");
    std::fs::write(root.join(".gitignore"), "*.log\n").expect("fixture");
    let all = |_: &Path| true;
    let inputs = directory_ignore_inputs_with(
        root,
        root,
        octocode_engine::search::walk::GlobalIgnore::NONE,
        all,
    );
    assert_eq!(
        inputs,
        DirectoryInputs {
            files: vec![root.join(".gitignore")],
            ..DirectoryInputs::default()
        }
    );
    let nested = root.join("nested");
    std::fs::create_dir_all(nested.join(".git")).expect("nested repository");
    std::fs::write(nested.join(".ignore"), "tmp\n").expect("fixture");
    let inputs = directory_ignore_inputs_with(
        &nested,
        root,
        octocode_engine::search::walk::GlobalIgnore::NONE,
        all,
    );
    assert_eq!(inputs.files, vec![nested.join(".ignore")]);
    let real = std::fs::canonicalize(&nested).expect("real path");
    assert!(
        inputs
            .external
            .contains(&real.join(".git").join("info").join("exclude")),
        "{:?}",
        inputs.external
    );
    assert!(!inputs.untracked);
    assert!(
        directory_ignore_inputs_with(
            &nested,
            root,
            octocode_engine::search::walk::GlobalIgnore::NONE,
            |_| false
        )
        .untracked
    );
    std::os::unix::fs::symlink(nested.join(".ignore"), nested.join(".gitignore")).expect("link");
    assert!(
        directory_ignore_inputs_with(
            &nested,
            root,
            octocode_engine::search::walk::GlobalIgnore::NONE,
            all
        )
        .untracked
    );
    std::fs::create_dir(root.join("dir")).expect("fixture");
    std::fs::create_dir(root.join("dir").join(".ignore")).expect("a directory");
    assert!(
        directory_ignore_inputs_with(
            &root.join("dir"),
            root,
            octocode_engine::search::walk::GlobalIgnore::NONE,
            all
        )
        .untracked
    );
}

/// A charge grows the stored value's weight; one past the whole budget
/// is refused and changes nothing, and nothing stored takes no charge.
#[test]
fn a_charge_grows_a_stored_value_within_the_budget() {
    let memo = PageMemo::new();
    memo.put("a".into(), "p".into(), 3, ());
    let stored = memo.contents().1;
    assert!(memo.charge("a", 5));
    assert_eq!(memo.contents().1, stored + 5);
    assert!(!memo.charge("a", octocode_engine::memory::MEMORY.limit() + 1));
    assert_eq!(memo.contents().1, stored + 5);
    assert!(!memo.charge("missing", 1));
}

/// A value over the whole budget is not kept, and evicts nothing
/// (oldest-first eviction across stores: `octocode_io::memo` tests).
#[test]
fn a_value_over_the_whole_budget_is_not_kept() {
    let memo = PageMemo::new();
    memo.put("small".into(), "p".into(), 3, ());
    memo.put(
        "huge".into(),
        "p".into(),
        octocode_engine::memory::MEMORY.limit(),
        (),
    );
    assert_eq!(memo.contents(), (vec!["small".into()], 3 + 5 + 1));
}

#[test]
fn a_value_is_served_under_its_own_policy_until_replaced_or_evicted() {
    let memo = PageMemo::new();
    memo.put("s".into(), "a".into(), 1, 1);
    assert_eq!(memo.get("s", "a", |v| *v), Some(1));
    assert_eq!(memo.get("s", "b", |v| *v), None);
    memo.put("s".into(), "a".into(), 1, 2);
    assert_eq!(memo.get("s", "a", |v| *v), Some(2));
    assert_eq!(memo.contents().0.len(), 1);
    memo.evict("s");
    assert_eq!(memo.get("s", "a", |v| *v), None);
}

/// A test forgets what its own workspace kept: the values of another policy,
/// a test beside it in the process, stay served.
#[test]
fn evicting_a_policy_keeps_every_other_policys_values() {
    let memo = PageMemo::new();
    memo.put("mine".into(), "a".into(), 1, 1);
    memo.put("also mine".into(), "a".into(), 1, 2);
    memo.put("theirs".into(), "b".into(), 1, 3);
    memo.evict_policy("a");
    assert_eq!(memo.get("mine", "a", |v| *v), None);
    assert_eq!(memo.get("also mine", "a", |v| *v), None);
    assert_eq!(memo.get("theirs", "b", |v| *v), Some(3));
}

#[test]
fn a_value_is_served_while_its_proof_holds_and_evicted_when_refused() {
    let root = tempfile::tempdir().expect("fixture");
    let marker = root.path().join("marker");
    let proof = || {
        Watch {
            root: root.path(),
            sources: &[],
            rules: &[],
            markers: std::slice::from_ref(&marker),
        }
        .capture(Link::Follow, None)
        .expect("proof")
    };
    let memo = PageMemo::new();
    let get = |serve: fn(u8) -> Option<u8>| {
        memo.get_current(
            "s",
            "a",
            |(value, proof): &(u8, Proof)| (*value, proof.clone()),
            serve,
        )
    };
    memo.put("s".into(), "a".into(), 1, (7, proof()));
    assert_eq!(get(Some), Some(7));
    // `serve` refusing the parts evicts them.
    assert_eq!(get(|_| None), None);
    assert!(memo.contents().0.is_empty());
    // A changed watched entry fails the proof: not served, and evicted.
    memo.put("s".into(), "a".into(), 1, (7, proof()));
    std::fs::write(&marker, "").expect("marker");
    assert_eq!(get(Some), None);
    assert!(memo.contents().0.is_empty());
}

/// `value` stored under policy `a` as if put `age` ago.
fn put_aged(memo: &PageMemo<()>, age: Duration) {
    let stored_at = std::time::Instant::now()
        .checked_sub(age)
        .expect("monotonic clock covers the age");
    memo.memo.insert_full(
        "s".into(),
        ("a".into(), ()),
        (),
        1,
        Some(CONTINUATION_IDLE),
        stored_at,
    );
}

#[test]
fn a_read_restarts_the_continuation_lifetime() {
    let memo = PageMemo::new();
    put_aged(&memo, CONTINUATION_IDLE - Duration::from_secs(5));
    assert_eq!(memo.get("s", "a", |()| ()), Some(()));
    let age = memo.memo.get("s").expect("still stored").age;
    assert!(
        age < Duration::from_secs(5),
        "read restarted the lifetime: {age:?}"
    );
    // A read under another policy is no use of this value.
    put_aged(&memo, CONTINUATION_IDLE - Duration::from_secs(5));
    assert_eq!(memo.get("s", "b", |()| ()), None);
    let age = memo.memo.get("s").expect("still stored").age;
    assert!(age >= CONTINUATION_IDLE - Duration::from_secs(5), "{age:?}");
}

#[test]
fn an_expired_value_is_not_served() {
    let memo = PageMemo::new();
    put_aged(&memo, CONTINUATION_IDLE - Duration::from_millis(20));
    std::thread::sleep(Duration::from_millis(40));
    assert_eq!(memo.get("s", "a", |()| ()), None);
}

/// A gate a computation waits at until the test opens it.
#[derive(Default)]
struct Gate {
    open: std::sync::Mutex<bool>,
    signal: std::sync::Condvar,
}

impl Gate {
    fn wait(&self) {
        let mut open = self.open.lock().expect("gate");
        while !*open {
            open = self.signal.wait(open).expect("gate");
        }
    }

    fn open(&self) {
        *self.open.lock().expect("gate") = true;
        self.signal.notify_all();
    }
}

/// Concurrent requests for one key compute it once: the rest wait for the
/// one computing it and read what it kept.
#[test]
fn concurrent_requests_for_one_key_compute_it_once() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let memo = PageMemo::<usize>::new();
    let (computed, started, gate) = (AtomicUsize::new(0), AtomicUsize::new(0), Gate::default());
    let requests = 6;
    let values = std::thread::scope(|scope| {
        let tasks = (0..requests)
            .map(|_| {
                scope.spawn(|| {
                    started.fetch_add(1, Ordering::SeqCst);
                    memo.get_or_compute("one flight".into(), "p", &|| false, || {
                        computed.fetch_add(1, Ordering::SeqCst);
                        gate.wait();
                        Computed {
                            value: 7,
                            keep: Some(1),
                        }
                    })
                })
            })
            .collect::<Vec<_>>();
        // Every request is under way before the computation may finish.
        while started.load(Ordering::SeqCst) < requests {
            std::thread::yield_now();
        }
        gate.open();
        tasks
            .into_iter()
            .map(|task| task.join().expect("request"))
            .collect::<Vec<_>>()
    });
    assert_eq!(values, vec![7; requests]);
    assert_eq!(computed.load(Ordering::SeqCst), 1);
    memo.evict("one flight");
}

/// A computation that keeps nothing (it stopped) is no answer for anyone
/// else: each waiter computes in turn, and nothing is kept.
#[test]
fn a_flight_that_keeps_nothing_leaves_each_waiter_to_compute() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let memo = PageMemo::<usize>::new();
    let computed = AtomicUsize::new(0);
    let requests = 4;
    std::thread::scope(|scope| {
        for _ in 0..requests {
            scope.spawn(|| {
                memo.get_or_compute("unkept".into(), "p", &|| false, || {
                    computed.fetch_add(1, Ordering::SeqCst);
                    Computed {
                        value: 1,
                        keep: None,
                    }
                })
            });
        }
    });
    assert_eq!(computed.load(Ordering::SeqCst), requests);
    assert_eq!(memo.get("unkept", "p", |value| *value), None);
}

/// A waiter whose own request was stopped does not wait out the flight: it
/// computes for itself at once.
#[test]
fn a_stopped_waiter_does_not_wait_for_the_flight() {
    let memo = PageMemo::<usize>::new();
    let (leading, gate) = (Gate::default(), Gate::default());
    std::thread::scope(|scope| {
        let leader = scope.spawn(|| {
            memo.get_or_compute("stopped".into(), "p", &|| false, || {
                leading.open();
                gate.wait();
                Computed {
                    value: 1,
                    keep: Some(1),
                }
            })
        });
        leading.wait();
        let own = memo.get_or_compute("stopped".into(), "p", &|| true, || Computed {
            value: 2,
            keep: None,
        });
        assert_eq!(own, 2, "the stopped waiter answered for itself");
        gate.open();
        assert_eq!(leader.join().expect("leader"), 1);
    });
    memo.evict("stopped");
}

/// `requests` asks for one key at once while the first computes it.
fn shared_by(
    shared: &Shared<u32>,
    requests: usize,
    take: impl Fn(&u32) -> Option<u32> + Sync,
) -> (Vec<u32>, usize) {
    let computed = std::sync::atomic::AtomicUsize::new(0);
    let led = std::sync::atomic::AtomicBool::new(false);
    let answers = std::thread::scope(|scope| {
        let ask = || {
            shared.share(
                "key".to_owned(),
                &|| false,
                |_| {
                    let n = computed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    // The first computation waits until every other request
                    // waits on it.
                    if !led.swap(true, std::sync::atomic::Ordering::SeqCst) {
                        while shared.waiting("key") < requests - 1 {
                            std::thread::yield_now();
                        }
                    }
                    (100 + u32::try_from(n).expect("small"), Some(7))
                },
                &take,
            )
        };
        let tasks = (0..requests).map(|_| scope.spawn(ask)).collect::<Vec<_>>();
        tasks
            .into_iter()
            .map(|task| task.join().expect("request"))
            .collect::<Vec<_>>()
    });
    (answers, computed.into_inner())
}

#[test]
fn requests_that_ask_at_once_share_one_computation() {
    let shared = Shared::new();
    let (mut answers, computed) = shared_by(&shared, 5, |handed| Some(*handed * 10));
    answers.sort_unstable();
    assert_eq!(computed, 1);
    assert_eq!(answers, vec![70, 70, 70, 70, 100]);
    // Nothing outlives the flight: a later request computes.
    assert_eq!(
        shared.share("key".to_owned(), &|| false, |_| (1, None), |_| Some(2)),
        1
    );
}

#[test]
fn a_leader_learns_whether_any_request_waits_on_it() {
    let shared = Shared::<u32>::new();
    let alone = shared.share(
        "key".to_owned(),
        &|| false,
        |waited| (u32::from(waited()), None),
        |_| None,
    );
    assert_eq!(alone, 0);
    let seen = std::sync::atomic::AtomicBool::new(false);
    std::thread::scope(|scope| {
        let leader = scope.spawn(|| {
            shared.share(
                "key".to_owned(),
                &|| false,
                |waited| {
                    while shared.waiting("key") < 1 {
                        std::thread::yield_now();
                    }
                    seen.store(waited(), std::sync::atomic::Ordering::SeqCst);
                    (1, Some(9))
                },
                |_| None,
            )
        });
        while shared.in_flight_keys() == 0 {
            std::thread::yield_now();
        }
        assert_eq!(
            shared.share(
                "key".to_owned(),
                &|| false,
                |_| (2, None),
                |handed| Some(*handed)
            ),
            9
        );
        assert_eq!(leader.join().expect("joined"), 1);
    });
    assert!(seen.into_inner());
}

#[test]
fn a_waiter_that_refuses_what_was_handed_computes_for_itself() {
    let shared = Shared::new();
    let (answers, computed) = shared_by(&shared, 3, |_| None);
    assert_eq!(computed, 3);
    assert!(answers.iter().all(|answer| *answer >= 100), "{answers:?}");
}

#[test]
fn a_leader_that_unwinds_or_hands_nothing_leaves_each_waiter_to_compute() {
    let shared = Shared::<u32>::new();
    let computed = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|scope| {
        let leader = scope.spawn(|| {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                shared.share(
                    "key".to_owned(),
                    &|| false,
                    |_| {
                        while shared.waiting("key") < 1 {
                            std::thread::yield_now();
                        }
                        panic!("the leader unwinds");
                    },
                    |_| Some(0),
                )
            }))
        });
        // Joins the leader's flight (the leader waits for it), then computes.
        while shared.in_flight_keys() == 0 {
            std::thread::yield_now();
        }
        let waiter = shared.share(
            "key".to_owned(),
            &|| false,
            |_| {
                computed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                (5, None)
            },
            |_| Some(0),
        );
        assert!(leader.join().expect("joined").is_err());
        assert_eq!(waiter, 5);
    });
    assert_eq!(computed.into_inner(), 1);
    assert_eq!(shared.in_flight_keys(), 0);
}

#[test]
fn a_stopped_waiter_computes_without_waiting_for_the_leader() {
    let shared = Shared::<u32>::new();
    let release = std::sync::atomic::AtomicBool::new(false);
    std::thread::scope(|scope| {
        let leader = scope.spawn(|| {
            shared.share(
                "key".to_owned(),
                &|| false,
                |_| {
                    while !release.load(std::sync::atomic::Ordering::SeqCst) {
                        std::thread::yield_now();
                    }
                    (1, Some(1))
                },
                |_| Some(0),
            )
        });
        while shared.in_flight_keys() == 0 {
            std::thread::yield_now();
        }
        // Stopped: it does not wait for the leader, which waits for it.
        assert_eq!(
            shared.share("key".to_owned(), &|| true, |_| (2, None), |_| Some(0)),
            2
        );
        release.store(true, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(leader.join().expect("joined"), 1);
    });
}
