//! Retention of extraction jobs: a job no build holds and none can resume
//! is removed with its lock file; a held job is never touched.
use super::*;

/// An interrupted build's job, left unheld on disk.
fn interrupted(scan: &GraphScan) -> PathBuf {
    let job_dir = job_path(scan.checkpoint.as_ref().unwrap(), &scan.root);
    assert_eq!(
        build(scan, &AfterCommitted(job_dir.clone()))
            .unwrap_err()
            .code,
        ErrorCode::Cancelled
    );
    assert!(job_dir.is_dir());
    job_dir
}

fn rewrite(job_dir: &Path, edit: impl FnOnce(&mut serde_json::Value)) {
    let path = job_dir.join("job.json");
    let mut record: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    edit(&mut record);
    std::fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
}

/// A home with no snapshot of any root.
fn never(_: &Path) -> Option<std::time::SystemTime> {
    None
}

/// A home whose every root was published after any job's last write.
fn always(_: &Path) -> Option<std::time::SystemTime> {
    Some(std::time::SystemTime::now() + std::time::Duration::from_secs(3600))
}

/// A settled fact cache lives with the snapshots of its root: it stays while
/// one remains, is never counted as unfinished bytes, and goes with the last.
#[test]
fn a_settled_cache_lives_and_dies_with_its_roots_snapshots() {
    let (_dir, scan) = fixture();
    let base = scan.checkpoint.clone().unwrap();
    let built = build(&scan, &NeverCancel).unwrap();
    let job_dir = job_path(&base, &scan.root);
    settle(
        &job_dir,
        built.construction.checkpoint_generation.as_deref().unwrap(),
    )
    .unwrap();
    let older = |_: &Path| Some(std::time::SystemTime::UNIX_EPOCH);
    assert_eq!(prune_jobs(&base, &older), 0, "a cache is not unfinished");
    assert!(job_dir.is_dir(), "its root still has a snapshot");
    assert_eq!(prune_jobs(&base, &always), 0);
    assert!(job_dir.is_dir(), "a newer snapshot does not end a cache");
    assert_eq!(prune_jobs(&base, &never), 0);
    assert!(!job_dir.exists(), "no snapshot of its root is left");
    #[cfg(unix)]
    assert!(!lock::job_lock_path(&job_dir).exists());
}

#[test]
fn a_held_job_is_never_pruned_whatever_supersedes_it() {
    let (_dir, scan) = fixture();
    let job_dir = interrupted(&scan);
    rewrite(&job_dir, |record| {
        record["root"] = serde_json::json!("/nowhere/gone");
    });
    let held = lock::try_job(&lock::job_lock_path(&job_dir))
        .unwrap()
        .expect("unheld");
    let kept = prune_jobs(scan.checkpoint.as_ref().unwrap(), &always);
    assert!(job_dir.is_dir(), "a held job stays");
    assert!(kept > 0, "its bytes are disclosed");
    drop(held);
    assert_eq!(prune_jobs(scan.checkpoint.as_ref().unwrap(), &always), 0);
    assert!(!job_dir.exists());
}

/// The bytes a build states are its own root's jobs: a held job of another
/// root in the same home is neither pruned nor counted.
#[test]
fn kept_job_bytes_count_only_the_given_root() {
    let (_dir, scan) = fixture();
    let base = scan.checkpoint.clone().unwrap();
    let job_dir = interrupted(&scan);
    rewrite(&job_dir, |record| {
        record["root"] = serde_json::json!("/nowhere/gone");
    });
    let _held = lock::try_job(&lock::job_lock_path(&job_dir))
        .unwrap()
        .expect("unheld");
    let other = std::path::Path::new("/some/other/root");
    assert_eq!(prune_jobs_of(&base, &always, Some(other)), 0);
    assert!(job_dir.is_dir(), "another root's held job stays");
    assert!(prune_jobs_of(&base, &always, Some(std::path::Path::new("/nowhere/gone"))) > 0);
    assert!(prune_jobs(&base, &always) > 0, "the home's total");
}

#[test]
fn a_resumable_job_stays_until_its_root_is_gone_or_superseded() {
    let (_dir, scan) = fixture();
    let base = scan.checkpoint.clone().unwrap();
    let job_dir = interrupted(&scan);
    let kept = prune_jobs(&base, &never);
    assert!(job_dir.is_dir(), "resumable");
    assert!(kept > 0);
    assert_eq!(prune_jobs(&base, &always), 0, "superseded");
    assert!(!job_dir.exists());
    #[cfg(unix)]
    assert!(!lock::job_lock_path(&job_dir).exists());

    let job_dir = interrupted(&scan);
    rewrite(&job_dir, |record| {
        record["root"] = serde_json::json!("/nowhere/gone");
    });
    assert_eq!(prune_jobs(&base, &never), 0, "its root is gone");
    assert!(!job_dir.exists());
}

#[test]
fn jobs_of_an_older_version_go_and_a_newer_builds_stay() {
    let (_dir, scan) = fixture();
    let base = scan.checkpoint.clone().unwrap();
    let job_dir = interrupted(&scan);
    rewrite(&job_dir, |record| {
        record["version"] = serde_json::json!(JOB_VERSION + 1);
    });
    assert!(prune_jobs(&base, &always) > 0);
    assert!(job_dir.is_dir(), "a newer build's job is that build's");
    rewrite(&job_dir, |record| {
        record["version"] = serde_json::json!(JOB_VERSION - 1);
        record.as_object_mut().unwrap().remove("root");
    });
    assert_eq!(prune_jobs(&base, &never), 0);
    assert!(!job_dir.exists(), "no build resumes an older version's job");
}

#[cfg(unix)]
#[test]
fn an_orphan_lock_file_is_removed_and_a_held_one_stays() {
    let (_dir, scan) = fixture();
    let base = scan.checkpoint.clone().unwrap();
    let jobs = base.join("jobs");
    std::fs::create_dir_all(&jobs).unwrap();
    let orphan = jobs.join("gone.lock");
    let held_path = jobs.join("busy.lock");
    std::fs::write(&orphan, b"").unwrap();
    let held = lock::try_job(&held_path).unwrap().expect("unheld");
    prune_jobs(&base, &never);
    assert!(!orphan.exists());
    assert!(held_path.exists(), "a held lock stays");
    // Its holder still holds the file its path names.
    assert!(held.still_at(&held_path));
}
