use super::*;

#[test]
fn an_older_publication_cannot_complete_a_newer_job() {
    let (_dir, scan) = fixture();
    let first = build(&scan, &NeverCancel).unwrap();
    let first_generation = first.construction.checkpoint_generation.as_deref().unwrap();
    let second = build(&scan, &NeverCancel).unwrap();
    let second_generation = second
        .construction
        .checkpoint_generation
        .as_deref()
        .unwrap();
    assert_ne!(first_generation, second_generation);
    let job_dir = job_path(scan.checkpoint.as_ref().unwrap(), &scan.root);
    complete(&job_dir, first_generation).unwrap();
    assert_eq!(
        Manifest::read(&job_dir).unwrap().generation,
        second_generation
    );
    assert!(
        std::fs::read_dir(&job_dir)
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| entry.path().extension().is_some_and(|ext| ext == "fact"))
    );
    complete(&job_dir, second_generation).unwrap();
    assert!(!job_dir.exists());
    // A redundant publication of the already-completed job is harmless.
    complete(&job_dir, second_generation).unwrap();
}

#[test]
fn completion_removes_artifacts_and_the_next_build_extracts_afresh() {
    let (_dir, scan) = fixture();
    let first = build(&scan, &NeverCancel).unwrap();
    let job_dir = job_path(scan.checkpoint.as_ref().unwrap(), &scan.root);
    complete(
        &job_dir,
        first.construction.checkpoint_generation.as_deref().unwrap(),
    )
    .unwrap();
    assert!(!job_dir.exists());
    let next = build(&scan, &NeverCancel).unwrap();
    assert_eq!(next.reused_files, 0);
    assert_eq!(next.construction.resumed_files, 0);
    assert_eq!(next.parsed_files, first.parsed_files);
    equivalent(first, next);
}

#[test]
fn an_interrupted_completion_marker_cannot_supply_reusable_facts() {
    let (_dir, scan) = fixture();
    let first = build(&scan, &NeverCancel).unwrap();
    let job_dir = job_path(scan.checkpoint.as_ref().unwrap(), &scan.root);
    // Publication committed the terminal marker, then stopped before removing
    // its artifacts. They must never resume as unfinished work.
    Manifest::read(&job_dir)
        .unwrap()
        .write(&job_dir, Stage::Complete)
        .unwrap();
    let next = build(&scan, &NeverCancel).unwrap();
    assert_eq!(next.reused_files, 0);
    assert_eq!(next.construction.resumed_files, 0);
    assert_eq!(next.parsed_files, first.parsed_files);
    equivalent(first, next);
}
