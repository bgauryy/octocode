use futures_util::FutureExt;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{Notify, Semaphore};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug)]
pub struct RuntimeLimits {
    pub concurrency: usize,
    pub pending: usize,
    pub timeout: Duration,
    pub output_bytes: usize,
}

impl Default for RuntimeLimits {
    fn default() -> Self {
        Self {
            concurrency: 4,
            pending: 64,
            timeout: Duration::from_secs(60),
            output_bytes: 16_000,
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
    WorkerFailed,
}

#[derive(Clone, Debug)]
pub struct ExecutionContext {
    pub cancellation: CancellationToken,
    pub deadline: Instant,
    pub output_bytes: usize,
}

impl ExecutionContext {
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
    drained: Notify,
    limits: RuntimeLimits,
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
    pub fn new(limits: RuntimeLimits) -> Result<Self, ExecutionError> {
        if limits.concurrency == 0
            || limits.pending < limits.concurrency
            || limits.timeout.is_zero()
            || limits.output_bytes == 0
        {
            return Err(ExecutionError::InvalidLimits);
        }
        Ok(Self {
            inner: Arc::new(Inner {
                state: Mutex::new(State::default()),
                slots: Arc::new(Semaphore::new(limits.concurrency)),
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

    pub fn active_requests(&self) -> usize {
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
        state.requests.insert(request_id.clone(), token.clone());
        Ok(RequestAdmission {
            guard: RequestGuard {
                inner: self.inner.clone(),
                id: request_id,
            },
            context: ExecutionContext {
                cancellation: token,
                deadline: Instant::now() + self.inner.limits.timeout,
                output_bytes: self.inner.limits.output_bytes,
            },
        })
    }

    /// Async work owns its resources in the future; cancellation drops that future
    /// before releasing admission. Spawned child tasks must be joined by their owner.
    pub async fn execute_async<T, F, Fut>(
        &self,
        request_id: String,
        work: F,
    ) -> Result<T, ExecutionError>
    where
        F: FnOnce(ExecutionContext) -> Fut,
        Fut: std::future::Future<Output = Result<T, ExecutionError>>,
    {
        let RequestAdmission {
            guard: _guard,
            context,
        } = self.admit(request_id)?;
        let token = context.cancellation.clone();
        let _cancel_on_drop = token.clone().drop_guard();
        let deadline = tokio::time::Instant::from_std(context.deadline);
        let _permit = tokio::select! {
            biased;
            _ = token.cancelled() => return Err(ExecutionError::Cancelled),
            _ = tokio::time::sleep_until(deadline) => return Err(ExecutionError::Timeout),
            permit = self.inner.slots.clone().acquire_owned() => permit.map_err(|_| ExecutionError::Closed)?,
        };
        context.check()?;
        let future =
            std::panic::AssertUnwindSafe(async { work(context.clone()).await }).catch_unwind();
        tokio::pin!(future);
        let result = tokio::select! {
            biased;
            _ = token.cancelled() => Err(ExecutionError::Cancelled),
            _ = tokio::time::sleep_until(deadline) => Err(ExecutionError::Timeout),
            result = &mut future => result.map_err(|_| ExecutionError::WorkerFailed)?,
        };
        context.check()?;
        result
    }

    pub async fn execute_blocking<T, F>(
        &self,
        request_id: String,
        work: F,
    ) -> Result<T, ExecutionError>
    where
        T: Send + 'static,
        F: FnOnce(ExecutionContext) -> Result<T, ExecutionError> + Send + 'static,
    {
        self.execute_blocking_admitted(self.admit(request_id)?, work)
            .await
    }

    pub async fn execute_blocking_admitted<T, F>(
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
        let permit = tokio::select! {
            biased;
            _ = token.cancelled() => return Err(ExecutionError::Cancelled),
            _ = tokio::time::sleep_until(deadline) => return Err(ExecutionError::Timeout),
            permit = self.inner.slots.clone().acquire_owned() => permit.map_err(|_| ExecutionError::Closed)?,
        };
        let mut task = tokio::task::spawn_blocking(move || {
            let _guard = guard;
            let _permit = permit;
            context.check()?;
            let result = work(context.clone());
            context.check()?;
            result
        });
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
