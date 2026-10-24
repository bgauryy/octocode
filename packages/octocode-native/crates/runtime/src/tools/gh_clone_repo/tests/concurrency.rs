//! Concurrency and scale: one publication per entry, distinct rows at once, large indexes.
use super::*;

#[test]
fn concurrent_requests_publish_once_and_validation_fails_closed() {
    let fixture = Fixture::shared();
    let (root, cache_home) = home_root();
    let policy = Arc::new(test_support::workspace_policy(&root.0));
    let endpoint = Arc::new(api_github_endpoint());
    let runner = Arc::new(fixture_runner(&fixture));
    let config = Arc::new(CloneConfig::persistent(&cache_home));
    let barrier = Arc::new(Barrier::new(2));
    let pinned = Arc::new(fixture.pin(&query()));
    let mut workers = vec![];
    for _ in 0..2 {
        let pinned = pinned.clone();
        let policy = policy.clone();
        let endpoint = endpoint.clone();
        let runner = runner.clone();
        let config = config.clone();
        let barrier = barrier.clone();
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            execute_clone(
                &pinned,
                &CloneContext {
                    config: &config,
                    endpoint: &endpoint,
                    credential: None,
                    cancellation: &NeverCancel,
                    deadline: Instant::now() + Duration::from_secs(30),
                    path_policy: &policy,
                    git: runner.as_ref(),
                },
            )
            .expect("race clone")
        }));
    }
    let values = workers
        .into_iter()
        .map(|worker| worker.join().expect("worker"))
        .collect::<Vec<_>>();
    assert_eq!(
        values.iter().filter(|value| value.location.cached).count(),
        1
    );
    assert_eq!(values[0].location.commit_sha, values[1].location.commit_sha);

    // A traversal owner never reaches the clone: the contract rejects it.
    assert!(
        "../escape"
            .parse::<octocode_contract_types::GhCloneRepoQueryOwner>()
            .is_err(),
        "the contract rejects a traversal owner"
    );
    assert!(
        rejects_sparse_path("../escape"),
        "contract must reject sparse parent traversal before clone"
    );
}

/// A published entry records its whole size (checkout and `.git`), which
/// eviction weighs it by instead of walking it; a file beside the entries
/// (a retired layout's default-branch memo) is swept by the next eviction
/// pass; rows for two entries run at once and both publish.
#[test]
fn entries_record_their_size_orphans_are_swept_and_distinct_rows_run_at_once() {
    let fixture = Fixture::shared();
    let (root, cache_home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = CloneConfig::persistent(&cache_home);
    let repo_dir = cache::clones_root(&cache_home)
        .join("fixture-owner")
        .join("fixture-repo");
    fs::create_dir_all(&repo_dir).expect("repo dir");
    let orphan = repo_dir.join(".default-branch__host_0123456789abcdef.json");
    fs::write(&orphan, "{}").expect("orphan");
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let fresh = clone_pinned(&fixture, &query(), &context).expect("fresh clone");
    let local = Path::new(&fresh.location.local_path);
    let meta: cache::CacheMeta =
        serde_json::from_slice(&fs::read(local.join(cache::META_FILE)).expect("meta"))
            .expect("meta json");
    let checkout = meta.checkout_bytes;
    assert_eq!(checkout, fresh.bytes);
    assert!(
        meta.size_bytes > checkout,
        "the entry's size counts .git too: {meta:?}"
    );
    assert!(!orphan.exists(), "an orphaned file is swept");
    assert!(repo_dir.is_dir(), "entry directories stay");

    let rows = std::thread::scope(|scope| {
        let workers = ["README.md", "src/lib.rs"].map(|path| {
            let (fixture, runner, config) = (&fixture, &runner, &config);
            let (endpoint, policy) = (&endpoint, &policy);
            scope.spawn(move || {
                let context = setup(runner, config, endpoint, policy, &NeverCancel, None);
                clone_pinned(
                    fixture,
                    &GhCloneRepoQuery {
                        paths: one(path),
                        ..query()
                    },
                    &context,
                )
            })
        });
        workers.map(|worker| worker.join().expect("worker"))
    });
    for row in rows {
        let row = row.expect("concurrent distinct clone");
        assert!(!row.location.cached);
        assert!(Path::new(&row.location.local_path).is_dir());
    }
}

#[test]
fn large_indexes_clone_whole_repositories_and_sparse_trees_without_exposing_symlinks() {
    use std::io::Write;
    use std::process::Stdio;

    let fixture = Fixture::new();
    let blob = git_output(&fixture.work, &["rev-parse", "HEAD:README.md"]);
    let target = "../../secret.json";
    let target_file = fixture.work.join("link-target.txt");
    fs::write(&target_file, target).expect("link target bytes");
    let link_blob = git_output(&fixture.work, &["hash-object", "-w", "link-target.txt"]);
    fs::remove_file(target_file).expect("remove unstaged helper");
    // Git output is capped at 64 KiB here (5 MiB in production), so about
    // 800 files carry the index past the cap.
    let cap = 64 * 1024;
    let mut entries = Vec::new();
    let mut files = 0;
    while entries.len() <= cap {
        write!(
            &mut entries,
            "100644 {}\tbulk/file-{files:06}-with spaces.txt\0",
            blob.trim()
        )
        .expect("index record");
        files += 1;
    }
    write!(&mut entries, "120000 {}\tescape\0", link_blob.trim()).expect("symlink index record");
    let mut child = fixture_git_command(&fixture.work, &["update-index", "-z", "--index-info"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("index writer");
    child
        .stdin
        .take()
        .expect("index stdin")
        .write_all(&entries)
        .expect("write index records");
    let indexed = child.wait_with_output().expect("index writer exit");
    assert!(
        indexed.status.success(),
        "{}",
        String::from_utf8_lossy(&indexed.stderr)
    );
    git(&fixture.work, &["commit", "-m", "large generated index"]);
    git(&fixture.work, &["push", "origin", "main"]);
    let index_bytes = fixture_git(&fixture.work, &["ls-files", "--stage", "-z"])
        .stdout
        .len();
    assert!(
        index_bytes > cap,
        "fixture must exceed collected output cap"
    );

    let (root, cache_home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = RewriteRunner::with_git(
        "https://github.com/fixture-owner/fixture-repo.git",
        &fixture.bare_url,
        SystemGit::default().with_max_output(cap),
    );
    let mut config = CloneConfig::persistent(&cache_home);
    config.cache_ttl = Duration::from_secs(120);
    let mut context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    context.deadline = Instant::now() + Duration::from_secs(120);
    for scope in [None, Some("README.md"), Some("src")] {
        let query = GhCloneRepoQuery {
            paths: scope.map(one).unwrap_or_default(),
            ..query()
        };
        let cloned = clone_pinned(&fixture, &query, &context).expect("large-index clone");
        let checkout = Path::new(&cloned.location.local_path);
        if scope.is_none() {
            assert_eq!(
                fs::read_dir(checkout.join("bulk"))
                    .expect("bulk checkout")
                    .count(),
                files
            );
        } else {
            assert!(
                !checkout.join("bulk").exists(),
                "sparse checkout must not widen"
            );
        }
        let link = checkout.join("escape");
        if scope == Some("README.md") {
            assert!(!link.exists());
            assert_eq!(cloned.location.symlinks, None);
        } else {
            assert!(
                !fs::symlink_metadata(&link)
                    .expect("target-text file")
                    .file_type()
                    .is_symlink()
            );
            assert_eq!(fs::read_to_string(&link).expect("target text"), target);
            assert_eq!(cloned.location.symlinks, Some(vec!["escape".into()]));
        }
        let cached = clone_pinned(&fixture, &query, &context).expect("large-index cache hit");
        assert!(cached.location.cached);
        assert_eq!(cached.location.symlinks, cloned.location.symlinks);
    }
}
