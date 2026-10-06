//! Per-key circuit breaker, fed only by secondary limits and 5xx.
use super::{ExecutorConfig, KeyState, now_ms};

impl KeyState {
    pub fn record_success(&self) {
        let mut facts = self.facts();
        facts.circuit_failures = 0;
        facts.circuit_open_until_ms = 0;
    }

    /// Only secondary limits and 5xx/transport failures open the circuit.
    pub fn record_circuit_failure(&self, config: &ExecutorConfig) {
        let mut facts = self.facts();
        facts.circuit_failures = facts.circuit_failures.saturating_add(1);
        if facts.circuit_failures >= config.circuit_failures {
            facts.circuit_open_until_ms =
                now_ms().saturating_add(config.circuit_open.as_millis() as u64);
        }
    }
}
