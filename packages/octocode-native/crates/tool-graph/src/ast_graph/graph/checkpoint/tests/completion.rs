use super::*;

#[test]
fn an_older_publication_cannot_settle_a_newer_job() {
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
    settle(&job_dir, first_generation).unwrap();
    let manifest = Manifest::read(&job_dir).unwrap();
    assert_eq!(manifest.generation, second_generation);
    assert!(manifest.stage != Stage::Settled);
    settle(&job_dir, second_generation).unwrap();
    assert!(Manifest::read(&job_dir).is_some_and(|manifest| manifest.stage == Stage::Settled));
    assert!(
        std::fs::read_dir(&job_dir)
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| entry.path().extension().is_some_and(|ext| ext == "fact"))
    );
    // A redundant publication of the already-settled job is harmless.
    settle(&job_dir, second_generation).unwrap();
}

#[test]
fn settling_keeps_the_artifacts_and_the_next_build_reuses_them() {
    let (_dir, scan) = fixture();
    let first = build(&scan, &NeverCancel).unwrap();
    let job_dir = job_path(scan.checkpoint.as_ref().unwrap(), &scan.root);
    settle(
        &job_dir,
        first.construction.checkpoint_generation.as_deref().unwrap(),
    )
    .unwrap();
    let next = build(&scan, &NeverCancel).unwrap();
    assert_eq!(next.reused_files, first.parsed_files);
    assert_eq!(next.parsed_files, 0);
    assert_eq!(
        next.construction.resumed_files, 0,
        "a settled job is a cache, not interrupted work"
    );
    equivalent(first, next);
}
