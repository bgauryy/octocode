use octocode_github::{CredentialSource, ResolvedCredential};
use secrecy::SecretString;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{Notify, Semaphore};
use tokio_util::sync::CancellationToken;

/// Longest per-request timeout a runtime accepts. Deadlines are
/// `Instant::now() + timeout`; a bound keeps that sum representable on every
/// platform instead of panicking at admission (M4).
pub(crate) const MAX_TIMEOUT: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Clone, Debug)]
pub struct RequestLimits {
    /// Requests of [`Cost::Walk`] that run at once.
    pub concurrency: usize,
    /// Requests of [`Cost::Light`] that run at once, on slots of their own.
    pub(crate) light_concurrency: usize,
    /// Admitted requests (running or queued, both costs) before `busy`.
    pub pending: usize,
    pub timeout: Duration,
}

/// What a request may cost while it holds a slot, decided at admission.
/// Light requests (a stored page, a read of one small file) run on their own
/// slots, so they never queue behind tree walks.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Cost {
    /// May walk a tree, or wait on a provider or a language server.
    #[default]
    Walk,
    /// Bounded, small work.
    Light,
}

/// The per-request execution budget of every runtime surface (CLI, warm
/// server, MCP): above the worst configured cold start plus one logical
/// request (initialize, language-server readiness, retries and delays, and
/// the request's base timeout); a language server that keeps progressing
/// may wait out the rest of it. `HostOptions::timeout_secs` overrides it for
/// an embedder.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);

impl Default for RequestLimits {
    fn default() -> Self {
        Self {
            concurrency: 4,
            light_concurrency: 4,
            pending: 64,
            timeout: DEFAULT_TIMEOUT,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ExecutionError {
    Closed,
    Busy,
    DuplicateRequest,
    InvalidLimits,
    Cancelled,
    Timeout,
    /// A tool reached a dispatcher that has no route for it.
    UnroutedTool,
    WorkerFailed,
}

impl ExecutionError {
    /// The declared `errorCodes` entry this failure reports.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Closed => "closed",
            Self::Busy => "busy",
            Self::DuplicateRequest => "duplicateRequest",
            Self::Cancelled => "cancelled",
            Self::Timeout => "timeout",
            Self::UnroutedTool => "unroutedTool",
            Self::InvalidLimits | Self::WorkerFailed => "executionFailed",
        }
    }
}

impl std::fmt::Display for ExecutionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Closed => "The runtime is shutting down.",
            Self::Busy => "Too many requests are pending; retry shortly.",
            Self::DuplicateRequest => "A request with this id is already running.",
            Self::InvalidLimits => "The runtime limits are invalid.",
            Self::Cancelled => "The request was cancelled.",
            Self::Timeout => "Tool call timed out.",
            Self::UnroutedTool => "No route executes this tool.",
            Self::WorkerFailed => "The tool worker failed.",
        })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ExecutionContext {
    pub cancellation: CancellationToken,
    pub deadline: Instant,
    /// This query's share of the cores for parallel directory walks, set by
    /// the batch budget when several queries walk at once. `None` = all cores.
    pub(crate) walk_threads: Option<u32>,
    /// This row's share of the response page (an explicit `responseLength`,
    /// else the runtime's `auto_page_chars`) that a streamed page must
    /// fit, set by the engine per row. `None` = no window; pages take their
    /// default size.
    pub(crate) response_window: Option<usize>,
    /// The GitHub credential the host supplied with this request
    /// ([`RequestAdmission::with_github_token`]). When set, every GitHub call
    /// of the request uses only it: no env, stored, or `gh` credential.
    pub(crate) github_credential: Option<ResolvedCredential>,
}

impl ExecutionContext {
    /// The characters a row's own pages fill: its share of the response page
    /// ([`Self::response_window`]), else the automatic page.
    #[must_use]
    pub(crate) fn row_page_chars(&self, auto_page_chars: usize) -> usize {
        self.response_window.unwrap_or(auto_page_chars)
    }

    pub fn check(&self) -> Result<(), ExecutionError> {
        if Instant::now() >= self.deadline {
            return Err(ExecutionError::Timeout);
        }
        if self.cancellation.is_cancelled() {
            return Err(ExecutionError::Cancelled);
        }
        Ok(())
    }
}

#[derive(Default)]
struct State {
    closed: bool,
    requests: HashMap<String, CancellationToken>,
}

struct Inner {
    state: Mutex<State>,
    slots: Arc<Semaphore>,
    light_slots: Arc<Semaphore>,
    drained: Notify,
    limits: RequestLimits,
}

#[derive(Clone)]
pub struct RequestRuntime {
    inner: Arc<Inner>,
}

struct RequestGuard {
    inner: Arc<Inner>,
    id: String,
}

/// Synchronous admission closes the gap between an interface call and first poll.
/// Dropping an unstarted admission releases its bounded slot immediately.
pub struct RequestAdmission {
    guard: RequestGuard,
    context: ExecutionContext,
    cost: Cost,
}

impl RequestAdmission {
    /// Run this request on the slots of `cost` (default [`Cost::Walk`]).
    #[must_use]
    pub(crate) fn with_cost(mut self, cost: Cost) -> Self {
        self.cost = cost;
        self
    }

    /// Carry the caller's GitHub token on this request as a secret override.
    pub(crate) fn with_github_token(mut self, token: SecretString) -> Self {
        self.context.github_credential =
            Some(ResolvedCredential::new(token, CredentialSource::Override));
        self
    }
    pub(crate) fn github_credential(&self) -> Option<&ResolvedCredential> {
        self.context.github_credential.as_ref()
    }
}

impl Drop for RequestGuard {
    fn drop(&mut self) {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.requests.remove(&self.id);
        self.inner.drained.notify_waiters();
    }
}

impl RequestRuntime {
    pub fn new(limits: RequestLimits) -> Result<Self, ExecutionError> {
        if limits.concurrency == 0
            || limits.light_concurrency == 0
            || limits.pending < limits.concurrency
            || limits.timeout.is_zero()
            || limits.timeout > MAX_TIMEOUT
        {
            return Err(ExecutionError::InvalidLimits);
        }
        Ok(Self {
            inner: Arc::new(Inner {
                state: Mutex::new(State::default()),
                slots: Arc::new(Semaphore::new(limits.concurrency)),
                light_slots: Arc::new(Semaphore::new(limits.light_concurrency)),
                drained: Notify::new(),
                limits,
            }),
        })
    }

    pub fn cancel(&self, request_id: &str) -> bool {
        let state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if let Some(token) = state.requests.get(request_id) {
            token.cancel();
            true
        } else {
            false
        }
    }

    pub fn is_closed(&self) -> bool {
        self.inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .closed
    }

    pub(crate) fn active_requests(&self) -> usize {
        self.inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .requests
            .len()
    }

    pub fn begin_close(&self) {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.closed = true;
        for token in state.requests.values() {
            token.cancel();
        }
    }

    /// Completes only after queued and running work releases its resources.
    pub async fn close(&self) {
        self.begin_close();
        loop {
            let notified = self.inner.drained.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.active_requests() == 0 {
                return;
            }
            notified.await;
        }
    }

    pub fn admit(&self, request_id: String) -> Result<RequestAdmission, ExecutionError> {
        let token = CancellationToken::new();
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if state.closed {
            return Err(ExecutionError::Closed);
        }
        if state.requests.contains_key(&request_id) {
            return Err(ExecutionError::DuplicateRequest);
        }
        if state.requests.len() >= self.inner.limits.pending {
            return Err(ExecutionError::Busy);
        }
        let deadline = Instant::now()
            .checked_add(self.inner.limits.timeout)
            .ok_or(ExecutionError::InvalidLimits)?;
        state.requests.insert(request_id.clone(), token.clone());
        Ok(RequestAdmission {
            guard: RequestGuard {
                inner: self.inner.clone(),
                id: request_id,
            },
            context: ExecutionContext {
                cancellation: token,
                deadline,
                walk_threads: None,
                response_window: None,
                github_credential: None,
            },
            cost: Cost::Walk,
        })
    }

    pub(crate) async fn execute_blocking_admitted<T, F>(
        &self,
        admission: RequestAdmission,
        work: F,
    ) -> Result<T, ExecutionError>
    where
        T: Send + 'static,
        F: FnOnce(ExecutionContext) -> Result<T, ExecutionError> + Send + 'static,
    {
        if !Arc::ptr_eq(&self.inner, &admission.guard.inner) {
            return Err(ExecutionError::WorkerFailed);
        }
        let RequestAdmission {
            guard,
            context,
            cost,
        } = admission;
        let slots = match cost {
            Cost::Walk => &self.inner.slots,
            Cost::Light => &self.inner.light_slots,
        };
        let token = context.cancellation.clone();
        let _cancel_on_drop = token.clone().drop_guard();
        let deadline = tokio::time::Instant::from_std(context.deadline);
        let permit = tokio::select! {
            biased;
            _ = token.cancelled() => return Err(ExecutionError::Cancelled),
            _ = tokio::time::sleep_until(deadline) => return Err(ExecutionError::Timeout),
            permit = slots.clone().acquire_owned() => permit.map_err(|_| ExecutionError::Closed)?,
        };
        let mut task = tokio::task::spawn_blocking(octocode_io::notice::carried(move || {
            let _guard = guard;
            let _permit = permit;
            context.check()?;
            let result = work(context.clone());
            context.check()?;
            result
        }));
        let reason = tokio::select! {
            biased;
            result = &mut task => return result.map_err(|_| ExecutionError::WorkerFailed)?,
            _ = token.cancelled() => ExecutionError::Cancelled,
            _ = tokio::time::sleep_until(deadline) => ExecutionError::Timeout,
        };
        // A blocking operation cannot be aborted safely. Signal it and join;
        // never report cleanup complete while it still owns file/process state.
        token.cancel();
        let _ = task.await;
        Err(reason)
    }
}

#[cfg(test)]
mod tests;
