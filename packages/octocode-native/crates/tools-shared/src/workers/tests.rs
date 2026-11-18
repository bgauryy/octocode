use super::*;

/// A query's share of the process budget bounds its fan-out, as do its
/// cap and its items; there is always one worker.
#[test]
fn a_fan_out_takes_its_share_its_cap_and_its_items() {
    let share = workers(usize::MAX, None);
    assert!((1..=cpu::host_cores()).contains(&share));
    assert_eq!(workers(usize::MAX, Some(1)), 1);
    assert_eq!(workers(1, None), 1);
    assert_eq!(workers(0, None), 1);
}

/// Every item is done once, on any worker count, and the outcomes come
/// back in item order.
#[test]
fn each_parallel_does_every_item_once_in_order() {
    for threads in [1, 2, 7] {
        let mut items = (0..50).collect::<Vec<u32>>();
        let out = each_parallel(&mut items, threads, |item| {
            *item += 100;
            *item * 2
        });
        assert_eq!(out, (100..150).map(|n| n * 2).collect::<Vec<_>>());
        assert_eq!(items, (100..150).collect::<Vec<_>>());
    }
}

/// A failed job fails the fan-out with the lowest-index error that ran,
/// and a run without failures returns every outcome in item order.
#[test]
fn try_each_parallel_returns_outcomes_or_the_first_error() {
    for threads in [1, 2, 7] {
        let mut items = (0..50).collect::<Vec<u32>>();
        let ok = try_each_parallel(&mut items, threads, |item| Ok::<_, u32>(*item + 1));
        assert_eq!(ok, Ok((1..51).collect::<Vec<_>>()));
        let mut items = (0..50).collect::<Vec<u32>>();
        let failed = try_each_parallel(&mut items, threads, |item| {
            if *item >= 10 { Err(*item) } else { Ok(*item) }
        });
        assert!(matches!(failed, Err(at) if at >= 10), "{failed:?}");
    }
    let mut items = (0..50).collect::<Vec<u32>>();
    assert_eq!(
        try_each_parallel(&mut items, 1, |item| if *item >= 10 {
            Err(*item)
        } else {
            Ok(*item)
        }),
        Err(10),
        "one worker stops at the first failure"
    );
}

#[test]
fn a_failed_job_stops_pulling_new_items() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    for threads in [1, 2, 7] {
        let pulled = AtomicUsize::new(0);
        let items = (0..100).inspect(|_| {
            pulled.fetch_add(1, Ordering::Relaxed);
        });
        let outcome: Result<Vec<usize>, usize> = try_each_parallel(items, threads, Err);
        assert!(outcome.is_err());
        assert!(
            pulled.load(Ordering::Relaxed) <= threads,
            "pulled {} inputs on {threads} workers after an immediate error",
            pulled.load(Ordering::Relaxed)
        );
    }
}

#[test]
fn concurrent_errors_keep_the_lowest_running_item_index() {
    let started = std::sync::Barrier::new(2);
    let outcome: Result<Vec<usize>, usize> = try_each_parallel(0..2, 2, |index| {
        started.wait();
        Err(index)
    });
    assert_eq!(outcome, Err(0));
}

#[test]
fn empty_work_returns_no_outcomes() {
    for threads in [1, 2] {
        let result = try_each_parallel(std::iter::empty::<usize>(), threads, |item| {
            Ok::<_, usize>(item)
        });
        assert_eq!(result, Ok(Vec::new()));
    }
}

/// Concurrent calls' fan-outs draw on one process budget: while six
/// calls compute, their workers together never run on more threads
/// than the cores (one each when calls outnumber cores), however many
/// items each fan-out has. Sized as if each had the machine, six fan-outs
/// of 64 items would run up to six times the cores at once.
#[test]
fn concurrent_calls_fan_out_within_the_cores() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    const CALLS: usize = 6;
    let (active, peak) = (AtomicUsize::new(0), AtomicUsize::new(0));
    let entered = std::sync::Barrier::new(CALLS);
    let done = std::thread::scope(|scope| {
        let calls = (0..CALLS).map(|_| {
            scope.spawn(|| {
                let _computing = cpu::host().enter();
                entered.wait();
                let mut items = vec![0_u8; 64];
                let threads = workers(items.len(), None);
                each_parallel(&mut items, threads, |_| {
                    let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(now, Ordering::SeqCst);
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    active.fetch_sub(1, Ordering::SeqCst);
                })
                .len()
            })
        });
        calls
            .collect::<Vec<_>>()
            .into_iter()
            .map(|call| call.join().expect("call"))
            .sum::<usize>()
    });
    assert_eq!(done, CALLS * 64, "every item of every call ran");
    let peak = peak.load(Ordering::SeqCst);
    assert!(
        peak <= cpu::host_cores().max(CALLS),
        "{peak} threads on {} cores",
        cpu::host_cores()
    );
}
