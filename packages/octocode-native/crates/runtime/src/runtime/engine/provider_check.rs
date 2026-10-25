use super::ToolRuntime;
use std::sync::Arc;
use std::time::{Duration, Instant};

const PING_TIMEOUT: Duration = Duration::from_secs(2);

impl ToolRuntime {
    /// Finish host startup before publishing discovery or admitting calls.
    /// Only an otherwise enabled clasify needs this read-only provider ping;
    /// inline classification keeps its independent credential-based policy.
    pub async fn check_clasify_provider(mut self) -> Self {
        if self.is_available("clasify") {
            let mut probe = self.evaluator.clone();
            probe.retries = 0;
            let budget = probe.budget(Instant::now() + PING_TIMEOUT, Default::default());
            if let Err(error) = probe.models(&budget, &probe.gate()).await {
                self.availability = Arc::new(
                    self.availability
                        .with_provider_failure(error.code, error.message),
                );
            }
        }
        self
    }
}
