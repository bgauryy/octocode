use super::*;

#[test]
fn a_busy_install_lock_is_waited_for_then_taken() {
    // Concurrent installers: the loser waits for the winner instead of failing.
    let dir = tempfile::tempdir().expect("tempdir");
    let lock = dir.path().join(".lock");
    let held = hold(&lock);
    let release = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        drop(held);
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let started = std::time::Instant::now();
    let taken = runtime.block_on(acquire_or_wait(
        lock.clone(),
        Duration::from_secs(5),
        Duration::from_millis(25),
    ));
    release.join().expect("release");
    assert!(taken.is_some(), "the lock is taken once its holder lets go");
    assert!(started.elapsed() >= Duration::from_millis(250));
}

#[test]
fn a_lock_file_left_by_a_crashed_installer_does_not_block() {
    // A holder that died leaves only the file; the kernel already released it.
    let dir = tempfile::tempdir().expect("tempdir");
    let lock = dir.path().join(".lock");
    std::fs::write(&lock, b"").expect("leftover lock file");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let started = std::time::Instant::now();
    let taken = runtime.block_on(acquire_or_wait(
        lock,
        Duration::from_millis(200),
        Duration::from_millis(25),
    ));
    assert!(taken.is_some(), "a leftover file is not a held lock");
    assert!(started.elapsed() < Duration::from_millis(150));
}

#[test]
fn a_lock_held_past_the_wait_is_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    let lock = dir.path().join(".lock");
    let _held = hold(&lock);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let taken = runtime.block_on(acquire_or_wait(
        lock,
        Duration::from_millis(150),
        Duration::from_millis(25),
    ));
    assert!(taken.is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn an_installed_binary_resolves_without_another_download() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let platform = "linux-x64";
    let bin = cached_server_bin_path(root, "rust-analyzer", platform).expect("bin path");
    // install_binary is the exact step provision_server runs after verification.
    install_binary(
        &bin,
        &b"rust-analyzer-bytes"[..],
        MAX_EXTRACTED_BYTES,
        pinned_asset_sha("rust-analyzer", platform),
    )
    .expect("install");
    assert_eq!(
        std::fs::read(&bin).expect("read bin"),
        b"rust-analyzer-bytes"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&bin).expect("meta").permissions().mode();
        // Owner-only executable: the binary lives under the user's home, and
        // the one atomic writer never grants group or other access.
        assert_eq!(mode & 0o700, 0o700, "installed binary is owner-executable");
        assert_eq!(
            mode & 0o077,
            0,
            "installed binary is not readable by others"
        );
        // The directory holding it is private too: no other user can
        // replace the binary between install and launch.
        let dir = bin.parent().expect("managed directory");
        let dir_mode = std::fs::metadata(dir)
            .expect("dir meta")
            .permissions()
            .mode();
        assert_eq!(dir_mode & 0o077, 0, "managed directory is private");
        // The completion marker is written the same way: owner-only, atomic.
        let marker_mode = std::fs::metadata(marker_path(&bin))
            .expect("marker")
            .permissions()
            .mode();
        assert_eq!(marker_mode & 0o077, 0, "completion marker is owner-only");
    }
    // Marker present + trusted → resolve returns the path.
    assert_eq!(
        resolve_cached_server(root, "rust-analyzer", platform),
        Some(bin.clone())
    );
    // provision_server short-circuits to already-present (fetch must not run).
    let again =
        provision_server(root, "rust-analyzer", platform, LspAutoInstall::Auto, never).await;
    assert_eq!(again.source, Some("already-present"));
}

/// Each refusal fails the outcome and names its reason.
#[tokio::test(flavor = "current_thread")]
async fn provision_refusals_name_their_reason() {
    type Fetch = fn(&'static str, tokio::fs::File) -> Spooled;
    let cases: [(&str, &str, LspAutoInstall, Fetch, &str); 3] = [
        (
            "rust-analyzer",
            "linux-x64",
            LspAutoInstall::Auto,
            tampered,
            "Checksum mismatch",
        ),
        (
            "clangd",
            "linux-x64",
            LspAutoInstall::Off,
            never,
            "Auto-install is off",
        ),
        (
            "clangd",
            "linux-arm64",
            LspAutoInstall::Auto,
            never,
            "no linux-arm64 release asset",
        ),
    ];
    for (name, platform, mode, fetch, reason) in cases {
        let dir = tempfile::tempdir().expect("tempdir");
        let outcome = provision_server(dir.path(), name, platform, mode, fetch).await;
        assert!(!outcome.ok, "{reason}");
        let error = outcome.error.expect("error should be set");
        assert!(error.contains(reason), "{reason}: {error}");
    }
}

/// A refused download is removed, and so are the spool and temporaries a
/// killed installer left behind, once the next installer holds the lock.
#[tokio::test(flavor = "current_thread")]
async fn no_download_outlives_its_install() {
    let dir = tempfile::tempdir().expect("tempdir");
    let install_dir = dir.path().join("rust-analyzer/2026-06-22");
    std::fs::create_dir_all(&install_dir).expect("install dir");
    for leftover in [".rust-analyzer-0f.download", ".rust-analyzer-1e.tmp"] {
        std::fs::write(install_dir.join(leftover), b"interrupted").expect("leftover");
    }
    let outcome = provision_server(
        dir.path(),
        "rust-analyzer",
        "linux-x64",
        LspAutoInstall::Auto,
        tampered,
    )
    .await;
    assert!(outcome.error.expect("error").contains("Checksum mismatch"));
    assert_eq!(install_dir_leftovers(dir.path()), Vec::<String>::new());
}

/// The CLI drives a current-thread runtime; installing must not need a
/// multi-thread one, and the per-target lock is free afterwards.
#[tokio::test(flavor = "current_thread")]
async fn install_runs_on_the_current_thread_runtime() {
    let dir = tempfile::tempdir().expect("tempdir");
    let code = run_install(
        dir.path(),
        "linux-x64",
        vec!["rust-analyzer".to_owned()],
        LspAutoInstall::Auto,
        true,
        tampered,
    )
    .await;
    assert_eq!(code, 3);
    let lock = dir.path().join("rust-analyzer/2026-06-22/.lock");
    assert!(lock_is_free(&lock));
}

/// A panic mid-install still releases the lock, so the next install is
/// not refused as "in progress".
#[tokio::test(flavor = "current_thread")]
async fn a_panicking_install_releases_its_lock() {
    use futures_util::FutureExt;
    let dir = tempfile::tempdir().expect("tempdir");
    let panicked = std::panic::AssertUnwindSafe(provision_server(
        dir.path(),
        "rust-analyzer",
        "linux-x64",
        LspAutoInstall::Auto,
        never,
    ))
    .catch_unwind()
    .await;
    assert!(panicked.is_err());
    assert!(lock_is_free(
        &dir.path().join("rust-analyzer/2026-06-22/.lock")
    ));
    let retry = provision_server(
        dir.path(),
        "rust-analyzer",
        "linux-x64",
        LspAutoInstall::Auto,
        tampered,
    )
    .await;
    assert!(retry.error.expect("error").contains("Checksum mismatch"));
}
