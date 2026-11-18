//! Drop guards that keep a cancelled or abandoned start from stranding a
//! language-server process or a stale in-flight registration.

use super::pool_client::PoolClient;
use super::state::{InFlight, State, inflight_is_current};
use std::future::Future;
use std::sync::Arc;
use tokio::runtime::Handle;
use tokio::sync::{Mutex, oneshot};

/// Spawn async cleanup from a `Drop`. Uses the runtime handle captured when
/// the guard was built (a `Drop` can run on a non-runtime thread, where
/// `Handle::try_current()` fails), falling back to the current runtime only
/// if none was captured.
fn spawn_cleanup<F>(runtime: Option<&Handle>, future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    if let Some(runtime) = runtime {
        runtime.spawn(future);
    } else if let Ok(runtime) = Handle::try_current() {
        runtime.spawn(future);
    }
}

pub(super) struct StartCancellationGuard<C: PoolClient> {
    pub(super) key: String,
    pub(super) state: Arc<Mutex<State<C>>>,
    pub(super) inflight: Arc<InFlight<C>>,
    pub(super) client: Option<C>,
    pub(super) armed: bool,
    pub(super) runtime: Option<Handle>,
}

impl<C: PoolClient> StartCancellationGuard<C> {
    pub(super) fn new(
        key: String,
        state: Arc<Mutex<State<C>>>,
        inflight: Arc<InFlight<C>>,
    ) -> Self {
        Self {
            key,
            state,
            inflight,
            client: None,
            armed: true,
            runtime: Handle::try_current().ok(),
        }
    }

    pub(super) fn track_client(&mut self, client: C) {
        self.client = Some(client);
    }

    pub(super) fn disarm(&mut self) {
        self.armed = false;
        self.client = None;
    }
}

impl<C: PoolClient> Drop for StartCancellationGuard<C> {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        self.inflight.cancel();
        let key = self.key.clone();
        let state = Arc::clone(&self.state);
        let inflight = Arc::clone(&self.inflight);
        let client = self.client.take();
        spawn_cleanup(self.runtime.as_ref(), async move {
            // Never remove a replacement installed by a retry for the same key.
            {
                let mut state = state.lock().await;
                if inflight_is_current(&state, &key, &inflight) {
                    state.inflight.remove(&key);
                }
            }
            if let Some(client) = client {
                client.stop().await;
            }
        });
    }
}

/// Completes once no caller waits for a supervised start any more: the
/// future awaiting [`run_cancellation_safe_start`] was dropped.
pub(super) struct Abandoned(oneshot::Receiver<()>);

impl Abandoned {
    /// Waits until the start's caller is gone.
    pub(super) async fn wait(self) {
        // Nothing is ever sent: the sender's drop is the signal.
        let _ = self.0.await;
    }
}

pub(super) async fn run_cancellation_safe_start<C, T, F, Fut>(
    client: C,
    start: F,
) -> std::result::Result<T, oneshot::error::RecvError>
where
    C: PoolClient,
    T: Send + 'static,
    F: FnOnce(Abandoned) -> Fut,
    Fut: Future<Output = T> + Send + 'static,
{
    // Dropping a raw start future can strand a spawned process before the
    // client publishes its child handle. Let startup finish in its supervisor;
    // a closed receiver then takes the normal full-client shutdown path. The
    // start learns through `Abandoned` that its caller left, so a wait inside
    // it that only progress bounds ends then instead of running on for no
    // one.
    let (attended, abandoned) = oneshot::channel::<()>();
    let future = start(Abandoned(abandoned));
    let (send, receive) = oneshot::channel();
    tokio::spawn(async move {
        let result = future.await;
        if send.send(result).is_err() {
            client.stop().await;
        }
    });
    let _attended = attended;
    receive.await
}
