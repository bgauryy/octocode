use super::*;

#[test]
fn pool_budget_holds_several_servers_within_half_the_host() {
    let mib = 1024 * 1024;
    // 16 GiB host: one server caps at 4 GiB; the pool gets 8 GiB.
    assert_eq!(pool_memory_budget_bytes(Some(16 * 1024), 4), 8 * 1024 * mib);
    // 8 GiB host: half is under one cap, so one cap.
    assert_eq!(pool_memory_budget_bytes(Some(8 * 1024), 4), 4 * 1024 * mib);
    // 128 GiB host: half exceeds four 8 GiB caps, so four caps.
    assert_eq!(
        pool_memory_budget_bytes(Some(128 * 1024), 4),
        32 * 1024 * mib
    );
    // Unreadable host memory: one cap.
    assert_eq!(pool_memory_budget_bytes(None, 4), 4 * 1024 * mib);
}

#[test]
fn configured_memory_cap_resolves_default_disable_and_explicit_values() {
    let default = memory_cap_bytes(None).expect("a default cap") / (1024 * 1024);
    assert!(
        (u64::from(MIN_DEFAULT_LSP_MEMORY_CAP_MB)..=u64::from(MAX_DEFAULT_LSP_MEMORY_CAP_MB))
            .contains(&default),
        "{default}"
    );
    assert_eq!(memory_cap_bytes(Some(0)), None);
    assert_eq!(memory_cap_bytes(Some(512)), Some(512 * 1024 * 1024));
}

/// A tree over the cap is reported and killed; one under it is
/// left alone.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[tokio::test]
async fn memory_watchdog_kills_a_tree_over_its_cap() {
    let has_python = std::process::Command::new("python3")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if !has_python {
        return;
    }
    let spawn = |megabytes: u32| {
        let mut command = Command::new("python3");
        command
            .args([
                "-c",
                &format!("import time\nx = b'a' * ({megabytes} << 20)\ntime.sleep(60)\n"),
            ])
            .process_group(0)
            .kill_on_drop(true);
        command.spawn().expect("spawn python")
    };
    let mut hog = spawn(200);
    let pid = hog.id().expect("pid");
    let exceeded = std::sync::Arc::new(std::sync::Mutex::new(None));
    let seen = std::sync::Arc::clone(&exceeded);
    let cap = 64 * 1024 * 1024;
    tokio::time::timeout(
        Duration::from_secs(30),
        watch_memory(
            pid,
            cap,
            Duration::from_millis(100),
            Arc::default(),
            || true,
            move |rss| {
                *seen.lock().expect("lock") = Some(rss);
            },
        ),
    )
    .await
    .expect("the watchdog acts well within the timeout");
    let rss = exceeded.lock().expect("lock").expect("cap exceeded");
    assert!(rss > cap, "{rss}");
    let status = tokio::time::timeout(Duration::from_secs(10), hog.wait())
        .await
        .expect("the hog is killed")
        .expect("wait");
    assert!(!status.success());
    assert!(memory_cap_exceeded_message(rss, cap).contains("exceeded memory cap"));

    // Under the cap: the watchdog keeps sampling until `alive` ends it.
    let mut small = spawn(1);
    let pid = small.id().expect("pid");
    let samples = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = std::sync::Arc::clone(&samples);
    let resident = Arc::new(AtomicU64::new(0));
    tokio::time::timeout(
        Duration::from_secs(30),
        watch_memory(
            pid,
            1024 * 1024 * 1024,
            Duration::from_millis(50),
            Arc::clone(&resident),
            move || counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst) < 5,
            |_| panic!("under the cap"),
        ),
    )
    .await
    .expect("alive() ends the watchdog");
    assert!(
        resident.load(Ordering::Relaxed) > 0,
        "each sample is published for the pool's aggregate budget"
    );
    assert!(
        small.try_wait().expect("try_wait").is_none(),
        "left running"
    );
    let _ = small.kill().await;
}

/// `RLIMIT_AS` limits reserved address space, not memory in use. JVMs
/// reserve their maximum heap and class space up front, V8 reserves
/// WebAssembly memory cages and Go its arenas, all far past what they
/// touch. Where the RSS watchdog runs (macOS, Linux), a server must be able
/// to reserve well past the cap; only resident memory over it is stopped
/// (`memory_watchdog_kills_a_tree_over_its_cap`).
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[tokio::test]
async fn default_policy_lets_a_server_reserve_address_space_past_the_cap() {
    let has_python = std::process::Command::new("python3")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if !has_python {
        return;
    }
    let cap = memory_cap_bytes(None).expect("default cap");
    let reserve = cap * 2;
    let mut command = Command::new("python3");
    command.args([
            "-c",
            &format!(
                "import mmap\nm = mmap.mmap(-1, {reserve}, flags=mmap.MAP_PRIVATE | mmap.MAP_ANONYMOUS, prot=0)\nprint(len(m))\n"
            ),
        ]);
    apply_pre_spawn_cap(&mut command, cap);
    let output = command.output().await.expect("spawn python");
    assert!(
        output.status.success(),
        "reserving {reserve} bytes (untouched) must not fail under a {cap}-byte cap: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Unix targets without the RSS watchdog keep `RLIMIT_AS` as their only
/// bound: the default cap still lets a child start, and an allocation past
/// a small cap fails inside the child instead of growing host memory.
#[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
#[tokio::test]
async fn rlimit_as_cap_allows_spawn_and_blocks_allocation_past_it() {
    let mut command = Command::new("/bin/true");
    apply_pre_spawn_cap(&mut command, memory_cap_bytes(None).expect("default cap"));
    assert!(
        command
            .status()
            .await
            .expect("spawn under default cap")
            .success()
    );

    let has_python = std::process::Command::new("python3")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if !has_python {
        return;
    }
    let run = |megabytes: u32| {
        let mut command = Command::new("python3");
        command.args([
            "-c",
            &format!("x = bytearray({megabytes} << 20)\nprint(len(x))\n"),
        ]);
        apply_pre_spawn_cap(&mut command, 256 * 1024 * 1024);
        command.output()
    };
    let small = run(16).await.expect("spawn python under the cap");
    assert!(small.status.success(), "16 MiB fits a 256 MiB cap");
    let over = run(512).await.expect("spawn python under the cap");
    assert!(
        !over.status.success(),
        "512 MiB must fail under a 256 MiB cap"
    );
    assert!(
        String::from_utf8_lossy(&over.stderr).contains("MemoryError"),
        "{}",
        String::from_utf8_lossy(&over.stderr)
    );
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn default_cap_does_not_prevent_process_spawn_on_macos() {
    let mut command = Command::new("/usr/bin/true");
    if let Some(cap_bytes) = memory_cap_bytes(None) {
        apply_pre_spawn_cap(&mut command, cap_bytes);
    }
    match command.status().await {
        Ok(status) => assert!(status.success()),
        Err(error) => panic!("macOS child must spawn with the default policy: {error}"),
    }
}
