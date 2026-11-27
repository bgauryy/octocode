use super::*;

/// However the workers interleave, what comes back is a prefix, in order.
#[test]
fn a_stop_inside_a_chunk_leaves_a_contiguous_prefix() {
    let entries = (0..2_000).collect::<Vec<usize>>();
    let done = AtomicUsize::new(0);
    let results =
        in_walk_order::<_, _, ()>(&entries, &|| done.load(Ordering::Relaxed) >= 100, |entry| {
            done.fetch_add(1, Ordering::Relaxed);
            Ok(*entry)
        })
        .expect("no error");
    assert!(
        results.len() >= 100 && results.len() < entries.len(),
        "{}",
        results.len()
    );
    assert_eq!(results, entries[..results.len()]);
}

#[test]
fn the_first_entry_always_starts() {
    let results = in_walk_order::<_, _, ()>(&[7, 8, 9], &|| true, |entry| Ok(*entry));
    assert_eq!(results, Ok(vec![7]));
}

/// Workers racing for the first entry while the scan is already held:
/// only one of them starts it (once, two read it unclaimed and started
/// the first two).
#[test]
fn the_first_entry_starts_alone_however_workers_race() {
    let entries = (0..64).collect::<Vec<usize>>();
    for _ in 0..2_000 {
        let results = in_walk_order::<_, _, ()>(&entries, &|| true, |entry| Ok(*entry));
        assert_eq!(results, Ok(vec![0]));
    }
}

#[test]
fn with_no_stop_every_entry_comes_back_in_order() {
    let entries = (0..500).collect::<Vec<usize>>();
    let results = in_walk_order::<_, _, ()>(&entries, &|| false, |entry| Ok(*entry));
    assert_eq!(results, Ok(entries));
}

#[test]
fn the_first_error_by_walk_order_wins() {
    let entries = (0..64).collect::<Vec<usize>>();
    let results = in_walk_order(&entries, &|| false, |entry| {
        if *entry >= 3 { Err(*entry) } else { Ok(*entry) }
    });
    assert_eq!(results, Err(3));
}

/// A panic in the work ends the watcher too: the panic comes back to the
/// caller instead of the scope waiting forever for a watcher told nothing.
#[test]
fn a_panicking_work_still_ends_the_watcher() {
    let stopped = std::sync::Arc::new(AtomicBool::new(false));
    let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        watch_stop(&|| false, &stopped, &|| -> () {
            std::panic::resume_unwind(Box::new("work unwound"))
        });
    }));
    assert!(unwound.is_err());
    assert!(!stopped.load(Ordering::Relaxed));
}
