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

fn never(_: &Path, _: std::time::SystemTime) -> bool {
    false
}

fn always(_: &Path, _: std::time::SystemTime) -> bool {
    true
}

#[test]
fn completion_removes_the_job_and_its_lock_file() {
    let (_dir, scan) = fixture();
    let built = build(&scan, &NeverCancel).unwrap();
    let job_dir = job_path(scan.checkpoint.as_ref().unwrap(), &scan.root);
    complete(
        &job_dir,
        built.construction.checkpoint_generation.as_deref().unwrap(),
    )
    .unwrap();
    assert!(!job_dir.exists());
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
