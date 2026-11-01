//! Per-key circuit breaker, fed only by secondary limits and 5xx/transport
//! outages. One logical request counts one failure, however many attempts
//! it took, and the open circuit reports the failure that opened it.
use super::{ExecutorConfig, KeyState, now_ms};

/// What opened the circuit, so a blocked request reports the real cause: an
/// outage is not a rate limit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CircuitCause {
    /// Repeated secondary rate limits.
    Secondary,
    /// Repeated 5xx responses.
    Server,
    /// Repeated connection failures.
    Transport,
}

impl KeyState {
    pub(crate) fn record_success(&self) {
        let mut facts = self.facts();
        facts.circuit_failures = 0;
        facts.circuit_open_until_ms = 0;
        facts.circuit_cause = None;
    }

    /// Count one failed logical request. Only secondary limits and
    /// 5xx/transport failures open the circuit.
    pub(crate) fn record_circuit_failure(&self, config: &ExecutorConfig, cause: CircuitCause) {
        let mut facts = self.facts();
        facts.circuit_failures = facts.circuit_failures.saturating_add(1);
        facts.circuit_cause = Some(cause);
        if facts.circuit_failures >= config.circuit_failures {
            facts.circuit_open_until_ms =
                now_ms().saturating_add(super::millis(config.circuit_open));
        }
    }
}
