mod cancellation;
mod eviction;
mod keys;
mod reuse;
mod starts;

use super::pool_client::{ClientFuture, PoolClient};
use super::start_guards::{StartCancellationGuard, StopOnDrop};
use super::state::*;
use super::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use tokio::sync::Mutex;
use tokio::sync::oneshot;
use tokio::time::Duration;

#[derive(Clone)]
struct FakeClient {
    id: usize,
    alive: Arc<AtomicBool>,
    health_gate: Arc<Mutex<Option<oneshot::Receiver<()>>>>,
    health_checks: Arc<AtomicUsize>,
    stops: Arc<AtomicUsize>,
    busy: Arc<AtomicBool>,
    resumes: Arc<AtomicUsize>,
    resume_gate: Arc<Mutex<Option<oneshot::Receiver<()>>>>,
    resident: Arc<AtomicU64>,
    config_changed: Arc<AtomicBool>,
}

impl FakeClient {
    fn new(id: usize) -> Self {
        Self {
            id,
            alive: Arc::new(AtomicBool::new(true)),
            health_gate: Arc::new(Mutex::new(None)),
            health_checks: Arc::new(AtomicUsize::new(0)),
            stops: Arc::new(AtomicUsize::new(0)),
            busy: Arc::new(AtomicBool::new(false)),
            resumes: Arc::new(AtomicUsize::new(0)),
            resume_gate: Arc::new(Mutex::new(None)),
            resident: Arc::new(AtomicU64::new(0)),
            config_changed: Arc::new(AtomicBool::new(false)),
        }
    }

    fn resident(self, bytes: u64) -> Self {
        self.resident.store(bytes, Ordering::SeqCst);
        self
    }
}

impl std::fmt::Debug for FakeClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FakeClient")
            .field("id", &self.id)
            .finish()
    }
}

impl PoolClient for FakeClient {
    fn alive(&self) -> ClientFuture<bool> {
        let alive = Arc::clone(&self.alive);
        let gate = Arc::clone(&self.health_gate);
        let checks = Arc::clone(&self.health_checks);
        Box::pin(async move {
            checks.fetch_add(1, Ordering::SeqCst);
            if let Some(receiver) = gate.lock().await.take() {
                let _ = receiver.await;
            }
            alive.load(Ordering::SeqCst)
        })
    }

    fn busy(&self) -> bool {
        self.busy.load(Ordering::SeqCst)
    }

    fn resident_bytes(&self) -> u64 {
        self.resident.load(Ordering::SeqCst)
    }

    fn stop(&self) -> ClientFuture<()> {
        let alive = Arc::clone(&self.alive);
        let stops = Arc::clone(&self.stops);
        Box::pin(async move {
            alive.store(false, Ordering::SeqCst);
            stops.fetch_add(1, Ordering::SeqCst);
        })
    }

    fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.alive, &other.alive)
    }

    fn resume(&self) -> ClientFuture<()> {
        let resumes = Arc::clone(&self.resumes);
        let gate = Arc::clone(&self.resume_gate);
        Box::pin(async move {
            resumes.fetch_add(1, Ordering::SeqCst);
            if let Some(receiver) = gate.lock().await.take() {
                let _ = receiver.await;
            }
        })
    }

    fn project_config_changed(&self) -> ClientFuture<bool> {
        let changed = self.config_changed.load(Ordering::SeqCst);
        Box::pin(async move { changed })
    }
}

fn pool(max_entries: usize, idle_timeout_ms: u64) -> GenericPool<FakeClient> {
    GenericPool::new(LspPoolOptions {
        max_entries,
        idle_timeout_ms,
        max_resident_bytes: 0,
        ..LspPoolOptions::default()
    })
}

fn pool_with_memory_budget(max_resident_bytes: u64) -> GenericPool<FakeClient> {
    GenericPool::new(LspPoolOptions {
        max_entries: 8,
        idle_timeout_ms: 60_000,
        max_resident_bytes,
        ..LspPoolOptions::default()
    })
}
