//! Candidate windows and bounded, ordered verification.
use super::*;

/// Following each window's digest from window 1 verifies every
/// candidate in exactly one window, in order, with no gap.
#[test]
fn importer_windows_cover_every_candidate_exactly_once() {
    for count in [0, 1, 23, 24, 25, 48, 60, 73] {
        let list = candidates(count);
        let mut reached: Vec<String> = Vec::new();
        let (mut page, mut digest) = (1, None::<String>);
        loop {
            let request = WindowRequest {
                page,
                expected: digest.as_deref(),
            };
            let Cut::Window(window) = Window::cut(list.clone(), request) else {
                panic!("window {page} of {count} is stale");
            };
            assert!(window.files().len() <= MAX_CANDIDATE_FILES);
            assert_eq!(window.covered().len(), reached.len() + window.files().len());
            assert_eq!(
                window.foreign().count() + window.files().len(),
                count,
                "own and foreign windows partition the list"
            );
            for file in window.files() {
                assert_eq!(window.owner(&file), page, "{file} belongs to its window");
            }
            assert_eq!(window.owner("/elsewhere.ts"), page, "non-candidates stay");
            reached.extend(window.files().iter().cloned());
            if !window.more() {
                assert_eq!(window.pages(), page);
                break;
            }
            page += 1;
            digest = Some(window.digest().to_owned());
        }
        assert_eq!(reached, list, "{count} candidates: once each, in order");
        assert_eq!(page as usize, count.div_ceil(MAX_CANDIDATE_FILES).max(1));
    }
}

/// A later window whose candidate list changed (or that no longer
/// exists) is stale; the first window never is.
#[test]
fn a_changed_candidate_list_stales_a_later_window() {
    let Cut::Window(first) = Window::cut(candidates(60), WindowRequest::default()) else {
        panic!("first window");
    };
    let mut changed = candidates(60);
    changed.insert(5, "/r/f004a.ts".into());
    let entering = WindowRequest {
        page: 2,
        expected: Some(first.digest()),
    };
    assert!(matches!(Window::cut(changed.clone(), entering), Cut::Stale));
    assert!(matches!(
        Window::cut(candidates(60), entering),
        Cut::Window(_)
    ));
    // A missing snapshot is not a match.
    let unsnapshotted = WindowRequest {
        page: 2,
        expected: Some(""),
    };
    assert!(matches!(
        Window::cut(candidates(60), unsnapshotted),
        Cut::Stale
    ));
    // Past the last window.
    let past = WindowRequest {
        page: 4,
        expected: None,
    };
    assert!(matches!(Window::cut(candidates(60), past), Cut::Stale));
    assert!(matches!(
        Window::cut(changed, WindowRequest::default()),
        Cut::Window(_)
    ));
}

/// Importer verification on a pipelining server: results keep candidate
/// order, no more than `limit` run at once, and several really overlap.
#[tokio::test]
async fn bounded_verification_overlaps_tasks_and_keeps_order() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let (running, peak) = (AtomicUsize::new(0), AtomicUsize::new(0));
    let done = ordered_concurrently((0..12u64).collect(), 4, |item| {
        let (running, peak) = (&running, &peak);
        async move {
            let now = running.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(now, Ordering::SeqCst);
            // Later items finish first, so order comes from the combinator.
            tokio::time::sleep(std::time::Duration::from_millis(40 - item * 3)).await;
            running.fetch_sub(1, Ordering::SeqCst);
            Ok(item)
        }
    })
    .await
    .expect("all tasks succeed");
    assert_eq!(done, (0..12).collect::<Vec<_>>());
    assert_eq!(peak.load(Ordering::SeqCst), 4);
}

#[tokio::test]
async fn a_failed_verification_stops_the_rest() {
    let started = std::sync::atomic::AtomicUsize::new(0);
    let result = ordered_concurrently((0..20u32).collect(), 2, |item| {
        let started = &started;
        async move {
            started.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if item == 1 {
                Err(LspFailure::invalid_query("boom"))
            } else {
                tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                Ok(item)
            }
        }
    })
    .await;
    assert!(result.is_err());
    assert!(started.load(std::sync::atomic::Ordering::SeqCst) < 20);
}
