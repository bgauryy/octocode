//! One chunk of a scan, its files started strictly in walk order, so a scan that
//! stops starting them inside a chunk has evaluated a contiguous prefix of it.
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use rayon::prelude::*;

use super::super::ScanHold;
use super::outcome::GraphFactsScanOutcome;

/// `evaluate` of `entries` in parallel, each started only after every entry
/// before it, until `held` says the files finished already pass what the scan may
/// hold: the results of the entries started, in order. Which ones those are
/// depends on how the workers ran; the prefix they form always holds the file
/// where the running total in walk order first passes the limit, since it holds
/// every finished file. The first entry always starts, so a window whose
/// first file alone passes the budget still advances. The first error by walk
/// order wins, and no entry starts after one.
pub(super) fn in_walk_order<E: Sync, R: Send, X: Send>(
    entries: &[E],
    held: &(dyn Fn() -> bool + Sync),
    evaluate: impl Fn(&E) -> Result<R, X> + Sync,
) -> Result<Vec<R>, X> {
    let next = AtomicUsize::new(0);
    let failed = AtomicBool::new(false);
    let slots = entries
        .iter()
        .map(|_| Mutex::new(None))
        .collect::<Vec<Mutex<Option<Result<R, X>>>>>();
    (0..rayon::current_num_threads().min(entries.len()).max(1))
        .into_par_iter()
        .for_each(|_| {
            while !failed.load(Ordering::Relaxed) {
                // Once held, only the first entry may still start: claimed by
                // one exchange, so two workers cannot both see it unclaimed
                // and start the first two.
                let at = if held() {
                    match next.compare_exchange(0, 1, Ordering::Relaxed, Ordering::Relaxed) {
                        Ok(first) => first,
                        Err(_) => return,
                    }
                } else {
                    next.fetch_add(1, Ordering::Relaxed)
                };
                let Some(entry) = entries.get(at) else {
                    return;
                };
                let result = evaluate(entry);
                failed.fetch_or(result.is_err(), Ordering::Relaxed);
                *slots[at]
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(result);
            }
        });
    slots
        .into_iter()
        .map_while(|slot| {
            slot.into_inner()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
        })
        .collect()
}

/// A file's result, and what it adds to what the scan holds.
pub(super) type Weighed<T> = (usize, Option<GraphFactsScanOutcome<T>>);

/// [`in_walk_order`] of a chunk of a scan holding `held` bytes of `limit`: each
/// file's result with what it adds to them.
pub(super) fn weighed_chunk<E: Sync, T: Send, X: Send>(
    chunk: &[E],
    (held, limit): (usize, usize),
    hold: Option<&ScanHold<'_, T>>,
    evaluate: impl Fn(&E) -> Result<Option<GraphFactsScanOutcome<T>>, X> + Sync,
) -> Result<Vec<Weighed<T>>, X> {
    let finished = AtomicUsize::new(held);
    in_walk_order(
        chunk,
        &|| finished.load(Ordering::Relaxed) > limit,
        |entry| {
            let outcome = evaluate(entry)?;
            let weight = match (hold, &outcome) {
                (Some(hold), Some(GraphFactsScanOutcome::Entry(_, Some(value)))) => {
                    (hold.weigh)(value)
                }
                _ => 0,
            };
            finished.fetch_add(weight, Ordering::Relaxed);
            Ok((weight, outcome))
        },
    )
}

/// Run `work` while a scoped watcher raises `stopped` once `stop` reports the
/// scan cancelled, so deep-stack jobs of the work stop with the scan.
pub(super) fn watch_stop<R>(
    stop: &crate::search::walk::Stop<'_>,
    stopped: &std::sync::Arc<AtomicBool>,
    work: &dyn Fn() -> R,
) -> R {
    let done = AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            while !done.load(Ordering::Relaxed) {
                if stop() {
                    stopped.store(true, Ordering::Relaxed);
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        });
        let result = work();
        done.store(true, Ordering::Relaxed);
        result
    })
}

#[cfg(test)]
mod tests {
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
}
