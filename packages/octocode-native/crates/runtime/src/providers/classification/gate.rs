//! Process-wide admission gate for classification provider requests.
//!
//! Every Clasify provider request, from every concurrent
//! tool call in this process, passes through one [`ClassificationGate`] per
//! provider endpoint (scheme + host + port + path) and account (key). The gate:
//!
//! - bounds in-flight requests to the runtime's classification concurrency;
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
use octocode_io::budget::RequestBudget;
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
/// Gates kept before idle ones are dropped (see [`lease`]).
const RETAINED_GATES: usize = 64;

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

/// A gate always admits at least one request.
fn clamp_limit(limit: usize) -> usize {
    limit.max(1)
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
/// `limit` is the runtime's classification concurrency; a changed value is
/// applied to the existing gate.
pub(crate) fn lease(key: &str, limit: usize) -> GateLease {
    static GATES: OnceLock<Mutex<HashMap<String, Arc<ClassificationGate>>>> = OnceLock::new();
    let gate = {
        let mut gates = GATES
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if !gates.contains_key(key) && gates.len() >= RETAINED_GATES {
            // Drop gates nobody holds that remember nothing: no permit out,
            // no cooldown, open circuit or quota memo still running.
            let now = Instant::now();
            gates.retain(|_, gate| Arc::strong_count(gate) > 1 || gate.is_active(now));
        }
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

    /// Whether dropping this gate would lose state: a permit in flight, a
    /// reduced window still restoring, or a cooldown, open circuit or quota
    /// memo that has not run out.
    fn is_active(&self, now: Instant) -> bool {
        let state = self.lock();
        let running = |until: Option<Instant>| until.is_some_and(|until| until > now);
        state.in_flight > 0
            || state.effective < state.limit
            || state.consecutive_failures > 0
            || running(state.not_before)
            || running(state.open_until)
            || running(state.quota_until)
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
    /// [`QUOTA_MEMO`] (`acquire` refuses every permit meanwhile).
    #[cfg(test)]
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
mod tests;
