//! Async provider adapters use the same store without blocking their executor.
use super::*;

const IO_CONCURRENCY: u32 = 8;
static IO: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(IO_CONCURRENCY as usize);

/// Wait for cancelled requests' blocking cache jobs too, holding them out during purge.
pub(crate) async fn drain()
-> Result<tokio::sync::SemaphorePermit<'static>, tokio::sync::AcquireError> {
    IO.acquire_many(IO_CONCURRENCY).await
}

async fn run<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let permit = IO.acquire().await.ok()?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        work()
    })
    .await
    .ok()
}

impl<V: Send + Sync + 'static, C: DiskCodec<V> + Send + Sync + 'static> Store<V, C> {
    pub(crate) async fn get_async(self: &Arc<Self>, key: CacheKey) -> Option<Cached<V>> {
        if let Some(hit) = self.read_memory(&key) {
            return Some(hit);
        }
        self.disk.as_ref()?;
        let store = Arc::clone(self);
        run(move || store.read_disk(&key)).await.flatten()
    }
    pub(crate) async fn put_async(
        self: &Arc<Self>,
        key: CacheKey,
        value: V,
        bytes: usize,
        class: CacheClass,
    ) {
        if self.disk.is_none() {
            self.put(key, value, bytes, class);
            return;
        }
        let store = Arc::clone(self);
        run(move || {
            store.put(key, value, bytes, class);
        })
        .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    static HEARTBEAT: AtomicBool = AtomicBool::new(false);
    static OBSERVED: AtomicBool = AtomicBool::new(false);
    struct SlowCodec;
    impl DiskCodec<u8> for SlowCodec {
        fn encode(value: &u8) -> Option<Vec<u8>> {
            std::thread::sleep(Duration::from_millis(30));
            OBSERVED.store(HEARTBEAT.load(Ordering::SeqCst), Ordering::SeqCst);
            Some(vec![*value])
        }
        fn decode(bytes: &[u8]) -> Option<u8> {
            bytes.first().copied()
        }
    }
    #[tokio::test(flavor = "current_thread")]
    async fn persistent_provider_cache_yields_the_executor_during_disk_work() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::<u8, SlowCodec>::new(
            CacheConfig::default(),
            Some(dir.path().into()),
        ));
        let key = CacheKey {
            namespace: "heartbeat".into(),
            scope: String::new(),
            resource: "answer".into(),
        };
        tokio::join!(
            store.put_async(key.clone(), 42, 1, CacheClass::Immutable),
            async {
                HEARTBEAT.store(true, Ordering::SeqCst);
            }
        );
        assert!(
            OBSERVED.load(Ordering::SeqCst),
            "cache encoding blocked the async executor"
        );
        store.clear_memory();
        assert_eq!(*store.get_async(key).await.unwrap().value, 42);
    }
}
