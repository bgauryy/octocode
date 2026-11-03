use super::*;

/// A recorder of a walk of `root` whose entries may weigh `max_weight`.
fn recorder(root: &Path, max_weight: usize) -> Recorder {
    let policy = crate::tools::test_support::workspace_policy(root);
    let walk = Arc::new(WalkAdmission::search(
        policy.clone(),
        root.to_path_buf(),
        true,
        &[],
    ));
    Recorder::new(walk, policy, root.to_path_buf(), true, max_weight)
}

/// Entries are kept up to the weight a stored scan may keep; past it
/// the recorder drops them all and the walk is marked unstorable, so a
/// large walk holds no memory proportional to its entries.
#[test]
fn a_walk_past_the_stored_weight_keeps_no_entries() {
    let dir = tempfile::tempdir().expect("fixture");
    let files = (0..20)
        .map(|index| {
            let path = dir.path().join(format!("f{index:02}.txt"));
            std::fs::write(&path, "x").expect("fixture");
            path
        })
        .collect::<Vec<_>>();
    let weight = files.iter().map(|path| path_weight(path)).sum::<usize>();
    let walk = |max_weight| {
        let recording = recorder(dir.path(), max_weight);
        for file in &files {
            assert!(recording.allows(file, false));
        }
        recording.walked()
    };
    let kept = walk(weight);
    assert_eq!(kept.entries, files);
    assert!(!kept.oversize);
    let dropped = walk(weight - 1);
    assert!(dropped.entries.is_empty());
    assert!(dropped.oversize);
    assert_eq!(
        dropped.weight(),
        dropped
            .rules
            .iter()
            .chain(&dropped.markers)
            .map(|path| path_weight(path))
            .sum::<usize>()
    );
}

/// A large repository's walk stays storable: an oversized walk stores
/// no scan, so each of its continuation pages walks the tree again.
#[test]
fn a_large_repository_walk_fits_what_a_stored_scan_may_keep() {
    let weight = Weight {
        bytes: AtomicUsize::new(0),
        max: crate::cache::MEMORY.limit(),
        oversize: AtomicBool::new(false),
    };
    let mut seen = Seen::default();
    for index in 0..100_000 {
        seen.record(
            PathBuf::from(format!(
                "/home/user/code/repository/packages/name/src/area/file-{index:06}.ts"
            )),
            &weight,
        );
    }
    assert!(!weight.oversize.load(Ordering::Relaxed));
    assert_eq!(seen.entries.len(), 100_000);
}

/// Walk threads record into their own shards: the record holds every
/// thread's entries, and the weight bound is over all of them.
#[test]
fn concurrent_walk_threads_share_one_record_and_one_weight() {
    let dir = tempfile::tempdir().expect("fixture");
    let files = (0..64)
        .map(|index| {
            let path = dir.path().join(format!("f{index:02}.txt"));
            std::fs::write(&path, "x").expect("fixture");
            path
        })
        .collect::<Vec<_>>();
    let weight = files.iter().map(|path| path_weight(path)).sum::<usize>();
    let walk = |max_weight| {
        let recording = recorder(dir.path(), max_weight);
        std::thread::scope(|scope| {
            for chunk in files.chunks(8) {
                let recording = &recording;
                scope.spawn(move || chunk.iter().all(|file| recording.allows(file, false)));
            }
        });
        recording.walked()
    };
    let kept = walk(weight);
    assert_eq!(
        kept.entries.iter().collect::<BTreeSet<_>>(),
        files.iter().collect()
    );
    assert!(!kept.oversize);
    let dropped = walk(weight - 1);
    assert!(dropped.entries.is_empty() && dropped.oversize);
}

/// A walked directory's ignore files are entries; a linked one is a
/// dependency no stamp watches.
#[cfg(unix)]
#[test]
fn a_walked_directory_records_its_ignore_inputs() {
    let dir = tempfile::tempdir().expect("fixture");
    let nested = dir.path().join("nested");
    std::fs::create_dir(&nested).expect("fixture");
    std::fs::write(nested.join(".gitignore"), "*.log\n").expect("fixture");
    let walk = || {
        let recording = recorder(dir.path(), usize::MAX);
        assert!(recording.allows(&nested, true));
        recording.walked()
    };
    let walked = walk();
    assert!(walked.entries.contains(&nested.join(".gitignore")));
    assert!(!walked.untracked);
    std::os::unix::fs::symlink(nested.join(".gitignore"), nested.join(".ignore")).expect("link");
    assert!(walk().untracked);
}
