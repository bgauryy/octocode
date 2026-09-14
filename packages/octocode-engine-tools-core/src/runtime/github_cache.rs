//! Provider cache partitions are minted by the transport after credential pinning.
use crate::cache::{BoundedCache, CacheConfig, CacheKey, CacheLookup, CachePartition};
use crate::providers::github::{
    CachePartition as ProviderPartition, CachedContent, ConditionalCache,
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Instant,
};

#[derive(Clone)]
pub(super) struct GitHubContentCache {
    cache: Arc<Mutex<BoundedCache<CachedContent>>>,
    revision: u64,
    disk: Option<PathBuf>,
}

impl GitHubContentCache {
    pub fn clear(&self) {
        self.cache
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .invalidate_all();
        if let Some(disk) = &self.disk {
            let _ = fs::remove_dir_all(disk);
            let _ = fs::create_dir_all(disk);
        }
    }
    pub fn new(config: CacheConfig, revision: u64, disk: Option<PathBuf>) -> Self {
        if let Some(disk) = &disk {
            let _ = fs::create_dir_all(disk);
        }
        Self {
            cache: Arc::new(Mutex::new(BoundedCache::new(config))),
            revision,
            disk,
        }
    }

    fn disk_file(&self, resource: &str) -> Option<PathBuf> {
        self.disk.as_ref().map(|dir| {
            let mut digest = Sha256::new();
            digest.update(resource.as_bytes());
            dir.join(format!("{:x}.json", digest.finalize()))
        })
    }

    fn read_disk(&self, resource: &str) -> Option<CachedContent> {
        let path = self.disk_file(resource)?;
        let bytes = fs::read(path).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    fn write_disk(&self, resource: &str, value: &CachedContent) {
        let Some(path) = self.disk_file(resource) else {
            return;
        };
        if let Ok(bytes) = serde_json::to_vec(value) {
            let _ = fs::write(path, bytes);
        }
    }

    fn key(partition: &ProviderPartition, resource: &str) -> CacheKey {
        CacheKey {
            namespace: "github-content".into(),
            resource: resource.into(),
            partition: CachePartition {
                endpoint: "provider-partition-v1".into(),
                credential_fingerprint: partition.0.clone(),
            },
        }
    }
}

impl ConditionalCache for GitHubContentCache {
    fn get<'a>(
        &'a self,
        partition: &'a ProviderPartition,
        key: &'a str,
    ) -> Pin<Box<dyn Future<Output = Option<CachedContent>> + Send + 'a>> {
        Box::pin(async move {
            match self.cache.lock().unwrap_or_else(|p| p.into_inner()).get(
                &Self::key(partition, key),
                self.revision,
                None,
                Instant::now(),
            ) {
                CacheLookup::Hit { value, .. } => Some((*value).clone()),
                CacheLookup::Miss(_) => self.read_disk(key),
            }
        })
    }

    fn put<'a>(
        &'a self,
        partition: &'a ProviderPartition,
        key: String,
        value: CachedContent,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            let key = Self::key(partition, &key);
            let bytes = value
                .bytes
                .capacity()
                .saturating_add(value.resolved_ref.capacity())
                .saturating_add(value.etag.as_ref().map_or(0, String::capacity))
                .saturating_add(key.resource.capacity())
                .saturating_add(key.partition.credential_fingerprint.capacity())
                .saturating_add(std::mem::size_of::<CachedContent>());
            self.write_disk(&key.resource, &value);
            self.cache.lock().unwrap_or_else(|p| p.into_inner()).insert(
                key,
                value,
                bytes,
                self.revision,
                Instant::now(),
            );
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn refuses_oversized_bodies_and_never_crosses_provider_partitions() {
        let cache = GitHubContentCache::new(
            CacheConfig {
                max_bytes: 1024,
                ..Default::default()
            },
            7,
            None,
        );
        let one = ProviderPartition("endpoint/credential/one".into());
        let two = ProviderPartition("endpoint/credential/two".into());
        let content = CachedContent {
            bytes: b"private source".to_vec(),
            etag: Some("v1".into()),
            resolved_ref: "sha".into(),
        };
        cache.put(&one, "file".into(), content.clone()).await;
        assert_eq!(cache.get(&one, "file").await, Some(content));
        assert_eq!(cache.get(&two, "file").await, None);
        cache
            .put(
                &one,
                "large".into(),
                CachedContent {
                    bytes: vec![0; 2048],
                    etag: None,
                    resolved_ref: "sha".into(),
                },
            )
            .await;
        assert_eq!(cache.get(&one, "large").await, None);
    }
}
