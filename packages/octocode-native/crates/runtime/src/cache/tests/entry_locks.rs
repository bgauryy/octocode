//! A lock file per entry goes when its entry has: a scan removes it unheld, and
//! a locker checks what it locked, so nobody holds two different "locks" of one
//! entry and no update is lost.
use super::*;
use octocode_io::lock::{LOCK_POLL, lock_path_within, open_lock_file};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

const WAIT: Duration = Duration::from_secs(30);

fn locks(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                name.starts_with(crate::cache::disk::ENTRY_LOCK_PREFIX) && name.ends_with(".lock")
            })
        })
        .collect()
}

/// What `call` says in its result about the store at `dir`, named by its
/// folder alone: no absolute path reaches the result.
fn store_notices(call: &octocode_io::notice::ResultNotes, dir: &Path) -> Vec<String> {
    let folder = dir.file_name().unwrap().to_string_lossy().into_owned();
    let said = call.take();
    for note in &said {
        assert!(!note.contains(&dir.display().to_string()), "{note}");
    }
    said.into_iter()
        .filter(|notice| notice.contains(&format!("store {folder} ")))
        .collect()
}

fn store(dir: &Path) -> Store<u64> {
    Store::new(CacheConfig::derived(), Some(dir.into()))
}

/// Scan until a whole one finds nothing to remove.
fn drain(store: &Store<u64>) {
    for _ in 0..100 {
        store.disk.as_ref().unwrap().prune(u64::MAX);
    }
}

#[cfg(unix)]
#[test]
fn a_lock_file_goes_with_its_entry_and_only_then() {
    let dir = tempfile::tempdir().unwrap();
    let store = store(dir.path());
    let (live, gone, held) = (key("live", "x"), key("gone", "x"), key("held", "x"));
    store.put_confirmed(live.clone(), 1, 8, volatile(600));
    for each in [&live, &gone, &held] {
        drop(open_lock_file(&store.entry_lock(each).unwrap()).unwrap());
    }
    // A lock file of a layout that named it otherwise holds no entry either.
    let older = dir
        .path()
        .join(format!("assessment-{}.lock", "c".repeat(64)));
    drop(open_lock_file(&older).unwrap());
    let the_store_lock = dir.path().join("assessment.lock");
    drop(open_lock_file(&the_store_lock).unwrap());
    let holding =
        lock_path_within(&store.entry_lock(&held).unwrap(), WAIT, LOCK_POLL).expect("held");

    drain(&store);

    assert!(store.entry_lock(&live).unwrap().exists(), "its entry lives");
    assert!(
        store.entry_lock(&held).unwrap().exists(),
        "somebody holds it"
    );
    assert!(the_store_lock.exists(), "it names no entry's lock");
    assert!(!store.entry_lock(&gone).unwrap().exists());
    assert!(!older.exists());
    assert!(holding.still_at(&store.entry_lock(&held).unwrap()));
    drop(holding);
    drain(&store);
    assert!(!store.entry_lock(&held).unwrap().exists());
    assert_eq!(locks(dir.path()).len(), 1, "the live entry's alone");
}

/// A locker that opened the file before a sweep removed it locks nothing anyone
/// else can see: it finds that out, and goes again on what the path names now.
#[cfg(unix)]
#[test]
fn a_lock_on_a_removed_file_is_seen_and_taken_again() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("assessment-a.lock");
    let opened = open_lock_file(&path).unwrap();
    assert!(octocode_io::lock::remove_unheld(&path, || true));
    assert!(!path.exists());
    let orphan = octocode_io::lock::lock_within(opened, WAIT, LOCK_POLL).expect("orphan");
    assert!(!orphan.still_at(&path), "the path names nothing now");
    let taken = lock_path_within(&path, WAIT, LOCK_POLL).expect("taken");
    assert!(taken.still_at(&path));
    assert!(
        !octocode_io::lock::remove_unheld(&path, || true),
        "held: it stays"
    );
    assert!(path.exists());
    drop(taken);
    assert!(
        !octocode_io::lock::remove_unheld(&path, || false),
        "still needed"
    );
    assert!(path.exists());
}

fn hold_the_store(dir: &Path) -> octocode_io::lock::LockedFile {
    let lock = open_lock_file(&dir.join(".capacity.lock")).unwrap();
    octocode_io::lock::lock_within(lock, WAIT, LOCK_POLL).expect("free")
}

/// A scan of a large store holds the store's lock for as long as it takes. A write
/// its call depends on waits past what a reuse write would, up to the request's
/// deadline.
#[test]
fn a_write_a_call_depends_on_waits_out_a_scan_that_holds_the_store() {
    let dir = tempfile::tempdir().unwrap();
    let store = store(dir.path());
    let scan = hold_the_store(dir.path());
    let deadline = std::time::Instant::now() + WAIT;
    std::thread::scope(|scope| {
        let write = scope.spawn(|| {
            store
                .put_until(deadline, key("waits", "x"), 1, 8, volatile(600))
                .1
        });
        std::thread::sleep(crate::cache::disk::REUSE_LOCK_WAIT + Duration::from_millis(500));
        drop(scan);
        assert!(write.join().expect("write"), "the write was admitted");
    });
}

/// Past the request's deadline the write is refused, and the call learns it.
#[test]
fn a_write_refused_at_its_deadline_reports_it_and_is_said() {
    let dir = tempfile::tempdir().unwrap();
    let store = store(dir.path());
    let _scan = hold_the_store(dir.path());
    let call = octocode_io::notice::ResultNotes::default();
    let _in_call = call.enter();
    let (_, kept) = store.put_until(
        std::time::Instant::now() + Duration::from_millis(50),
        key("late", "x"),
        1,
        8,
        volatile(600),
    );
    assert!(!kept);
    assert!(store.get(&key("late", "x")).is_none());
    let notices = store_notices(&call, dir.path());
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert!(notices[0].contains("was not saved"));
}

/// B75 F1: a write waits on a held store under its request's stop flag, so a
/// cancel ends the wait at once, whatever the deadline, instead of leaving the
/// cancelled request joined to a thread only the holder releases.
#[test]
fn a_cancel_ends_a_write_waiting_on_a_held_store() {
    let dir = tempfile::tempdir().unwrap();
    let store = store(dir.path());
    let scan = hold_the_store(dir.path());
    let stopped = std::sync::Arc::new(AtomicBool::new(false));
    let (done, ended) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let call = octocode_io::notice::ResultNotes::default();
            let in_call = call.enter();
            let kept = octocode_engine::deep_stack::while_stoppable(&stopped, || {
                let deadline = std::time::Instant::now() + crate::execution::MAX_TIMEOUT;
                store
                    .put_until(deadline, key("cancelled", "x"), 1, 8, volatile(600))
                    .1
            });
            drop(in_call);
            // B75 R1c: a cancelled call is not told its store was busy.
            assert_eq!(
                store_notices(&call, dir.path()),
                Vec::<String>::new(),
                "no busy notice"
            );
            let _ = done.send(kept);
        });
        std::thread::sleep(Duration::from_millis(200));
        stopped.store(true, Ordering::Relaxed);
        let answer = ended.recv_timeout(Duration::from_secs(1));
        // Release the holder so a wait the cancel missed still ends the test.
        drop(scan);
        assert_eq!(
            answer,
            Ok(false),
            "the cancelled write ends within 1 s, not kept"
        );
    });
}

/// A write only later reuse reads gives up sooner, and is said too: nobody would
/// otherwise learn why the next call reads afresh.
#[test]
fn a_reuse_write_refused_for_a_busy_store_is_said() {
    let dir = tempfile::tempdir().unwrap();
    let store = store(dir.path());
    let _scan = hold_the_store(dir.path());
    let call = octocode_io::notice::ResultNotes::default();
    let _in_call = call.enter();
    assert!(
        !store
            .put_confirmed(key("reuse", "x"), 1, 8, volatile(600))
            .1
    );
    let notices = store_notices(&call, dir.path());
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert!(notices[0].contains("was not saved"));
}

/// Two writers and a sweep that never stops. Each writer's turn on a key with no
/// entry is what a sweep may remove from under it. One holder at a time, every
/// update kept, and only the live entries' lock files left.
#[cfg(unix)]
#[test]
fn two_writers_and_a_sweep_hold_one_lock_and_lose_nothing() {
    const ROUNDS: usize = 150;
    const TURNS: u64 = 4;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();
    // One count per lock: another entry's lock is another holder's to take.
    let holders = (0..2 * ROUNDS)
        .map(|_| AtomicUsize::new(0))
        .collect::<Vec<_>>();
    let (most, done) = (AtomicUsize::new(0), AtomicBool::new(false));
    let writer = || {
        let store = store(path);
        for round in 0..ROUNDS {
            // No entry here, ever: nothing but the lock asks the sweep to wait.
            let bare = store
                .entry_lock(&key(&format!("bare{round}"), "x"))
                .unwrap();
            let counted = key(&format!("counted{round}"), "x");
            let file = store.entry_lock(&counted).unwrap();
            for _ in 0..TURNS {
                for (at, lock) in [&bare, &file].into_iter().enumerate() {
                    let held = lock_path_within(lock, WAIT, LOCK_POLL).expect("lock");
                    let holding = &holders[2 * round + at];
                    most.fetch_max(holding.fetch_add(1, Ordering::SeqCst) + 1, Ordering::SeqCst);
                    if lock == &file {
                        let seen = store.get(&counted).map_or(0, |kept| *kept.value);
                        std::thread::yield_now();
                        let writing = std::time::Instant::now();
                        let stored = store.put_until(
                            writing + WAIT,
                            counted.clone(),
                            seen + 1,
                            8,
                            volatile(600),
                        );
                        assert!(
                            stored.1,
                            "the write was admitted, refused after {:?}",
                            writing.elapsed()
                        );
                    } else {
                        std::thread::yield_now();
                    }
                    assert!(held.still_at(lock), "nothing removes a held lock");
                    holding.fetch_sub(1, Ordering::SeqCst);
                    drop(held);
                }
            }
        }
    };
    std::thread::scope(|scope| {
        let sweep = scope.spawn(|| {
            let store = store(path);
            while !done.load(Ordering::SeqCst) {
                store.disk.as_ref().unwrap().prune(u64::MAX);
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        let writers = [scope.spawn(writer), scope.spawn(writer)];
        let results = writers.map(std::thread::ScopedJoinHandle::join);
        // Before any panic: the scope waits for the sweep, which ends only on this.
        done.store(true, Ordering::SeqCst);
        sweep.join().expect("sweep");
        for result in results {
            result.expect("writer");
        }
    });
    assert_eq!(most.load(Ordering::SeqCst), 1, "one holder at a time");
    let store = store(path);
    for round in 0..ROUNDS {
        let kept = store
            .get(&key(&format!("counted{round}"), "x"))
            .expect("kept");
        assert_eq!(*kept.value, 2 * TURNS, "round {round}: every update");
    }
    drain(&store);
    let left = locks(path).len();
    assert!(left <= ROUNDS, "{left} lock files for {ROUNDS} entries");
}
