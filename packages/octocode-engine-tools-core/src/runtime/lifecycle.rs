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

    pub async fn execute_blocking<T, F>(
        &self,
        request_id: String,
        work: F,
    ) -> Result<T, ExecutionError>
    where
        T: Send + 'static,
        F: FnOnce(ExecutionContext) -> Result<T, ExecutionError> + Send + 'static,
    {
        let token = CancellationToken::new();
        let _cancel_on_drop = token.clone().drop_guard();
        let guard = {
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
            RequestGuard {
                inner: self.inner.clone(),
                id: request_id,
            }
        };
        let context = ExecutionContext {
            cancellation: token.clone(),
            deadline: Instant::now() + self.inner.limits.timeout,
            output_bytes: self.inner.limits.output_bytes,
        };
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
