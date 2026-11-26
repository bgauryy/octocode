use super::*;
use std::time::Duration;

const SIZE: u64 = 100;

fn at(seconds: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
}

/// A checkout of `SIZE` bytes whose last use is `used`.
fn checkout(home: &Path, name: &str, used: u64) -> PathBuf {
    let dir = storage::clones_root(home).join("o").join("r").join(name);
    fs::create_dir_all(dir.join(".git")).unwrap();
    fs::write(dir.join("file"), vec![0u8; SIZE as usize]).unwrap();
    storage::record_use(&dir, at(used)).unwrap();
    dir
}

fn survivors(home: &Path) -> Vec<String> {
    let mut names = entries(home)
        .iter()
        .map(|entry| {
            entry
                .dir
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>();
    names.sort();
    names
}

fn home() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

#[test]
fn evicts_oldest_use_first_until_the_total_fits() {
    let home = home();
    checkout(home.path(), "b", 20);
    checkout(home.path(), "a", 10);
    checkout(home.path(), "c", 30);
    checkout(home.path(), "d", 40);
    let freed = evict_to_cap(home.path(), 2 * SIZE, |_| true);
    assert_eq!(freed, 2 * SIZE);
    assert_eq!(survivors(home.path()), ["c", "d"]);
}

#[test]
fn a_total_within_the_cap_removes_nothing() {
    let home = home();
    checkout(home.path(), "a", 10);
    checkout(home.path(), "b", 20);
    assert_eq!(evict_to_cap(home.path(), 2 * SIZE, |_| true), 0);
    assert_eq!(survivors(home.path()), ["a", "b"]);
}

#[test]
fn a_locked_checkout_survives_even_when_oldest() {
    let home = home();
    let oldest = checkout(home.path(), "a", 10);
    checkout(home.path(), "b", 20);
    checkout(home.path(), "c", 30);
    let _held = storage::try_lock_checkout(home.path(), &oldest)
        .unwrap()
        .expect("unlocked");
    let freed = evict_to_cap(home.path(), SIZE, |_| true);
    assert_eq!(freed, 2 * SIZE);
    assert_eq!(survivors(home.path()), ["a"]);
}

#[test]
fn reuse_refreshes_recency() {
    let home = home();
    let reused = checkout(home.path(), "a", 10);
    checkout(home.path(), "b", 20);
    checkout(home.path(), "c", 30);
    storage::record_use(&reused, at(99)).unwrap();
    evict_to_cap(home.path(), 2 * SIZE, |_| true);
    assert_eq!(survivors(home.path()), ["a", "c"]);
}

#[test]
fn cap_zero_never_evicts() {
    let home = home();
    checkout(home.path(), "a", 10);
    checkout(home.path(), "b", 20);
    assert_eq!(evict_to_cap(home.path(), 0, |_| true), 0);
    assert_eq!(survivors(home.path()), ["a", "b"]);
}

#[test]
fn a_checkout_that_is_not_removable_stays_and_the_next_oldest_goes() {
    let home = home();
    let dirty = checkout(home.path(), "a", 10);
    checkout(home.path(), "b", 20);
    checkout(home.path(), "c", 30);
    evict_to_cap(home.path(), 2 * SIZE, |dir| dir != dirty);
    assert_eq!(survivors(home.path()), ["a", "c"]);
}

#[test]
fn the_commit_is_the_name_before_the_scope_key() {
    let sha = "c".repeat(40);
    let dir = Path::new("/h/clones/o/r").join(format!("{sha}__full__sp_ab__host_cd"));
    assert_eq!(commit_of(&dir), Some(sha.as_str()));
    assert_eq!(commit_of(Path::new("/h/clones/o/r/other")), None);
}

/// A checkout's size recorded when it was published is read instead of
/// walking its files, so a cap pass reads one small file per checkout; one
/// with no record is walked.
#[test]
fn a_recorded_size_decides_without_walking_the_checkout() {
    let home = home();
    let a = checkout(home.path(), "a", 10);
    checkout(home.path(), "b", 20);
    // `a` holds SIZE bytes on disk but its publish recorded 1 byte: walked,
    // the two (2 * SIZE) pass the cap and `a` would go; recorded, they fit.
    storage::record_size(&a, 1).unwrap();
    assert_eq!(evict_to_cap(home.path(), SIZE + 1, |_| true), 0);
    assert_eq!(survivors(home.path()), ["a", "b"]);
    assert_eq!(storage::recorded_size(&a), Some(1));
}

/// A checkout an earlier build published has no recorded size: the first
/// cap pass walks it once and records it, so the next pass reads it.
#[test]
fn an_unrecorded_checkout_is_walked_once_and_recorded() {
    let home = home();
    let old = checkout(home.path(), "old", 10);
    assert_eq!(storage::recorded_size(&old), None);
    assert_eq!(evict_to_cap(home.path(), 10 * SIZE, |_| true), 0);
    assert_eq!(storage::recorded_size(&old), Some(SIZE));
}
