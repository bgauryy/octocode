use super::*;

/// The in-process turn comes first, so a call waiting for it does not hold
/// the sidecar against other processes meanwhile.
#[test]
fn a_call_waiting_its_turn_leaves_the_sidecar_free() {
    let disk = tempfile::tempdir().unwrap();
    let registry = Registry::new(
        CacheConfig::derived(),
        Some(disk.path().into()),
        "policy".into(),
    );
    let sidecar = disk.path().join("continuations.lock");
    let context = context();
    let held = registry.mutation_lock(&context).unwrap();
    let mut pending = Pending::new([5; 32], None);
    let token = insert_page(&mut pending, 1);
    std::thread::scope(|scope| {
        let waiting = scope.spawn(|| registry.retain(pending, &context));
        std::thread::sleep(Duration::from_millis(50));
        assert!(!waiting.is_finished());
        let free = octocode_io::lock::lock_within(
            octocode_io::lock::open_lock_file(&sidecar).unwrap(),
            Duration::ZERO,
            octocode_io::lock::LOCK_POLL,
        );
        assert!(free.is_ok(), "the waiter holds the sidecar");
        drop(free);
        drop(held);
        assert!(waiting.join().unwrap().is_ok());
    });
    assert!(registry.open(ToolId::LocalSearch, &token, [5; 32]).is_ok());
}

/// Parallel calls each publish everything they issued, whichever waits.
#[test]
fn parallel_retains_keep_every_handle_reachable() {
    let disk = tempfile::tempdir().unwrap();
    let registry = Registry::new(
        CacheConfig {
            disk_bytes: 1024 * 1024,
        },
        Some(disk.path().into()),
        "policy".into(),
    );
    const CALLS: usize = 8;
    let gate = std::sync::Barrier::new(CALLS);
    let tokens = std::thread::scope(|scope| {
        let calls = (0..CALLS)
            .map(|call| {
                let (registry, gate) = (&registry, &gate);
                scope.spawn(move || {
                    let mut pending = Pending::new([8; 32], None);
                    let tokens = (0..4)
                        .map(|page| insert_page(&mut pending, call * 10 + page))
                        .collect::<Vec<_>>();
                    gate.wait();
                    assert!(registry.retain(pending, &context()).is_ok());
                    tokens
                })
            })
            .collect::<Vec<_>>();
        calls
            .into_iter()
            .flat_map(|call| call.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(tokens.len(), CALLS * 4);
    let reopened = Registry::new(
        CacheConfig {
            disk_bytes: 1024 * 1024,
        },
        Some(disk.path().into()),
        "policy".into(),
    );
    for runtime in [&registry, &reopened] {
        for token in &tokens {
            assert!(runtime.open(ToolId::LocalSearch, token, [8; 32]).is_ok());
        }
    }
}
