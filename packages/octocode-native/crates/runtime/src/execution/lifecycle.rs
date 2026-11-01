use octocode_contract_types::ErrorCode;
use octocode_github::{CredentialSource, ResolvedCredential};
use secrecy::SecretString;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

/// Longest per-request timeout a runtime accepts. Deadlines are
/// `Instant::now() + timeout`; a bound keeps that sum representable on every
/// platform instead of panicking at admission (M4).
pub const MAX_TIMEOUT: Duration = Duration::from_secs(24 * 60 * 60);

/// Admission bounds. Every admitted request runs at once: there is no run
/// queue, so an independent call never waits for another to finish. Their
/// fan-outs share the cores through the process CPU budget
/// ([`octocode_engine::search::cpu`]), and `pending` refuses work past it.
#[derive(Clone, Debug)]
pub struct RequestLimits {
    /// Admitted requests (all running) before `busy`.
    pub pending: usize,
    pub timeout: Duration,
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
    pub const fn code(self) -> ErrorCode {
        match self {
            Self::Closed => ErrorCode::Closed,
            Self::Busy => ErrorCode::Busy,
            Self::DuplicateRequest => ErrorCode::DuplicateRequest,
            Self::Cancelled => ErrorCode::Cancelled,
            Self::Timeout => ErrorCode::Timeout,
            Self::UnroutedTool => ErrorCode::UnroutedTool,
            Self::InvalidLimits | Self::WorkerFailed => ErrorCode::ExecutionFailed,
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
    /// This row's share of the response page (an explicit `responseChars`,
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
}

impl RequestAdmission {
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
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.requests.remove(&self.id);
        self.inner.drained.notify_waiters();
    }
}

impl RequestRuntime {
    pub fn new(limits: RequestLimits) -> Result<Self, ExecutionError> {
        if limits.pending == 0 || limits.timeout.is_zero() || limits.timeout > MAX_TIMEOUT {
            return Err(ExecutionError::InvalidLimits);
        }
        Ok(Self {
            inner: Arc::new(Inner {
                state: Mutex::new(State::default()),
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
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(token) = state.requests.get(request_id) {
            token.cancel();
            true
        } else {
            false
        }
    }

    pub(crate) fn active_requests(&self) -> usize {
        self.inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .requests
            .len()
    }

    pub fn begin_close(&self) {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
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
            .unwrap_or_else(std::sync::PoisonError::into_inner);
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
                response_window: None,
                github_credential: None,
            },
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
        let RequestAdmission { guard, context } = admission;
        let token = context.cancellation.clone();
        let _cancel_on_drop = token.clone().drop_guard();
        let deadline = tokio::time::Instant::from_std(context.deadline);
        // A request cancelled or expired before it starts never runs work.
        let mut task = tokio::task::spawn_blocking(octocode_io::notice::carried(move || {
            let _guard = guard;
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
