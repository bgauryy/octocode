//! Cache-entry lifecycle: metadata writes and conflicts, corruption, stale locks, eviction and limits.
use super::*;

/// Cache metadata for a shallow checkout of the fixture.
fn fixture_main_meta() -> cache::CacheMeta {
    cache::CacheMeta::new(
        &cache::Identity {
            owner: "fixture-owner",
            repo: "fixture-repo",
            commit: &"1".repeat(40),
            sparse_key: None,
            depth: 1,
        },
        &"1".repeat(40),
        Duration::from_secs(60),
        cache::CheckoutFacts::default(),
    )
}

#[test]
fn clone_eviction_preserves_dirty_expired_and_over_budget_checkouts() {
    let fixture = Fixture::shared();
    let (root, cache_home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let mut config = CloneConfig::persistent(&cache_home);
    config.max_clone_count = 1;
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let fresh = clone_pinned(&fixture, &query(), &context).expect("fresh clone");
    let checkout = PathBuf::from(fresh.location.local_path);
    fs::write(checkout.join(".git/info/exclude"), "ignored-evidence.txt\n").expect("exclude");
    let evidence = checkout.join("ignored-evidence.txt");
    fs::write(&evidence, "preserve through eviction\n").expect("owned evidence");
    // Every ref here names one commit: each history depth is its own entry.
    clone_pinned(
        &fixture,
        &GhCloneRepoQuery {
            ref_: Some(reference("v1")),
            history_depth: std::num::NonZeroU64::new(2).expect("depth"),
            ..query()
        },
        &context,
    )
    .expect("budget clone");
    assert_eq!(
        fs::read_to_string(&evidence).expect("budget preserved"),
        "preserve through eviction\n"
    );
    let meta_path = checkout.join(cache::META_FILE);
    let mut meta: serde_json::Value =
        serde_json::from_slice(&fs::read(&meta_path).expect("metadata")).expect("parse meta");
    meta["expiresAt"] = "1970-01-01T00:00:00.000Z".into();
    fs::write(
        &meta_path,
        serde_json::to_vec(&meta).expect("serialize meta"),
    )
    .expect("expire");
    clone_pinned(
        &fixture,
        &GhCloneRepoQuery {
            ref_: Some(reference(&fixture.first_commit)),
            history_depth: std::num::NonZeroU64::new(3).expect("depth"),
            ..query()
        },
        &context,
    )
    .expect("expiry cleanup clone");
    assert_eq!(
        fs::read_to_string(&evidence).expect("expired preserved"),
        "preserve through eviction\n"
    );
}

#[test]
fn metadata_write_preserves_repository_pid_temp_filename() {
    let stage = test_support::Fixture::new();
    let evidence = stage
        .0
        .join(format!("{}.tmp-{}", cache::META_FILE, std::process::id()));
    fs::write(&evidence, "repository temp-name evidence\n").expect("source evidence");
    let meta = fixture_main_meta();
    cache::write_meta(&stage.0, &meta).expect("new stage metadata");
    assert_eq!(
        fs::read_to_string(&evidence).expect("source filename still exists"),
        "repository temp-name evidence\n"
    );
    let written: cache::CacheMeta =
        serde_json::from_slice(&fs::read(stage.0.join(cache::META_FILE)).expect("metadata"))
            .expect("complete metadata");
    assert_eq!(written.commit_sha, meta.commit_sha);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(stage.0.join(cache::META_FILE))
                .expect("metadata permissions")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[test]
fn metadata_write_rejects_existing_source_without_overwrite() {
    let stage = test_support::Fixture::new();
    let destination = stage.0.join(cache::META_FILE);
    fs::write(&destination, "source arrived after inspection\n").expect("source evidence");
    let meta = fixture_main_meta();
    let error = cache::write_meta(&stage.0, &meta).expect_err("exclusive metadata creation");
    assert_eq!(error.code, ErrorCode::CacheUnavailable);
    assert!(error.message.contains(cache::META_FILE));
    assert!(
        error
            .hints
            .iter()
            .any(|hint| hint.contains("ghGetFileContent"))
    );
    assert_eq!(
        fs::read_to_string(destination).expect("preserved source"),
        "source arrived after inspection\n"
    );
}

#[test]
fn repository_metadata_filename_conflict_never_replaces_an_existing_checkout() {
    struct CollisionRunner {
        inner: RewriteRunner,
        collide: AtomicBool,
        stages: Mutex<Vec<PathBuf>>,
    }
    impl GitRunner for CollisionRunner {
        fn run(
            &self,
            request: &GitRunRequest<'_>,
            control: &GitRunControl<'_>,
        ) -> Result<GitOutput, CloneError> {
            let output = self.inner.run(request, control)?;
            if request.label == "check out requested commit" && self.collide.load(Ordering::SeqCst)
            {
                let stage = request
                    .args
                    .iter()
                    .position(|argument| argument == "-C")
                    .and_then(|at| request.args.get(at + 1))
                    .map(PathBuf::from)
                    .expect("checkout stage argument");
                fs::write(
                    stage.join(cache::META_FILE),
                    "reserved repository evidence\n",
                )
                .expect("write owned source fixture");
                self.stages.lock().expect("stage lock").push(stage);
            }
            Ok(output)
        }
        fn run_nul_records(
            &self,
            request: &GitRunRequest<'_>,
            control: &GitRunControl<'_>,
            record: &mut (dyn FnMut(&[u8]) -> Result<(), CloneError> + Send),
        ) -> Result<GitOutput, CloneError> {
            self.inner.run_nul_records(request, control, record)
        }
    }
    let fixture = Fixture::shared();
    let (root, cache_home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = CollisionRunner {
        inner: fixture_runner(&fixture),
        collide: AtomicBool::new(true),
        stages: Mutex::new(Vec::new()),
    };
    let config = CloneConfig::persistent(&cache_home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let fresh_error =
        clone_pinned(&fixture, &query(), &context).expect_err("reject fresh reserved source");
    assert_eq!(fresh_error.code, ErrorCode::CacheUnavailable);
    assert!(fresh_error.message.contains(cache::META_FILE));
    assert!(fresh_error.message.contains(&fixture.first_commit));
    assert!(
        fresh_error
            .hints
            .iter()
            .any(|hint| hint.contains("ghGetFileContent") && hint.contains(cache::META_FILE))
    );
    assert!(
        runner
            .stages
            .lock()
            .expect("stage lock")
            .iter()
            .all(|stage| !stage.exists()),
        "failed tool-owned staging is cleaned up"
    );
    runner.collide.store(false, Ordering::SeqCst);
    let initial = clone_pinned(&fixture, &query(), &context).expect("clean initial checkout");
    let checkout = PathBuf::from(initial.location.local_path);
    let original_meta = fs::read(checkout.join(cache::META_FILE)).expect("original metadata");
    runner.collide.store(true, Ordering::SeqCst);
    let refresh_error = clone_pinned(
        &fixture,
        &GhCloneRepoQuery {
            force_refresh: Some(true),
            ..query()
        },
        &context,
    )
    .expect_err("reject reserved source before refresh");
    assert_eq!(refresh_error.code, ErrorCode::CacheUnavailable);
    assert_eq!(
        fs::read(checkout.join(cache::META_FILE)).expect("unchanged metadata"),
        original_meta
    );
    assert_eq!(
        fs::read_to_string(checkout.join("README.md")).expect("unchanged source"),
        "fixture\n"
    );
    assert_eq!(
        git_output(&checkout, &["rev-parse", "HEAD"]).trim(),
        fixture.first_commit
    );
}

#[test]
fn corruption_expiry_stale_lock_and_failed_publication_preserve_cache() {
    let fixture = Fixture::shared();
    let (root, cache_home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let mut config = CloneConfig::persistent(&cache_home);
    config.cache_ttl = Duration::from_secs(60);
    config.lock_wait = Duration::from_secs(1);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let first = clone_pinned(&fixture, &query(), &context).expect("first clone");
    let clone_path = PathBuf::from(&first.location.local_path);
    fs::write(clone_path.join(cache::META_FILE), "bad").expect("corrupt meta");
    assert!(
        !clone_pinned(&fixture, &query(), &context)
            .expect("rebuild corrupt")
            .location
            .cached
    );

    let mut expired: serde_json::Value = serde_json::from_slice(
        &fs::read(clone_path.join(cache::META_FILE)).expect("read metadata"),
    )
    .expect("parse metadata");
    expired["expiresAt"] = serde_json::Value::String("1970-01-01T00:00:00.000Z".into());
    fs::write(
        clone_path.join(cache::META_FILE),
        serde_json::to_vec(&expired).expect("encode metadata"),
    )
    .expect("expire metadata");
    assert!(
        !clone_pinned(&fixture, &query(), &context)
            .expect("rebuild expired")
            .location
            .cached
    );

    // A holder that ended without unlocking (a crash closes its descriptor)
    // leaves only the lock file: the kernel released the lock, so the next
    // clone takes it at once, with no age or pid heuristics.
    let lock = cache::lock_path(&cache_home, &clone_path);
    let crashed = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&lock)
        .expect("lock file");
    crashed.try_lock().expect("crashed holder takes the lock");
    drop(crashed);
    assert!(lock.is_file(), "the lock file stays behind");
    assert!(
        clone_pinned(&fixture, &query(), &context)
            .expect("a crashed holder's lock does not block")
            .location
            .cached
    );

    let destination = root.0.join("publication");
    fs::create_dir_all(&destination).expect("old destination");
    fs::write(destination.join("old"), "preserved").expect("old content");
    let missing_stage = root.0.join("missing-stage");
    assert!(cache::promote(&cache_home, &missing_stage, &destination).is_err());
    assert_eq!(
        fs::read_to_string(destination.join("old")).expect("restored"),
        "preserved"
    );
}

#[test]
fn cache_limits_live_lock_and_failed_sparse_refresh_are_bounded() {
    let fixture = Fixture::shared();
    let (root, cache_home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let mut config = CloneConfig::persistent(&cache_home);
    config.max_clone_count = 1;
    config.lock_wait = Duration::from_millis(120);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let main = clone_pinned(&fixture, &query(), &context).expect("main clone");
    let main_path = PathBuf::from(&main.location.local_path);
    // The tag names main's commit: a deeper history is another entry.
    let deep_tag = GhCloneRepoQuery {
        ref_: Some(reference("v1")),
        history_depth: std::num::NonZeroU64::new(2).expect("depth"),
        ..query()
    };
    let tag = clone_pinned(&fixture, &deep_tag, &context).expect("tag clone");
    assert!(!main_path.exists(), "oldest clone should be evicted");
    assert!(Path::new(&tag.location.local_path).is_dir());

    // The eviction must be attributable: one JSONL line in the trail.
    let (evictions, log_bytes) = crate::cache::evictions::recent_evictions(&cache_home, 10);
    let size_limit = evictions
        .iter()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|value| value["reason"] == "size-limit")
        .expect("size-limit eviction recorded in evictions.jsonl");
    assert_eq!(size_limit["path"], main.location.local_path.as_str());
    assert!(size_limit["bytes"].as_u64().is_some());
    assert_eq!(
        size_limit["pid"].as_u64(),
        Some(u64::from(std::process::id()))
    );
    assert!(log_bytes > 0);

    let live_lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(cache::lock_path(
            &cache_home,
            Path::new(&tag.location.local_path),
        ))
        .expect("live lock file");
    let held = match octocode_io::lock::try_lock(live_lock).expect("lock") {
        octocode_io::lock::TryLock::Locked(held) => held,
        octocode_io::lock::TryLock::Busy(_) => panic!("the live lock is free"),
    };
    let error = clone_pinned(&fixture, &deep_tag, &context).expect_err("live lock must time out");
    assert_eq!(error.code, ErrorCode::Timeout);
    drop(held);

    let missing = clone_pinned(
        &fixture,
        &GhCloneRepoQuery {
            paths: one("missing/path"),
            ..query()
        },
        &context,
    )
    .expect_err("missing sparse path");
    assert_eq!(missing.code, ErrorCode::PathNotFound);
    assert!(missing.message.contains("\"missing/path\""), "{missing:?}");
    // The lead, not a hint or the message, names the recovery: nothing near
    // `path` exists at the root, so it lists the root at the commit.
    assert!(missing.hints.is_empty(), "{missing:?}");
    test_support::checks::assert_absent(&serde_json::json!(missing.message), "ghStructure");
    let next = missing.calls.as_deref().expect("recovery lead");
    assert_eq!(next["viewTree"]["tool"], "ghStructure", "{next}");
    assert!(Path::new(&tag.location.local_path).is_dir());
}
