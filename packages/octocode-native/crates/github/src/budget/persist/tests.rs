//! Merging, viewing and writing the persisted limiter facts.
use super::super::key_state::Bucket;
use super::super::*;
use super::KeyFacts;
use reqwest::header::HeaderMap;

/// A search quota GitHub reported is mirrored at once: the next process
/// (a fresh registry on the same file) paces its first search from it
/// instead of starting unpaced.
#[test]
fn a_reported_search_quota_reaches_the_next_process() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = LimiterKey::new("ghe.example", Some("tok"));
    let reset = now_ms() / 1000 + 60;
    let config = ExecutorConfig::octokit();
    {
        let budget = GitHubBudget::with_config(config.clone());
        let state = budget.key_state(&key, Some(dir.path()));
        let mut headers = HeaderMap::new();
        let reset = reset.to_string();
        for (name, value) in [
            ("x-ratelimit-limit", "10"),
            ("x-ratelimit-remaining", "9"),
            ("x-ratelimit-reset", reset.as_str()),
            ("x-ratelimit-resource", "search"),
        ] {
            headers.insert(name, value.parse().expect("header"));
        }
        state.observe(&headers, "search");
    }
    let file = dir.path().join(key.file_name());
    let raw: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&file).expect("state file")).expect("json");
    assert!(
        raw["buckets"]["search"].get("limit").is_none(),
        "the unused reported limit must not be persisted: {raw}"
    );
    let budget = GitHubBudget::with_config(config.clone());
    let state = budget.key_state(&key, Some(dir.path()));
    state.refresh_from_disk();
    let facts = state.facts();
    let reported = Bucket {
        remaining: 9,
        reset,
    };
    assert_eq!(facts.buckets.get("search"), Some(&reported));
    let now = now_ms();
    assert_eq!(
        config.spacing_ms(Group::Search, &facts, now, 1),
        reported.spacing_ms(&config, now, 1)
    );
    assert_eq!(reported.spacing_ms(&config, now, 1), 0);
    assert!(reported.spacing_ms(&config, now, 10) > 0);
}

/// Merging is idempotent and keeps the most restrictive view of a bucket:
/// the later window, the lower count within one window, an exhausted one first;
/// an ended window is dropped.
#[test]
fn bucket_merge_is_idempotent_and_keeps_the_most_restrictive_view() {
    let now = 1_000_000_000;
    let reset = now / 1000 + 60;
    let bucket = |remaining, reset| Bucket { remaining, reset };
    let mut facts = KeyFacts::default();
    facts.buckets.insert("code_search".into(), bucket(7, reset));
    let peer = KeyFacts {
        buckets: [
            ("code_search".to_owned(), bucket(4, reset)),
            ("search".to_owned(), bucket(9, now / 1000 - 1)),
        ]
        .into(),
        ..KeyFacts::default()
    };
    for _ in 0..3 {
        facts.merge(peer.clone(), now);
        assert_eq!(facts.buckets.get("code_search"), Some(&bucket(4, reset)));
        assert!(!facts.buckets.contains_key("search"), "an ended window");
        facts.merge(facts.clone(), now);
        assert_eq!(facts.buckets.get("code_search"), Some(&bucket(4, reset)));
    }
    facts.merge(
        KeyFacts {
            buckets: [("code_search".to_owned(), bucket(9, reset))].into(),
            ..KeyFacts::default()
        },
        now,
    );
    assert_eq!(facts.buckets.get("code_search"), Some(&bucket(4, reset)));
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
    let budgets = (0..8).map(|_| GitHubBudget::relaxed()).collect::<Vec<_>>();
    let states = budgets
        .iter()
        .map(|budget| budget.key_state(&key, Some(dir.path())))
        .collect::<Vec<_>>();
    std::thread::scope(|scope| {
        for (writer, state) in states.iter().enumerate() {
            scope.spawn(move || {
                for round in 0..25 {
                    state.persist_with(|view| {
                        view.last_start_ms.insert(format!("w{writer}-{round}"), now);
                    });
                }
            });
        }
    });
    // A write that met a busy lock was deferred in its writer's memory; the
    // next write, the lock free, carries it.
    for state in &states {
        state.persist_with(|_| {});
    }
    let disk: KeyFacts =
        serde_json::from_slice(&std::fs::read(dir.path().join(key.file_name())).expect("file"))
            .expect("json");
    assert_eq!(disk.last_start_ms.len(), 8 * 25);
}

/// A write that cannot take the peer lock in time is deferred, not made
/// unlocked: the peer's write while it holds the lock keeps every fact, and
/// the deferred fact reaches the file with the next write.
#[test]
fn a_write_past_a_held_lock_defers_instead_of_dropping_the_peers_facts() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = LimiterKey::new("ghe.example", Some("tok"));
    let file = dir.path().join(key.file_name());
    let budget = GitHubBudget::relaxed();
    let state = budget.key_state(&key, Some(dir.path()));
    let now = now_ms();
    let facts = |names: &[&str]| {
        let mut facts = KeyFacts::default();
        for name in names {
            facts.last_start_ms.insert((*name).to_owned(), now);
        }
        serde_json::to_vec(&facts).expect("json")
    };
    let mut sidecar = file.as_os_str().to_os_string();
    sidecar.push(".lock");
    let peer = octocode_io::lock::lock_within(
        octocode_io::lock::open_lock_file(std::path::Path::new(&sidecar)).expect("sidecar"),
        std::time::Duration::ZERO,
        octocode_io::lock::LOCK_POLL,
    )
    .expect("the peer's lock");
    std::fs::write(&file, facts(&["peer-a"])).expect("peer write");
    state.persist_with(|view| {
        view.last_start_ms.insert("mine".into(), now);
    });
    // The peer finishes its read-merge-write from what it read.
    std::fs::write(&file, facts(&["peer-a", "peer-b"])).expect("peer write");
    drop(peer);
    state.persist_with(|_| {});
    let disk: KeyFacts =
        serde_json::from_slice(&std::fs::read(&file).expect("file")).expect("json");
    let mut names = disk
        .last_start_ms
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    names.sort_unstable();
    assert_eq!(names, ["mine", "peer-a", "peer-b"]);
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
