//! Merging, viewing and writing the persisted limiter facts.
use super::super::slots::{SlotPlan, Subject};
use super::super::*;
use super::KeyFacts;
use reqwest::header::HeaderMap;

#[test]
fn code_search_window_merge_preserves_same_millisecond_reservations() {
    let now = 1_000_000;
    let stamp = now - 1;
    for (local, peer, count) in [
        (vec![stamp, stamp], vec![stamp, stamp], 2),
        (vec![stamp, stamp], vec![stamp], 2),
        (vec![stamp], vec![stamp, stamp], 2),
        (vec![stamp, stamp], vec![stamp, stamp, stamp], 3),
        (vec![stamp, stamp, stamp], vec![stamp, stamp], 3),
    ] {
        let mut facts = KeyFacts {
            code_search_ms: local.into(),
            ..KeyFacts::default()
        };
        facts.merge(
            KeyFacts {
                code_search_ms: peer.into(),
                ..KeyFacts::default()
            },
            now,
        );
        assert_eq!(facts.code_search_ms, vec![stamp; count]);
        let plan = SlotPlan::new(
            Group::CodeSearch,
            &ExecutorConfig {
                code_search_per_minute: 2,
                ..ExecutorConfig::relaxed()
            },
            true,
            Duration::from_secs(1),
            Instant::now() + Duration::from_secs(10),
        );
        assert!(
            plan.refusal(plan.start(&facts, now), Subject::Request)
                .is_some()
        );
    }
}

#[test]
fn code_search_window_merge_is_idempotent_sorted_and_expires_stamps() {
    let now = 1_000_000;
    let expired = now - 60_000;
    let stamp = now - 2;
    let mut facts = KeyFacts {
        code_search_ms: vec![now - 1, stamp, stamp, expired].into(),
        ..KeyFacts::default()
    };
    let peer = KeyFacts {
        code_search_ms: vec![stamp, now - 1, expired - 1].into(),
        ..KeyFacts::default()
    };
    for _ in 0..3 {
        facts.merge(peer.clone(), now);
        assert_eq!(facts.code_search_ms, vec![stamp, stamp, now - 1]);
        facts.merge(facts.clone(), now);
        assert_eq!(facts.code_search_ms, vec![stamp, stamp, now - 1]);
    }
}

/// A rate-limit fact learned on an async worker never waits for the peer
/// lock there; it still reaches the file once the peer lets go.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cooldown_on_an_async_worker_does_not_wait_for_the_peer_lock() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = LimiterKey::new("ghe.example", Some("tok"));
    let budget = GitHubBudget::relaxed();
    let state = budget.key_state(&key, Some(dir.path()));
    let file = dir.path().join(key.file_name());
    let mut lock_name = file.as_os_str().to_os_string();
    lock_name.push(".lock");
    let peer = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(std::path::PathBuf::from(lock_name))
        .expect("lock file");
    let held = octocode_io::lock::lock_within(peer, Duration::ZERO, Duration::from_millis(10))
        .expect("peer holds the lock");
    let until = now_ms() + 60_000;
    let started = Instant::now();
    state.cool_down(until);
    assert!(
        started.elapsed() < Duration::from_millis(200),
        "cool_down blocked the worker for {:?}",
        started.elapsed()
    );
    drop(held);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let written = std::fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<KeyFacts>(&bytes).ok())
            .is_some_and(|facts| facts.cooldown_until_ms >= until);
        if written {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the cooldown never reached the file"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[cfg(unix)]
#[test]
fn the_persisted_budget_file_is_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("tempdir");
    let key = LimiterKey::new("ghe.example", Some("tok"));
    {
        let budget = GitHubBudget::relaxed();
        let state = budget.key_state(&key, Some(dir.path()));
        state.cool_down(now_ms() + 30_000);
    }
    let mode = std::fs::metadata(dir.path().join(key.file_name()))
        .expect("state file")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn disk_state_round_trips_blocking_facts_only() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = LimiterKey::new("ghe.example", Some("tok"));
    let reset = now_ms() / 1000 + 300;
    {
        let budget = GitHubBudget::relaxed();
        let state = budget.key_state(&key, Some(dir.path()));
        let mut headers = HeaderMap::new();
        headers.insert("x-ratelimit-remaining", "42".parse().expect("header"));
        headers.insert(
            "x-ratelimit-reset",
            reset.to_string().parse().expect("header"),
        );
        headers.insert("x-ratelimit-resource", "core".parse().expect("header"));
        state.observe(&headers, "core");
        state.exhaust("search", reset);
        state.cool_down(now_ms() + 30_000);
    }
    let file = dir.path().join(key.file_name());
    let raw: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&file).expect("state file")).expect("json");
    assert!(raw["buckets"].get("core").is_none(), "{raw}");
    assert_eq!(raw["buckets"]["search"]["remaining"], 0);
    // A fresh process (new registry) sees the other process's facts.
    let budget = GitHubBudget::relaxed();
    let state = budget.key_state(&key, Some(dir.path()));
    let search = state.blocked("search", budget.config()).expect("search");
    assert_eq!(search.reset, Some(reset));
    let core = state.blocked("core", budget.config()).expect("cooldown");
    assert_eq!(core.reset, None);
    let leftovers = std::fs::read_dir(dir.path())
        .expect("dir")
        .filter(|entry| {
            entry
                .as_ref()
                .is_ok_and(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
        })
        .count();
    assert_eq!(leftovers, 0);
}

/// Concurrent writers that do not share memory (separate
/// processes; here separate registries) never drop each other's facts.
#[test]
fn concurrent_writers_keep_every_fact() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = LimiterKey::new("ghe.example", Some("tok"));
    let now = now_ms();
    std::thread::scope(|scope| {
        for writer in 0..8 {
            let (dir, key) = (dir.path(), &key);
            scope.spawn(move || {
                let budget = GitHubBudget::relaxed();
                let state = budget.key_state(key, Some(dir));
                for round in 0..25 {
                    state.persist_with(|view| {
                        view.last_start_ms.insert(format!("w{writer}-{round}"), now);
                    });
                }
            });
        }
    });
    let disk: KeyFacts =
        serde_json::from_slice(&std::fs::read(dir.path().join(key.file_name())).expect("file"))
            .expect("json");
    assert_eq!(disk.last_start_ms.len(), 8 * 25);
}

/// A peer's write in the same clock tick as the last one seen is still
/// merged: every write renames a fresh file, so its identity changes even
/// when its mtime does not.
#[test]
fn a_peer_write_with_an_unchanged_mtime_is_still_merged() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = LimiterKey::new("ghe.example", Some("tok"));
    let file = dir.path().join(key.file_name());
    let (peer_budget, budget) = (GitHubBudget::relaxed(), GitHubBudget::relaxed());
    let peer = peer_budget.key_state(&key, Some(dir.path()));
    let first = now_ms() + 30_000;
    peer.cool_down(first);
    let mtime = std::fs::metadata(&file)
        .and_then(|meta| meta.modified())
        .expect("mtime");
    let state = budget.key_state(&key, Some(dir.path()));
    state.refresh_from_disk();
    assert_eq!(state.facts().cooldown_until_ms, first);

    let second = first + 30_000;
    peer.cool_down(second);
    std::fs::File::options()
        .write(true)
        .open(&file)
        .and_then(|file| file.set_modified(mtime))
        .expect("same tick");
    state.refresh_from_disk();
    assert_eq!(state.facts().cooldown_until_ms, second);
}

/// A repeat of the same blocking fact leaves the file in place; a new
/// fact replaces it.
#[cfg(unix)]
#[test]
fn an_unchanged_view_is_not_rewritten() {
    use std::os::unix::fs::MetadataExt;
    let dir = tempfile::tempdir().expect("tempdir");
    let key = LimiterKey::new("ghe.example", Some("tok"));
    let budget = GitHubBudget::relaxed();
    let state = budget.key_state(&key, Some(dir.path()));
    let file = dir.path().join(key.file_name());
    let inode = || std::fs::metadata(&file).expect("state file").ino();
    let reset = now_ms() / 1000 + 300;
    state.exhaust("search", reset);
    let first = inode();
    state.exhaust("search", reset);
    assert_eq!(inode(), first, "same view, no rewrite");
    state.cool_down(now_ms() + 30_000);
    assert_ne!(inode(), first, "a new cooldown is written");
}
