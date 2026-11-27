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

/// The last tickets of a wave dropping at once still forget the key:
/// every wave starts from an empty slot, never the previous one's state.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn concurrent_last_drops_never_leak_a_transient_key_into_the_next_wave() {
    let flight = Arc::new(KeyedFlight::<u8, Option<u32>>::transient());
    let tasks = 8;
    // Miri explores interleavings per seed instead (`$DEV miri`); 5 000
    // interpreted waves would outlast its deadline.
    let waves = if cfg!(miri) { 8 } else { 5_000 };
    for wave in 0..waves {
        let barrier = Arc::new(tokio::sync::Barrier::new(tasks));
        let joined = (0..tasks)
            .map(|_| {
                let (flight, barrier) = (Arc::clone(&flight), Arc::clone(&barrier));
                tokio::spawn(async move {
                    barrier.wait().await;
                    let ticket = flight.join(1);
                    let mut slot = ticket.turn().await;
                    assert_eq!(*slot.get_or_insert(wave), wave, "a stale slot");
                })
            })
            .collect::<Vec<_>>();
        for task in joined {
            task.await.expect("turn");
        }
        assert!(flight.is_empty(), "wave {wave} left its key behind");
    }
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
