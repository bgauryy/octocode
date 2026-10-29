//! Opt-in, call-scoped phase timings and the state of the server used. No
//! query data or path enters the trace.

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
pub struct Trace(Arc<Mutex<Recorded>>);

#[derive(Default)]
struct Recorded {
    timings: BTreeMap<Phase, Timing>,
    server: Option<ServerNote>,
}

/// The state of the server a traced call used: where discovery found it,
/// its readiness, and the operations it advertises. No command, argv or
/// path, so the trace keeps its no-paths promise.
pub struct ServerNote {
    pub source: &'static str,
    pub readiness: Option<String>,
    pub operations: Vec<&'static str>,
}

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

    /// Record the server the current traced call used; the last note wins.
    /// Outside a traced call `note` is never built.
    pub fn note_server(note: impl FnOnce() -> ServerNote) {
        let _ = CURRENT.try_with(|trace| trace.recorded().server = Some(note()));
    }

    fn recorded(&self) -> std::sync::MutexGuard<'_, Recorded> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn summary(&self) -> String {
        let recorded = self.recorded();
        let mut summary =
            String::from("octocode: LSP trace (cumulative inclusive phases; not additive)");
        for (phase, timing) in recorded.timings.iter() {
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
        if let Some(server) = &recorded.server {
            let _ = write!(summary, "; server: source={}", server.source);
            if let Some(readiness) = &server.readiness {
                let _ = write!(summary, ", readiness={readiness}");
            }
            let _ = write!(summary, ", operations=[{}]", server.operations.join(","));
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
        trace.recorded().timings.entry(phase).or_default().active += 1;
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
        let mut recorded = self.trace.recorded();
        let timing = recorded.timings.entry(self.phase).or_default();
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
