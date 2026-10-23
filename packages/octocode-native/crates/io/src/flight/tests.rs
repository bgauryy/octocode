use super::KeyedFlight;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[tokio::test]
async fn one_key_takes_turns_and_later_turns_see_the_first_ones_state() {
    let flight = Arc::new(KeyedFlight::<&str, Option<u32>>::transient());
    let work = Arc::new(AtomicUsize::new(0));
    let tasks = (0..8)
        .map(|_| {
            let (flight, work) = (Arc::clone(&flight), Arc::clone(&work));
            tokio::spawn(async move {
                let ticket = flight.join("key");
                let mut slot = ticket.turn().await;
                if slot.is_none() {
                    work.fetch_add(1, Ordering::SeqCst);
                    tokio::task::yield_now().await;
                    *slot = Some(7);
                }
                slot.unwrap_or_default()
            })
        })
        .collect::<Vec<_>>();
    for task in tasks {
        assert_eq!(task.await.unwrap(), 7);
    }
    assert_eq!(
        work.load(Ordering::SeqCst),
        1,
        "the first turn did the work"
    );
    assert!(
        flight.is_empty(),
        "a transient key is forgotten after its wave"
    );
    let ticket = flight.join("key");
    assert_eq!(*ticket.turn().await, None, "a new wave starts empty");
}

#[tokio::test]
async fn retained_slots_outlive_their_wave_and_idle_ones_are_pruned_past_the_cap() {
    let flight = KeyedFlight::<u32, Option<u32>>::retained(2);
    let held = flight.join(0);
    *held.turn().await = Some(0);
    for key in 1..3 {
        *flight.join(key).turn().await = Some(key);
    }
    assert_eq!(flight.len(), 3, "idle slots are retained up to the cap");
    assert_eq!(
        *flight.join(1).turn().await,
        None,
        "past the cap idle slots go"
    );
    assert_eq!(flight.len(), 2);
    assert_eq!(*held.turn().await, Some(0), "a held slot survives pruning");
}

/// Blocking threads take turns on one key like async requests do: the
/// first does the work, the rest reuse its slot, and the key is
/// forgotten after the wave. Every thread joins before any takes its
/// turn, so all eight are one wave by construction.
#[test]
fn blocking_turns_share_one_slot_across_threads() {
    let flight = KeyedFlight::<&str, Option<u32>>::transient();
    let work = AtomicUsize::new(0);
    let joined = std::sync::Barrier::new(8);
    std::thread::scope(|scope| {
        let tasks = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    let ticket = flight.join("key");
                    joined.wait();
                    let mut slot = ticket.blocking_turn();
                    if slot.is_none() {
                        work.fetch_add(1, Ordering::SeqCst);
                        *slot = Some(7);
                    }
                    slot.unwrap_or_default()
                })
            })
            .collect::<Vec<_>>();
        for task in tasks {
            assert_eq!(task.join().unwrap(), 7);
        }
    });
    assert_eq!(work.load(Ordering::SeqCst), 1);
    assert!(flight.is_empty());
}

/// A blocking turn taken on a thread inside a runtime (current-thread
/// and multi-thread `block_on`) waits like any other, never panics.
#[test]
fn blocking_turns_work_on_threads_inside_a_runtime() {
    let flight = KeyedFlight::<&str, Option<u32>>::transient();
    let turn = || {
        let ticket = flight.join("key");
        let mut slot = ticket.blocking_turn();
        *slot.get_or_insert(9)
    };
    let current = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("runtime");
    assert_eq!(current.block_on(async { turn() }), 9);
    let multi = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .build()
        .expect("runtime");
    assert_eq!(multi.block_on(async { turn() }), 9);
    let _entered = multi.enter();
    assert_eq!(turn(), 9);
}

/// Runs `turn` on its own thread and fails, instead of hanging, when it
/// does not finish in time.
fn within_seconds(turn: impl FnOnce() -> u32 + Send + 'static) -> u32 {
    let (sent, received) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sent.send(turn());
    });
    received
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("the blocking turn came")
}

/// A thread whose coop budget is spent still takes an uncontended
/// blocking turn: under `Runtime::block_on` (where a budget-starved poll
/// would park forever) and under `Handle::block_on` from a blocking
/// worker, the shape rows run in (where it would spin forever).
#[test]
fn a_blocking_turn_on_a_thread_with_a_spent_coop_budget_still_comes() {
    static FLIGHT: KeyedFlight<&str, Option<u32>> = KeyedFlight::transient();
    async fn spent_then_turn() -> u32 {
        for _ in 0..256 {
            tokio::task::coop::consume_budget().await;
        }
        let ticket = FLIGHT.join("key");
        *ticket.blocking_turn().get_or_insert(3)
    }
    let current = within_seconds(|| {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("runtime")
            .block_on(spent_then_turn())
    });
    assert_eq!(current, 3);
    let from_blocking_worker = within_seconds(|| {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .build()
            .expect("runtime");
        let handle = runtime.handle().clone();
        let worker = runtime.spawn_blocking(move || handle.block_on(spent_then_turn()));
        runtime.block_on(worker).expect("blocking worker")
    });
    assert_eq!(from_blocking_worker, 3);
}

/// The last tickets of a wave dropping at once still forget the key:
/// every wave starts from an empty slot, never the previous one's state.
#[test]
fn concurrent_last_drops_never_leak_a_transient_key_into_the_next_wave() {
    let flight = KeyedFlight::<u8, Option<u32>>::transient();
    let threads = 8;
    let barrier = std::sync::Barrier::new(threads);
    // Miri explores interleavings per seed instead (`$DEV miri`); 5 000
    // interpreted waves would outlast its deadline.
    let waves = if cfg!(miri) { 8 } else { 5_000 };
    for wave in 0..waves {
        std::thread::scope(|scope| {
            for _ in 0..threads {
                scope.spawn(|| {
                    barrier.wait();
                    let ticket = flight.join(1);
                    let mut slot = ticket.blocking_turn();
                    slot.get_or_insert(wave);
                });
            }
        });
        assert!(flight.is_empty(), "wave {wave} left its key behind");
    }
}

/// A blocking waiter stops when its own stop check fails, while the
/// holder still has the turn, and leaves nothing behind.
#[test]
fn a_blocking_waiter_stops_on_its_own_check_while_the_holder_works() {
    let flight = KeyedFlight::<u8, ()>::transient();
    let holder = flight.join(1);
    let held = holder.blocking_turn();
    let stopped = std::sync::atomic::AtomicBool::new(false);
    let started = std::time::Instant::now();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(50));
            stopped.store(true, Ordering::SeqCst);
        });
        let waiter = flight.join(1);
        let outcome = waiter.blocking_turn_until(|| {
            if stopped.load(Ordering::SeqCst) {
                Err("cancelled")
            } else {
                Ok(())
            }
        });
        assert_eq!(outcome.err(), Some("cancelled"));
    });
    assert!(
        started.elapsed() < std::time::Duration::from_secs(1),
        "the waiter stopped within a few ticks: {:?}",
        started.elapsed()
    );
    drop(held);
    drop(holder);
    assert!(flight.is_empty());
}

/// Concurrent misses of one key compute it once: the others take the
/// holder's value from the slot. A later call reads the memo without
/// computing, and a waiter whose stop fails returns its error while the
/// holder still computes.
#[test]
fn memoized_blocking_computes_once_and_reads_the_memo_after() {
    let flight = KeyedFlight::<u8, Option<u32>>::transient();
    let memo = std::sync::Mutex::new(std::collections::BTreeMap::<u8, u32>::new());
    let computed = AtomicUsize::new(0);
    let lookup = |key: &u8| memo.lock().unwrap().get(key).copied();
    let compute = |key: u8| {
        computed.fetch_add(1, Ordering::SeqCst);
        std::thread::sleep(std::time::Duration::from_millis(50));
        memo.lock().unwrap().insert(key, 7);
        7
    };
    let never = || Ok::<(), &str>(());
    let barrier = std::sync::Barrier::new(8);
    std::thread::scope(|scope| {
        let rows = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    barrier.wait();
                    flight.memoized_blocking(1, lookup, compute, never)
                })
            })
            .collect::<Vec<_>>();
        for row in rows {
            assert_eq!(row.join().unwrap(), Ok(7));
        }
    });
    assert_eq!(computed.load(Ordering::SeqCst), 1);
    assert!(flight.is_empty());
    let hit = flight.memoized_blocking(1, lookup, |_| unreachable!("a memo hit"), never);
    assert_eq!(hit, Ok(7));
    let holder = flight.join(2);
    let held = holder.blocking_turn();
    let stopped = flight.memoized_blocking(2, lookup, |_| 0, || Err("cancelled"));
    assert_eq!(stopped, Err("cancelled"));
    drop(held);
    drop(holder);
    assert!(flight.is_empty());
}

#[tokio::test]
async fn a_cancelled_waiter_forgets_its_transient_key() {
    let flight = KeyedFlight::<u8, ()>::transient();
    let first = flight.join(1);
    let guard = first.turn().await;
    {
        let second = flight.join(1);
        let waiting = second.turn();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(10), waiting)
                .await
                .is_err()
        );
    }
    drop(guard);
    drop(first);
    assert!(flight.is_empty());
}
