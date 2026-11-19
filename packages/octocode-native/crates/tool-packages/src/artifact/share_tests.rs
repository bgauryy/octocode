//! The call share's single flight.
use super::Flights;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Askers of one key at once wait for one read and each get its answer;
/// a later asker in the call gets it too; another key reads on its own.
#[tokio::test]
async fn one_key_is_read_once_however_many_ask() {
    let flights = Flights::<String>::default();
    let reads = AtomicUsize::new(0);
    let read = |answer: &'static str| {
        let reads = &reads;
        async move {
            reads.fetch_add(1, Ordering::SeqCst);
            tokio::task::yield_now().await;
            answer.to_owned()
        }
    };
    let (first, second, third) = tokio::join!(
        flights.once("a".into(), read("A")),
        flights.once("a".into(), read("not read")),
        flights.once("b".into(), read("B")),
    );
    assert_eq!(
        (first.as_str(), second.as_str(), third.as_str()),
        ("A", "A", "B")
    );
    assert_eq!(flights.once("a".into(), read("not read")).await, "A");
    assert_eq!(reads.load(Ordering::SeqCst), 2);
}

/// An asker that gives up mid-read leaves the key to the next one, which
/// reads it.
#[tokio::test]
async fn an_abandoned_read_leaves_the_key_to_the_next_asker() {
    let flights = Flights::<u32>::default();
    let abandoned = flights.once("k".into(), std::future::pending::<u32>());
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(10), abandoned)
            .await
            .is_err()
    );
    assert_eq!(flights.once("k".into(), async { 7 }).await, 7);
}
