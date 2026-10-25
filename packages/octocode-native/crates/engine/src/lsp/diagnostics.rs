//! Opt-in, call-scoped phase timings. No query data enters the trace.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::future::Future;
use std::sync::{Arc, Mutex};
use tokio::time::{Duration, Instant};

/// Inclusive phase durations: nested or concurrent phases must not be summed
/// as wall-clock latency. Acquire includes shared-start and readiness waits.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Phase {
    Acquire,
    Spawn,
    Initialize,
    Readiness,
    DocumentSync,
    OpenReadiness,
    Request,
    WriteQueue,
}

#[derive(Clone, Default)]
pub struct Trace(Arc<Mutex<BTreeMap<Phase, Timing>>>);

#[derive(Default)]
struct Timing {
    elapsed: Duration,
    completed: u64,
    failed: u64,
    cancelled: u64,
    active: u64,
}

tokio::task_local! {
    static CURRENT: Trace;
}

impl Trace {
    pub fn run<F: Future>(&self, future: F) -> impl Future<Output = F::Output> + use<F> {
        CURRENT.scope(self.clone(), future)
    }

    pub fn carried<F: Future>(future: F) -> impl Future<Output = F::Output> {
        let trace = CURRENT.try_with(Clone::clone).ok();
        async move {
            match trace {
                Some(trace) => trace.run(future).await,
                None => future.await,
            }
        }
    }

    pub fn summary(&self) -> String {
        let timings = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut summary =
            String::from("octocode: LSP trace (cumulative inclusive phases; not additive)");
        for (phase, timing) in timings.iter() {
            let _ = write!(
                summary,
                "; {}: {}ms, completed={}, failed={}, cancelled={}, active={}",
                phase.name(),
                timing.elapsed.as_millis(),
                timing.completed,
                timing.failed,
                timing.cancelled,
                timing.active
            );
        }
        summary
    }
}

impl Phase {
    fn name(self) -> &'static str {
        match self {
            Self::Acquire => "acquire",
            Self::Spawn => "spawn",
            Self::Initialize => "initialize",
            Self::Readiness => "readiness",
            Self::DocumentSync => "documentSync",
            Self::OpenReadiness => "openReadiness",
            Self::Request => "request",
            Self::WriteQueue => "writeQueue",
        }
    }

    pub async fn measure<T, E>(self, future: impl Future<Output = Result<T, E>>) -> Result<T, E> {
        let mut span = CURRENT
            .try_with(|trace| Span::new(trace.clone(), self))
            .ok();
        let result = future.await;
        if let Some(span) = &mut span {
            span.finished = Some(result.is_ok());
        }
        result
    }
}

struct Span {
    trace: Trace,
    phase: Phase,
    started: Instant,
    finished: Option<bool>,
}

impl Span {
    fn new(trace: Trace, phase: Phase) -> Self {
        trace
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(phase)
            .or_default()
            .active += 1;
        Self {
            trace,
            phase,
            started: Instant::now(),
            finished: None,
        }
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        let mut timings = self
            .trace
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let timing = timings.entry(self.phase).or_default();
        timing.active = timing.active.saturating_sub(1);
        timing.elapsed = timing.elapsed.saturating_add(self.started.elapsed());
        let count = match self.finished {
            Some(true) => &mut timing.completed,
            Some(false) => &mut timing.failed,
            None => &mut timing.cancelled,
        };
        *count = count.saturating_add(1);
    }
}

#[cfg(test)]
mod tests;
