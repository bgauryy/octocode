//! Process-wide admission gate for classification provider requests.
//!
//! Every Clasify provider request, from every concurrent
//! tool call in this process, passes through one [`ClassificationGate`] per
//! provider endpoint (scheme + host + port + path) and account (key). The gate:
//!
//! - bounds in-flight requests to `classification.maxConcurrency`;
//! - adapts that bound (AIMD): a throttle response (429/503/529) halves the
//!   effective limit, and every [`RESTORE_AFTER_SUCCESSES`] successes restore
//!   one permit, never below 1 nor above the configured limit;
//! - shares a `not_before` cooldown taken from `Retry-After`, so one throttle
//!   response pauses every task instead of each rediscovering it;
//! - opens a short circuit after [`CIRCUIT_THRESHOLD`] consecutive
//!   failures (transport errors / 5xx), failing fast until it cools down; one
//!   further failure after the cooldown re-opens it, one success closes it;
//! - caps a single tool call at `max(1, limit * 3 / 4)` permits so one large
//!   matrix cannot starve concurrent calls;
//! - remembers exhausted billing or quota (HTTP 402) for [`QUOTA_MEMO`]:
//!   every call fails fast until then, and the first call after it probes
//!   again, since credit can be added.
//!
//! Permits are held only while a request is on the wire; callers drop them
//! before any retry backoff sleep. The gate is transport-neutral: it reports
//! admission failures as [`GateDenied`] and the transport maps them to public
//! error codes.
use crate::providers::RequestBudget;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};
use tokio::sync::Notify;
use url::Url;

/// Successes needed to restore one permit after a multiplicative decrease.
pub(crate) const RESTORE_AFTER_SUCCESSES: u32 = 4;
/// Consecutive failures that open the circuit.
pub(crate) const CIRCUIT_THRESHOLD: u32 = 5;
/// How long an open circuit fails fast before admitting a probe.
pub(crate) const CIRCUIT_COOLDOWN: Duration = Duration::from_secs(10);
/// How long a 402 stops every call to the endpoint before one probes again.
pub(crate) const QUOTA_MEMO: Duration = Duration::from_secs(60);
/// Bounds mirrored from `classification.maxConcurrency` in the config contract.
const MIN_LIMIT: usize = 1;
const MAX_LIMIT: usize = 64;

/// Why the gate refused to admit a request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GateDenied {
    Cancelled,
    /// The request deadline passed while waiting for a permit.
    Deadline,
    /// A shared provider cooldown outlasts the remaining deadline.
    RateLimited {
        retry_after: Duration,
    },
    /// Too many consecutive provider failures; fail fast until cooldown ends.
    CircuitOpen {
        retry_after: Duration,
    },
    /// The provider reported exhausted billing or quota within [`QUOTA_MEMO`].
    QuotaExhausted,
}

#[derive(Debug)]
struct GateState {
    limit: usize,
    effective: usize,
    in_flight: usize,
    successes: u32,
    not_before: Option<Instant>,
    consecutive_failures: u32,
    open_until: Option<Instant>,
    quota_until: Option<Instant>,
}

/// Admission state for one provider endpoint.
#[derive(Debug)]
pub(crate) struct ClassificationGate {
    state: Mutex<GateState>,
    notify: Notify,
}

/// Per-tool-call fairness counter shared by every request of one call.
#[derive(Debug, Default)]
pub(crate) struct CallShare {
    in_flight: AtomicUsize,
}

/// One tool call's handle on an endpoint gate.
#[derive(Clone, Debug)]
pub(crate) struct GateLease {
    gate: Arc<ClassificationGate>,
    share: Arc<CallShare>,
}

/// An admitted in-flight request. Dropping it releases the permit; report the
/// outcome first with [`GatePermit::success`], [`GatePermit::throttled`], or
/// [`GatePermit::failed`] (an unreported drop is neutral).
#[derive(Debug)]
pub(crate) struct GatePermit {
    gate: Arc<ClassificationGate>,
    share: Arc<CallShare>,
}

fn clamp_limit(limit: usize) -> usize {
    limit.clamp(MIN_LIMIT, MAX_LIMIT)
}

/// Largest share of the configured limit one tool call may hold.
pub(crate) fn call_cap(limit: usize) -> usize {
    (limit.saturating_mul(3) / 4).max(1)
}

/// Stable gate key for an endpoint: scheme, host, effective port, and path.
pub(crate) fn endpoint_key(endpoint: &Url) -> String {
    format!(
        "{}://{}:{}{}",
        endpoint.scheme(),
        endpoint.host_str().unwrap_or_default(),
        endpoint.port_or_known_default().unwrap_or_default(),
        endpoint.path()
    )
}

/// A lease on the process-wide gate for `key`, with a fresh per-call share.
/// `limit` is the resolved `classification.maxConcurrency`; a changed value
/// is applied to the existing gate.
pub(crate) fn lease(key: &str, limit: usize) -> GateLease {
    static GATES: OnceLock<Mutex<HashMap<String, Arc<ClassificationGate>>>> = OnceLock::new();
    let gate = {
        let mut gates = GATES
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        gates
            .entry(key.to_owned())
            .or_insert_with(|| Arc::new(ClassificationGate::new(limit)))
            .clone()
    };
    gate.set_limit(limit);
    GateLease {
        gate,
        share: Arc::new(CallShare::default()),
    }
}

impl ClassificationGate {
    pub(crate) fn new(limit: usize) -> Self {
        let limit = clamp_limit(limit);
        Self {
            state: Mutex::new(GateState {
                limit,
                effective: limit,
                in_flight: 0,
                successes: 0,
                not_before: None,
                consecutive_failures: 0,
                open_until: None,
                quota_until: None,
            }),
            notify: Notify::new(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, GateState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn set_limit(&self, limit: usize) {
        let limit = clamp_limit(limit);
        let mut state = self.lock();
        if state.limit == limit {
            return;
        }
        // A healthy gate follows the new limit; a throttled one keeps its
        // reduced window (capped by the new limit) and restores via AIMD.
        state.effective = if state.effective >= state.limit {
            limit
        } else {
            state.effective.min(limit)
        };
        state.limit = limit;
        drop(state);
        self.notify.notify_waiters();
    }

    #[cfg(test)]
    pub(crate) fn effective_limit(&self) -> usize {
        self.lock().effective
    }

    #[cfg(test)]
    pub(crate) fn in_flight(&self) -> usize {
        self.lock().in_flight
    }

    /// Whether a 402 within [`QUOTA_MEMO`] still stops requests; an expired
    /// memo is cleared so the next request probes.
    fn quota_exhausted(&self, now: Instant) -> bool {
        let mut state = self.lock();
        match state.quota_until {
            Some(until) if now < until => true,
            Some(_) => {
                state.quota_until = None;
                false
            }
            None => false,
        }
    }

    #[cfg(test)]
    pub(crate) fn expire_quota_memo(&self) {
        self.lock().quota_until = Some(Instant::now());
    }

    fn release(&self, share: &CallShare) {
        let mut state = self.lock();
        state.in_flight = state.in_flight.saturating_sub(1);
        share.in_flight.fetch_sub(1, Ordering::Relaxed);
        drop(state);
        self.notify.notify_waiters();
    }
}

impl GateLease {
    /// A lease on the same gate with a fresh fairness share.
    #[cfg(test)]
    pub(crate) fn fork(&self) -> Self {
        Self {
            gate: self.gate.clone(),
            share: Arc::new(CallShare::default()),
        }
    }

    #[cfg(test)]
    pub(crate) fn gate(&self) -> &ClassificationGate {
        &self.gate
    }

    /// Whether the endpoint reported exhausted billing or quota within
    /// [`QUOTA_MEMO`]: a call checks this before capturing any evidence.
    pub(crate) fn quota_exhausted(&self) -> bool {
        self.gate.quota_exhausted(Instant::now())
    }

    /// Wait for a permit, honouring cancellation, the budget deadline, the
    /// shared cooldown, the adaptive limit, and this call's fairness cap.
    pub(crate) async fn acquire(&self, budget: &RequestBudget) -> Result<GatePermit, GateDenied> {
        loop {
            // Checked every turn: a waiter queued before the 402 stops too.
            if self.gate.quota_exhausted(Instant::now()) {
                return Err(GateDenied::QuotaExhausted);
            }
            if budget.cancellation.is_cancelled() {
                return Err(GateDenied::Cancelled);
            }
            let notified = self.gate.notify.notified();
            tokio::pin!(notified);
            // Register for wakeups before inspecting state so a release that
            // lands between the check and the await is never missed.
            notified.as_mut().enable();
            let now = Instant::now();
            if now >= budget.deadline {
                return Err(GateDenied::Deadline);
            }
            let wake_at = {
                let mut state = self.gate.lock();
                if let Some(open_until) = state.open_until {
                    if now < open_until {
                        return Err(GateDenied::CircuitOpen {
                            retry_after: open_until - now,
                        });
                    }
                    // Cooldown over: admit probes; the failure count stays at
                    // the threshold so one more failure re-opens the circuit.
                    state.open_until = None;
                }
                match state.not_before {
                    Some(not_before) if not_before > now => {
                        if not_before >= budget.deadline {
                            return Err(GateDenied::RateLimited {
                                retry_after: not_before - now,
                            });
                        }
                        Some(not_before)
                    }
                    _ => {
                        let share = self.share.in_flight.load(Ordering::Relaxed);
                        if state.in_flight < state.effective && share < call_cap(state.limit) {
                            state.in_flight += 1;
                            self.share.in_flight.fetch_add(1, Ordering::Relaxed);
                            return Ok(GatePermit {
                                gate: self.gate.clone(),
                                share: self.share.clone(),
                            });
                        }
                        None
                    }
                }
            };
            let sleep_until = wake_at.map_or(budget.deadline, |at| at.min(budget.deadline));
            tokio::select! {
                () = budget.cancellation.cancelled() => return Err(GateDenied::Cancelled),
                () = tokio::time::sleep_until(sleep_until.into()) => {}
                () = &mut notified => {}
            }
        }
    }
}

impl GatePermit {
    /// The provider answered: count toward restoring the adaptive limit and
    /// close any circuit.
    pub(crate) fn success(self) {
        let mut state = self.gate.lock();
        state.consecutive_failures = 0;
        state.open_until = None;
        if state.effective < state.limit {
            state.successes = state.successes.saturating_add(1);
            if state.successes >= RESTORE_AFTER_SUCCESSES {
                state.effective += 1;
                state.successes = 0;
            }
        }
        drop(state);
        // `Drop` releases the permit and wakes waiters.
    }

    /// The provider throttled (429/503/529): halve the effective limit and,
    /// when the provider named a delay, pause every task until it passes.
    pub(crate) fn throttled(self, retry_after: Option<Duration>) {
        let mut state = self.gate.lock();
        state.effective = (state.effective / 2).max(1);
        state.successes = 0;
        if let Some(delay) = retry_after {
            let until = Instant::now() + delay;
            state.not_before = Some(state.not_before.map_or(until, |current| current.max(until)));
        }
    }

    /// Billing or quota exhausted (HTTP 402): not a health signal, so the
    /// limit and circuit stay as they are, but no call sends more until
    /// [`QUOTA_MEMO`] passes.
    pub(crate) fn quota_exhausted(self) {
        self.gate.lock().quota_until = Some(Instant::now() + QUOTA_MEMO);
    }

    /// Transport error or server failure: count toward opening the circuit.
    pub(crate) fn failed(self) {
        let mut state = self.gate.lock();
        state.consecutive_failures = state.consecutive_failures.saturating_add(1);
        state.successes = 0;
        if state.consecutive_failures >= CIRCUIT_THRESHOLD {
            state.open_until = Some(Instant::now() + CIRCUIT_COOLDOWN);
        }
    }
}

impl Drop for GatePermit {
    fn drop(&mut self) {
        self.gate.release(&self.share);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_util::sync::CancellationToken;

    fn budget(timeout: Duration) -> RequestBudget {
        RequestBudget {
            deadline: Instant::now() + timeout,
            cancellation: CancellationToken::new(),
            max_body_bytes: 1024,
        }
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
        assert_eq!(lease(key, 500).gate.effective_limit(), 64);
    }
}
