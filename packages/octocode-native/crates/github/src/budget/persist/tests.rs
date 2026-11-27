//! Merging, viewing and writing the persisted limiter facts.
use super::super::key_state::Bucket;
use super::super::*;
use super::KeyFacts;
use reqwest::header::HeaderMap;
use std::path::{Path, PathBuf};

/// Which thread touched which state file, for the tests that keep this file
/// IO off the async workers.
static IO_THREADS: std::sync::Mutex<Vec<(PathBuf, std::thread::ThreadId)>> =
    std::sync::Mutex::new(Vec::new());

/// Records that this thread touched `path`'s state file.
pub(super) fn record_io(path: &Path) {
    IO_THREADS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push((path.to_path_buf(), std::thread::current().id()));
}

/// The threads that read or wrote `path`'s state file.
fn io_threads(path: &Path) -> Vec<std::thread::ThreadId> {
    IO_THREADS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .filter(|(seen, _)| seen == path)
        .map(|(_, thread)| *thread)
        .collect()
}

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

/// A shared reservation reads and writes the state file on the blocking
/// pool, never on the async worker that polls the request (here the
/// current-thread runtime's only worker, the test thread).
#[tokio::test]
async fn a_shared_reservation_does_its_file_io_off_the_async_worker() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = LimiterKey::new("ghe.example", Some("tok"));
    let budget = GitHubBudget::relaxed();
    let state = budget.key_state(&key, Some(dir.path()));
    let file = dir.path().join(key.file_name());
    let worker = std::thread::current().id();
    state
        .reserve_shared(
            Instant::now() + Duration::from_secs(5),
            &octocode_io::cancellation::Cancellation::new(),
            &crate::budget::ClientWaits::default(),
            |_| (),
        )
        .await
        .expect("reserved");
    let io = io_threads(&file);
    assert_eq!(io.len(), 2, "one read and one write of {}", file.display());
    assert!(
        io.iter().all(|thread| *thread != worker),
        "state file IO ran on the async worker"
    );
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

/// An owner's listing route reaches the next process; a route forgotten
/// after a 404 leaves the file, while a newer one a peer wrote stays.
#[tokio::test]
async fn owner_routes_reach_the_next_process_and_a_forgotten_one_leaves_the_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = LimiterKey::new("ghe.example", Some("tok"));
    let earlier = GitHubBudget::relaxed().key_state(&key, Some(dir.path()));
    earlier.learn_owner_route("Me", "viewer").await;
    earlier.learn_owner_route("acme", "organization").await;
    let next = GitHubBudget::relaxed().key_state(&key, Some(dir.path()));
    assert_eq!(next.owner_route("me").as_deref(), Some("viewer"));
    assert_eq!(next.owner_route("acme").as_deref(), Some("organization"));

    next.forget_owner_route("acme").await;
    let after = GitHubBudget::relaxed().key_state(&key, Some(dir.path()));
    assert_eq!(after.owner_route("acme"), None);
    assert_eq!(after.owner_route("me").as_deref(), Some("viewer"));

    // A peer learned a new route before this process forgot the old one.
    let peer = GitHubBudget::relaxed().key_state(&key, Some(dir.path()));
    peer.learn_owner_route("me", "user").await;
    earlier.forget_owner_route("me").await;
    let last = GitHubBudget::relaxed().key_state(&key, Some(dir.path()));
    assert_eq!(last.owner_route("me").as_deref(), Some("user"));
}

/// A learned route is on disk when the learning request ends, even if the
/// process's runtime shuts down at once (a CLI call). A blocking write handed
/// to an idle pool thread and still queued at shutdown is dropped (tokio's
/// shutdown drain runs no queued blocking task), so this write is awaited.
#[test]
fn a_learned_route_survives_an_immediate_runtime_shutdown() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = LimiterKey::new("ghe.example", Some("tok"));
    for round in 0..50 {
        let owner = format!("owner{round}");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let state = GitHubBudget::relaxed().key_state(&key, Some(dir.path()));
        runtime.block_on(async {
            // An earlier file read leaves an idle blocking thread, as a
            // request's own reads do before the route is learned.
            tokio::task::spawn_blocking(|| ())
                .await
                .expect("warm the pool");
            state.learn_owner_route(&owner, "organization").await;
        });
        drop(runtime);
        let next = GitHubBudget::relaxed().key_state(&key, Some(dir.path()));
        assert_eq!(
            next.owner_route(&owner).as_deref(),
            Some("organization"),
            "round {round}: the route was lost at shutdown"
        );
    }
}

/// Every fact written without awaiting reaches the next process when the
/// process that wrote it ends right after its call, as a CLI call does: the
/// runtime's close flushes the detached writes before the runtime stops.
/// Each round is a fresh single-threaded runtime whose blocking pool holds an
/// idle thread (a request's own earlier reads leave one): a write handed to
/// it and still queued at shutdown is dropped unless the flush awaited it.
#[test]
fn every_detached_write_reaches_the_next_process_after_an_immediate_exit() {
    type Write = fn(&Arc<KeyState>, u64);
    type Holds = fn(&KeyFacts, u64) -> bool;
    let writes: [(&str, Write, Holds); 5] = [
        (
            "exhaust",
            |state, round| state.exhaust(&format!("core{round}"), now_ms() / 1000 + 600),
            |facts, round| {
                facts
                    .buckets
                    .get(&format!("core{round}"))
                    .is_some_and(|bucket| bucket.remaining == 0)
            },
        ),
        (
            "cool_down",
            |state, round| state.cool_down(now_ms() + 600_000 + round * 1000),
            |facts, round| facts.cooldown_until_ms >= now_ms() + 590_000 + round * 1000,
        ),
        (
            "observed quota",
            |state, round| {
                let mut headers = HeaderMap::new();
                let reset = (now_ms() / 1000 + 600).to_string();
                let resource = format!("graphql{round}");
                for (name, value) in [
                    ("x-ratelimit-remaining", "0"),
                    ("x-ratelimit-reset", reset.as_str()),
                    ("x-ratelimit-resource", resource.as_str()),
                ] {
                    headers.insert(name, value.parse().expect("header"));
                }
                state.observe(&headers, "core");
            },
            |facts, round| {
                facts
                    .buckets
                    .get(&format!("graphql{round}"))
                    .is_some_and(|bucket| bucket.remaining == 0)
            },
        ),
        (
            "pull request max-age",
            |state, round| state.learn_pull_request_max_age(10_000 - round),
            |facts, round| facts.pull_request_max_age == Some(10_000 - round),
        ),
        (
            "handed-back start",
            |state, round| {
                let start = start_of(round);
                state.release_reservation_detached((Group::Search, start, None, true));
            },
            |facts, round| facts.last_start_ms.get("search") != Some(&start_of(round)),
        ),
    ];
    for (kind, write, holds) in writes {
        let dir = tempfile::tempdir().expect("tempdir");
        let key = LimiterKey::new("ghe.example", Some("tok"));
        for round in 0..50 {
            if kind == "handed-back start" {
                // The reservation is on disk before the call hands it back.
                let reserving = GitHubBudget::relaxed().key_state(&key, Some(dir.path()));
                reserving
                    .facts()
                    .last_start_ms
                    .insert("search".to_owned(), start_of(round));
                reserving.persist_with(|_| {});
            }
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime");
            let state = GitHubBudget::relaxed().key_state(&key, Some(dir.path()));
            runtime.block_on(async {
                tokio::task::spawn_blocking(|| ())
                    .await
                    .expect("warm the pool");
                write(&state, round);
                // The CLI's `ToolRuntime::close`.
                GitHubBudget::flush_detached_writes().await;
            });
            drop(runtime);
            let next = GitHubBudget::relaxed().key_state(&key, Some(dir.path()));
            assert!(
                holds(&next.facts(), round),
                "{kind}, round {round}: the write was lost at exit"
            );
        }
    }
}

/// A start stamp per round, one base for the whole test, inside the window
/// the mirror keeps.
fn start_of(round: u64) -> u64 {
    static BASE: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    BASE.get_or_init(|| now_ms() + 5_000) + round
}

/// A current-thread runtime whose blocking pool has one thread.
fn one_blocking_thread() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .max_blocking_threads(1)
        .enable_all()
        .build()
        .expect("runtime")
}

/// Hold the runtime's one blocking thread until the returned sender sends:
/// a blocking write spawned meanwhile stays queued, however long the test
/// takes (no lock wait or clock involved).
fn hold_the_blocking_pool() -> std::sync::mpsc::Sender<()> {
    let (release, released) = std::sync::mpsc::channel::<()>();
    let (started, running) = std::sync::mpsc::channel::<()>();
    drop(tokio::task::spawn_blocking(move || {
        let _ = started.send(());
        let _ = released.recv();
    }));
    running.recv().expect("the blocker runs");
    release
}

/// An awaited write whose request is dropped mid-write (an
/// MCP cancel, Ctrl-C) keeps running and stays tracked, so the flush a
/// process runs before it exits waits for it and the next process finds
/// what it wrote. The check is of this test's own write (its key state),
/// whatever else the process writes.
#[test]
fn an_awaited_write_of_a_cancelled_request_is_still_flushed() {
    use futures_util::FutureExt as _;
    let dir = tempfile::tempdir().expect("tempdir");
    let key = LimiterKey::new("ghe.example", Some("cancelled"));
    one_blocking_thread().block_on(async {
        let state = GitHubBudget::relaxed().key_state(&key, Some(dir.path()));
        let release = hold_the_blocking_pool();
        {
            let learn = state.learn_owner_route("acme", "organization");
            tokio::pin!(learn);
            assert!(
                learn.as_mut().now_or_never().is_none(),
                "the write is queued behind the held pool"
            );
            // The request is dropped here, its write still queued.
        }
        assert!(
            state.has_running_write(),
            "the cancelled request's write is tracked"
        );
        release.send(()).expect("release the pool");
        GitHubBudget::flush_detached_writes().await;
        assert!(!state.has_running_write(), "the flush waited for it");
        let next = GitHubBudget::relaxed().key_state(&key, Some(dir.path()));
        assert_eq!(next.owner_route("acme").as_deref(), Some("organization"));
    });
}

/// Handing back a shared reservation writes through the one
/// tracked mechanism (`persist_awaited`): a release whose request is dropped
/// is still flushed, and the next process sees the start handed back.
#[test]
fn a_released_reservation_is_a_tracked_write() {
    use futures_util::FutureExt as _;
    let dir = tempfile::tempdir().expect("tempdir");
    let key = LimiterKey::new("ghe.example", Some("released"));
    let start = now_ms();
    one_blocking_thread().block_on(async {
        let state = GitHubBudget::relaxed().key_state(&key, Some(dir.path()));
        state
            .facts()
            .last_start_ms
            .insert(Group::Search.name().to_owned(), start);
        state
            .persist_awaited(move |view| {
                view.last_start_ms
                    .insert(Group::Search.name().to_owned(), start);
            })
            .await;
        let reserved = GitHubBudget::relaxed().key_state(&key, Some(dir.path()));
        assert_eq!(
            reserved.facts().last_start_ms.get(Group::Search.name()),
            Some(&start),
            "the reservation is on disk"
        );
        let release = hold_the_blocking_pool();
        {
            let hand_back = state.release_reservation(Group::Search, start, None, true);
            tokio::pin!(hand_back);
            assert!(
                hand_back.as_mut().now_or_never().is_none(),
                "the write is queued behind the held pool"
            );
        }
        assert!(
            state.has_running_write(),
            "the hand-back's write is tracked"
        );
        release.send(()).expect("release the pool");
        GitHubBudget::flush_detached_writes().await;
        let next = GitHubBudget::relaxed().key_state(&key, Some(dir.path()));
        assert_eq!(
            next.facts().last_start_ms.get(Group::Search.name()),
            None,
            "the start was handed back on disk"
        );
    });
}

/// A peer's cooldown this process first meets inside its own write (here
/// of a spent bucket) reaches its memory too: the file stamp that write
/// records hides no peer fact from the next refresh, so the next core
/// admission is blocked without waiting for another peer write.
#[test]
fn a_peer_cooldown_merged_by_a_write_blocks_this_process() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = LimiterKey::new("ghe.example", Some("tok"));
    let config = ExecutorConfig::relaxed();
    let state = GitHubBudget::relaxed().key_state(&key, Some(dir.path()));
    let peer = GitHubBudget::relaxed().key_state(&key, Some(dir.path()));
    let until = now_ms() + 30_000;
    peer.cool_down(until);
    state.exhaust("search", now_ms() / 1000 + 60);
    state.refresh_from_disk();
    assert!(
        state.blocked("core", &config).is_some(),
        "the peer's cooldown does not hold this process"
    );
    assert_eq!(state.facts().cooldown_until_ms, until);
}

/// A start handed back stays handed back in memory once the write that
/// takes it off the file has merged that file, which still held it.
#[tokio::test]
async fn a_handed_back_start_is_not_restored_by_the_merged_write() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = LimiterKey::new("ghe.example", Some("handed-back"));
    let start = now_ms() + 5_000;
    let state = GitHubBudget::relaxed().key_state(&key, Some(dir.path()));
    state
        .facts()
        .last_start_ms
        .insert(Group::Search.name().to_owned(), start);
    state.persist_with(|_| {});
    state
        .release_reservation(Group::Search, start, None, true)
        .await;
    assert_eq!(
        state.facts().last_start_ms.get(Group::Search.name()),
        None,
        "memory took the start back from the file"
    );
    let next = GitHubBudget::relaxed().key_state(&key, Some(dir.path()));
    assert_eq!(next.facts().last_start_ms.get(Group::Search.name()), None);
}

/// A cooldown or bucket reset beyond GitHub's own rate-limit window comes
/// from a bogus header or a clock that jumped. The process that received it
/// still honors it, but it is not mirrored, and one a file already holds
/// (a peer's, an older build's) is not merged: no other process is blocked.
#[test]
fn a_limit_beyond_the_github_window_holds_only_the_process_that_received_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = LimiterKey::new("ghe.example", Some("tok"));
    let config = ExecutorConfig::relaxed();
    let file = dir.path().join(key.file_name());
    let far = now_ms() + 10 * 24 * 3_600_000;
    let held = KeyFacts {
        cooldown_until_ms: far,
        buckets: [(
            "core".to_owned(),
            Bucket {
                remaining: 0,
                reset: far / 1000,
            },
        )]
        .into(),
        ..KeyFacts::default()
    };
    std::fs::write(&file, serde_json::to_vec(&held).expect("json")).expect("peer write");
    let fresh = GitHubBudget::relaxed().key_state(&key, Some(dir.path()));
    assert_eq!(fresh.blocked("core", &config), None);

    let other = tempfile::tempdir().expect("tempdir");
    let receiving = GitHubBudget::relaxed().key_state(&key, Some(other.path()));
    receiving.cool_down(far);
    receiving.exhaust("graphql", far / 1000);
    assert_eq!(
        receiving
            .blocked("core", &config)
            .map(|block| block.until_ms),
        Some(far)
    );
    let peer = GitHubBudget::relaxed().key_state(&key, Some(other.path()));
    assert_eq!(peer.blocked("core", &config), None);
    assert_eq!(peer.blocked("graphql", &config), None);
}
