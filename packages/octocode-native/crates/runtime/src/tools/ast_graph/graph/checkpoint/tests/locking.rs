//! One build at a time writes a scope's extraction job; a waiting build can
//! be cancelled, and settling leaves a job another build holds alone.
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Allows the given number of checks, then cancels.
struct AfterChecks(AtomicUsize);
impl CancellationCheck for AfterChecks {
    fn check(&self) -> Result<(), String> {
        if self.0.fetch_sub(1, Ordering::SeqCst) == 0 {
            return Err("stopped waiting".into());
        }
        Ok(())
    }
}

fn open<'a>(
    scan: &GraphScan,
    policy: &'a PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<Option<Job<'a>>, ToolError> {
    Job::open(
        scan,
        &scan.root,
        &[],
        policy,
        RegistryView::installed(),
        cancel,
    )
}

#[test]
fn a_held_job_blocks_a_second_build_until_released_and_the_wait_cancels() {
    let (_dir, scan) = fixture();
    let policy = workspace_policy(&scan.root);
    let held = open(&scan, &policy, &NeverCancel).unwrap().expect("job");
    let job_dir = job_path(scan.checkpoint.as_ref().unwrap(), &scan.root);
    let artifact = job_dir.join("kept.fact");
    std::fs::write(&artifact, b"in-flight").unwrap();
    let waited = open(&scan, &policy, &AfterChecks(AtomicUsize::new(3)));
    let error = waited.err().expect("the held job is not opened twice");
    assert_eq!(error.code, ErrorCode::Cancelled);
    assert!(
        artifact.is_file(),
        "a waiting build never prunes the holder's files"
    );
    // Settling by another build leaves the held job as its holder left it.
    settle(&job_dir, &held.manifest.generation).unwrap();
    assert!(Manifest::read(&job_dir).is_some_and(|manifest| manifest.stage == Stage::Extracting));
    drop(held);
    assert!(open(&scan, &policy, &NeverCancel).unwrap().is_some());
    assert!(!artifact.exists(), "the next holder prunes unknown files");
}

/// A graph home that cannot be created under (here: its jobs folder is a
/// file) fails the build as a write, so the recovery names a writable home,
/// not read permissions.
#[test]
fn creating_the_job_folder_under_an_unusable_home_gives_the_write_recovery() {
    let (_dir, scan) = fixture();
    let policy = workspace_policy(&scan.root);
    let base = scan.checkpoint.as_ref().unwrap();
    std::fs::create_dir_all(base).unwrap();
    std::fs::write(base.join("jobs"), b"not a folder").unwrap();
    let error = open(&scan, &policy, &NeverCancel)
        .err()
        .expect("no job folder can be made");
    assert_eq!(error.code, ErrorCode::FileAccessFailed);
    assert!(
        error
            .hints
            .iter()
            .any(|hint| hint == crate::response::rows::WRITE_ACCESS_HINT),
        "{error:?}"
    );
}
