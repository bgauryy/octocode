use super::*;
use tokio_util::sync::CancellationToken;

fn budget(timeout: Duration) -> RequestBudget {
    RequestBudget::new(Instant::now() + timeout, CancellationToken::new(), 1024)
}

fn lease_on(limit: usize) -> GateLease {
    GateLease {
        gate: Arc::new(ClassificationGate::new(limit)),
        share: Arc::new(CallShare::default()),
    }
}

#[tokio::test]
async fn in_flight_never_exceeds_the_limit_across_calls() {
    let first = lease_on(4);
    let calls = [first.clone(), first.fork(), first.fork()];
    let peak = Arc::new(AtomicUsize::new(0));
    let mut tasks = Vec::new();
    for call in calls {
        for _ in 0..8 {
            let call = call.clone();
            let peak = peak.clone();
            tasks.push(tokio::spawn(async move {
                let permit = call
                    .acquire(&budget(Duration::from_secs(10)))
                    .await
                    .unwrap();
                peak.fetch_max(call.gate().in_flight(), Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(5)).await;
                permit.success();
            }));
        }
    }
    for task in tasks {
        task.await.unwrap();
    }
    assert_eq!(peak.load(Ordering::SeqCst), 4);
    assert_eq!(first.gate().in_flight(), 0);
}

#[tokio::test]
async fn one_call_is_capped_at_three_quarters_of_the_limit() {
    let call = lease_on(8);
    let mut held = Vec::new();
    for _ in 0..call_cap(8) {
        held.push(call.acquire(&budget(Duration::from_secs(1))).await.unwrap());
    }
    assert_eq!(call_cap(8), 6);
    // The same call is now blocked...
    assert_eq!(
        call.acquire(&budget(Duration::from_millis(30)))
            .await
            .unwrap_err(),
        GateDenied::Deadline
    );
    // ...while another call still gets a permit.
    let other = call.fork();
    assert!(other.acquire(&budget(Duration::from_secs(1))).await.is_ok());
    assert_eq!(call_cap(1), 1);
    assert_eq!(call_cap(2), 1);
}

#[tokio::test]
async fn throttle_halves_the_window_and_successes_restore_it() {
    let call = lease_on(8);
    let b = budget(Duration::from_secs(1));
    call.acquire(&b).await.unwrap().throttled(None);
    assert_eq!(call.gate().effective_limit(), 4);
    call.acquire(&b).await.unwrap().throttled(None);
    call.acquire(&b).await.unwrap().throttled(None);
    call.acquire(&b).await.unwrap().throttled(None);
    assert_eq!(call.gate().effective_limit(), 1, "never below one permit");
    for _ in 0..RESTORE_AFTER_SUCCESSES {
        call.acquire(&b).await.unwrap().success();
    }
    assert_eq!(call.gate().effective_limit(), 2);
    for _ in 0..RESTORE_AFTER_SUCCESSES * 20 {
        call.acquire(&b).await.unwrap().success();
    }
    assert_eq!(call.gate().effective_limit(), 8, "never above the limit");
}

#[tokio::test]
async fn retry_after_cooldown_is_shared_by_every_call() {
    let call = lease_on(4);
    let other = call.fork();
    call.acquire(&budget(Duration::from_secs(1)))
        .await
        .unwrap()
        .throttled(Some(Duration::from_millis(150)));
    let started = Instant::now();
    let permit = other
        .acquire(&budget(Duration::from_secs(2)))
        .await
        .unwrap();
    assert!(
        started.elapsed() >= Duration::from_millis(140),
        "second call ignored the shared cooldown: {:?}",
        started.elapsed()
    );
    drop(permit);
    // A cooldown longer than the remaining deadline is reported, not slept.
    call.acquire(&budget(Duration::from_secs(1)))
        .await
        .unwrap()
        .throttled(Some(Duration::from_secs(30)));
    assert!(matches!(
        other.acquire(&budget(Duration::from_secs(1))).await,
        Err(GateDenied::RateLimited { retry_after }) if retry_after > Duration::from_secs(20)
    ));
}

#[tokio::test]
async fn consecutive_failures_open_the_circuit_and_success_closes_it() {
    let call = lease_on(4);
    let b = budget(Duration::from_secs(1));
    for _ in 0..CIRCUIT_THRESHOLD {
        call.acquire(&b).await.unwrap().failed();
    }
    assert!(matches!(
        call.acquire(&b).await,
        Err(GateDenied::CircuitOpen { .. })
    ));
    // Simulate the cooldown expiring: one probe is admitted and a success
    // closes the circuit.
    call.gate().lock().open_until = Some(Instant::now());
    call.acquire(&b).await.unwrap().success();
    call.acquire(&b).await.unwrap().failed();
    assert!(call.acquire(&b).await.is_ok(), "one failure after close");
}

#[tokio::test]
async fn cancellation_interrupts_a_waiting_acquire() {
    let call = lease_on(1);
    let _held = call.fork().acquire(&budget(Duration::from_secs(1))).await;
    let b = budget(Duration::from_secs(5));
    let token = b.cancellation.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(20)).await;
        token.cancel();
    });
    assert_eq!(call.acquire(&b).await.unwrap_err(), GateDenied::Cancelled);
}

#[test]
fn endpoint_keys_include_port_and_path_and_limits_are_applied() {
    let a = Url::parse("https://api.example.com/v1/systemone").unwrap();
    let b = Url::parse("https://api.example.com:8443/v1/systemone").unwrap();
    assert_eq!(endpoint_key(&a), "https://api.example.com:443/v1/systemone");
    assert_ne!(endpoint_key(&a), endpoint_key(&b));
    let key = "test://gate-limit-update";
    assert_eq!(lease(key, 10).gate.effective_limit(), 10);
    assert_eq!(lease(key, 3).gate.effective_limit(), 3);
    assert_eq!(lease(key, 0).gate.effective_limit(), 1);
    assert_eq!(lease(key, 500).gate.effective_limit(), 500);
}

/// Past the retained count, idle gates are dropped, while one that holds
/// a permit or a running cooldown keeps its state.
#[tokio::test]
async fn only_idle_gates_are_dropped_past_the_retained_count() {
    let busy = lease("test://retain-busy", 2);
    let permit = busy
        .acquire(&budget(Duration::from_secs(5)))
        .await
        .expect("permit");
    drop(busy);
    let cooling = lease("test://retain-cooling", 2);
    cooling.gate.lock().not_before = Some(Instant::now() + Duration::from_secs(60));
    drop(cooling);
    for index in 0..RETAINED_GATES * 2 {
        drop(lease(&format!("test://retain-idle-{index}"), 2));
    }
    assert_eq!(
        lease("test://retain-busy", 2).gate.in_flight(),
        1,
        "a held permit survives"
    );
    assert!(
        lease("test://retain-cooling", 2)
            .gate
            .is_active(Instant::now()),
        "a running cooldown survives"
    );
    drop(permit);
}
