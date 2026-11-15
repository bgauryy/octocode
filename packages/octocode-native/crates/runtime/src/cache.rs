//! Retained continuation state. Each store has one authoritative location:
//! disk for persistent sessions, or process memory for memory-only sessions.
//! Fresh tool requests never use this store to reuse computed results; its
//! only fresh-request reader is [`crate::providers::saved_answers`], whose
//! provider answers are revalidated or reused as their origin states. The
//! in-memory memos keyed by content digest ([`crate::tools::page_memo`]:
//! outlines, parsed declarations, name mentions) are the other results a
//! fresh request may reuse.
mod disk;
pub(crate) use disk::Disk;

use octocode_io::memo::Memo;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::fs;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Issued work remains reachable while pages are being consumed.
pub(crate) const CONTINUATION_IDLE: Duration = octocode_engine::memory::IDLE;
pub(crate) const CONTINUATION_RETENTION: CacheClass = CacheClass::Idle {
    ttl: CONTINUATION_IDLE,
};

/// Persistent store directories: construction and maintenance share this inventory.
#[derive(Clone, Copy)]
pub(crate) enum Directory {
    ResponsePages,
    Continuations,
    SectionAssessments,
    LspAnswers,
    GraphRows,
    ProviderAnswers,
}
impl Directory {
    /// Store directories a published release wrote and no code reads or
    /// writes any more, relative to the home: the maintenance sweep and
    /// `cache clear` remove them. `tmp/response` is the result cache of
    /// release 20.1.0. A layout bump that ships moves the old directory here.
    pub(crate) const RETIRED: [&'static str; 2] = ["tmp/response", "cache/provider-answers-v2"];

    /// [`Self::RETIRED`] under `home`.
    pub(crate) fn retired(home: &std::path::Path) -> impl Iterator<Item = PathBuf> {
        Self::RETIRED.into_iter().map(|name| home.join(name))
    }

    pub(crate) const ALL: [Self; 6] = [
        Self::ResponsePages,
        Self::Continuations,
        Self::SectionAssessments,
        Self::LspAnswers,
        Self::GraphRows,
        Self::ProviderAnswers,
    ];
    pub(crate) fn path(self, home: &std::path::Path) -> PathBuf {
        match self {
            Self::ResponsePages => home.join("tmp/response-pages"),
            Self::Continuations => home.join("tmp/continuations"),
            Self::SectionAssessments => home.join("tmp/section-assessments"),
            Self::LspAnswers => home.join("tmp/lsp-answers"),
            Self::GraphRows => home.join("tmp/graph-rows"),
            // Outlives its session, so not under `tmp`. The name carries the
            // layout's version: a bump moves the old name to [`Self::RETIRED`].
            Self::ProviderAnswers => home.join("cache/provider-answers-v3"),
        }
    }
}

/// An entry's identity. `scope` is opaque to the store: a provider passes
/// its credential [`octocode_github::Partition::identity`], a local memo
/// whatever separates its entries (a path, a parser, a files digest).
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CacheKey {
    pub namespace: String,
    pub scope: String,
    pub resource: String,
}

/// How an entry ages; the writer chooses it, with its window, at `put_confirmed`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CacheClass {
    /// Keyed by a content digest or a commit SHA: never expires and is always
    /// fresh. Only the byte budget evicts it.
    Immutable,
    /// Expires `ttl` after it is stored.
    Volatile { ttl: Duration },
    /// Expires `ttl` after its last use: every read restarts its lifetime in
    /// both tiers, so retained work lives while a reader keeps using it.
    Idle { ttl: Duration },
}

impl CacheClass {
    pub(crate) fn ttl(self) -> Option<Duration> {
        match self {
            Self::Volatile { ttl } | Self::Idle { ttl } => Some(ttl),
            Self::Immutable => None,
        }
    }

    /// The lifetime a read restarts, for a class whose age counts from its
    /// last use.
    fn idle(self) -> Option<Duration> {
        match self {
            Self::Idle { ttl } => Some(ttl),
            Self::Immutable | Self::Volatile { .. } => None,
        }
    }

    /// Whether retained state is still within its declared lifetime.
    fn is_live(self, age: Duration) -> bool {
        match self {
            Self::Immutable => true,
            Self::Volatile { ttl } | Self::Idle { ttl } => age < ttl,
        }
    }
}

/// A store's bounds. Memory counts against [`octocode_engine::memory::MEMORY`] alone, which refuses
/// an entry heavier than the whole budget; the disk tier has this one budget.
#[derive(Clone, Debug)]
pub struct CacheConfig {
    /// One store directory's budget, pruned oldest-first by modification
    /// time. It is also the largest single entry the disk accepts.
    pub disk_bytes: u64,
}

impl CacheConfig {
    /// The policy every retained-state store runs under: its disk budget is
    /// the memory budget ([`octocode_engine::memory::MEMORY`]), so an entry that fits in memory also
    /// fits on disk and a larger host retains larger work.
    pub(crate) fn derived() -> Self {
        Self::for_memory(octocode_engine::memory::MEMORY.limit())
    }

    fn for_memory(memory_bytes: usize) -> Self {
        Self {
            disk_bytes: memory_bytes as u64,
        }
    }
}

/// Retained state retrieved from its authoritative store.
#[derive(Clone, Debug)]
pub struct Cached<V> {
    pub value: Arc<V>,
}

/// How a store lays its values out on disk, after the entry header.
pub trait DiskCodec<V> {
    fn encode(value: &V) -> Option<Vec<u8>>;
    fn decode(bytes: &[u8]) -> Option<V>;
}

/// A value as JSON: the layout of every store whose values are not bytes.
pub struct JsonCodec;

impl<V: Serialize + DeserializeOwned> DiskCodec<V> for JsonCodec {
    fn encode(value: &V) -> Option<Vec<u8>> {
        serde_json::to_vec(value).ok()
    }

    fn decode(bytes: &[u8]) -> Option<V> {
        serde_json::from_slice(bytes).ok()
    }
}

/// One authoritative state store; `C` lays values out on disk.
pub struct Store<V, C = JsonCodec> {
    config: CacheConfig,
    memory: Option<Memo<Arc<CacheKey>, V, CacheClass>>,
    disk: Option<Disk>,
    codec: PhantomData<C>,
}

impl<V: Send + Sync + 'static, C: DiskCodec<V>> Store<V, C> {
    /// `disk: None` keeps the store in this process only.
    pub(crate) fn new(config: CacheConfig, disk: Option<PathBuf>) -> Self {
        Self {
            memory: disk
                .is_none()
                .then(|| Memo::new(&octocode_engine::memory::MEMORY)),
            disk: disk.map(Disk::new),
            config,
            codec: PhantomData,
        }
    }

    pub(crate) fn get(&self, key: &CacheKey) -> Option<Cached<V>> {
        match &self.disk {
            Some(disk) => {
                let (value, _, _, _) = disk.read::<V, C>(key, &self.config)?;
                Some(Cached {
                    value: Arc::new(value),
                })
            }
            None => self.read_memory(key),
        }
    }

    fn read_memory(&self, key: &CacheKey) -> Option<Cached<V>> {
        let memory = self.memory.as_ref()?;
        let hit = memory.get(key)?;
        if !hit.meta.is_live(hit.age) {
            memory.remove(key);
            return None;
        }
        if let Some(ttl) = hit.meta.idle() {
            memory.touch(key, ttl);
        }
        Some(Cached { value: hit.value })
    }

    /// Store `value` (about `bytes` large) in the selected storage mode; an
    /// entry heavier than the memory budget, or than the disk budget on disk,
    /// is not stored at all. Returns
    /// the value, shared with the entry, and whether it was retained: a result
    /// continuation must check this before advertising saved pages. A store
    /// whose lock stays busy refuses it ([`disk::REUSE_LOCK_WAIT`]), so this is
    /// for what only later reuse reads.
    pub(crate) fn put_confirmed(
        &self,
        key: CacheKey,
        value: V,
        bytes: usize,
        class: CacheClass,
    ) -> (Arc<V>, bool) {
        let deadline = Instant::now() + disk::REUSE_LOCK_WAIT;
        self.put_until(deadline, key, value, bytes, class)
    }

    /// [`Self::put_confirmed`] for what its call's answer depends on: a busy
    /// store is waited out up to `deadline`, the request's own.
    pub(crate) fn put_until(
        &self,
        deadline: Instant,
        key: CacheKey,
        value: V,
        bytes: usize,
        class: CacheClass,
    ) -> (Arc<V>, bool) {
        if !class.is_live(Duration::ZERO) {
            return (Arc::new(value), false);
        }
        if let Some(disk) = &self.disk {
            let saved = disk.write::<V, C>(&key, &value, class, &self.config, deadline);
            return (Arc::new(value), saved);
        }
        let Some(memory) = &self.memory else {
            return (Arc::new(value), false);
        };
        memory.insert_full(
            Arc::new(key),
            value,
            class,
            bytes,
            class.ttl(),
            Instant::now(),
        )
    }

    /// Drop `key` from its authoritative store.
    pub(crate) fn remove(&self, key: &CacheKey) {
        if let Some(memory) = &self.memory {
            memory.remove(key);
        }
        if let Some(disk) = &self.disk {
            let _ = fs::remove_file(disk.file(key));
        }
    }

    /// Whether `key` is retained, counting as a use of it.
    pub(crate) fn refresh(&self, key: &CacheKey) -> bool {
        match &self.disk {
            Some(disk) => disk.touch(key),
            None => self.read_memory(key).is_some(),
        }
    }

    /// Give this store its own hard link to the file `source` holds under
    /// `key` ([`Disk::link`]). False leaves nothing to read back.
    pub(crate) fn link(&self, source: &Disk, key: &CacheKey) -> bool {
        self.disk
            .as_ref()
            .is_some_and(|disk| disk.link(source, key, &self.config))
    }

    /// The lock file of `key`'s entry; `None` in memory.
    pub(crate) fn entry_lock(&self, key: &CacheKey) -> Option<PathBuf> {
        self.disk.as_ref().map(|disk| disk.entry_lock(key))
    }

    /// Run `update` holding `key`'s entry lock, so a read-modify-write of the
    /// entry excludes writers in other processes sharing the directory; a
    /// store in memory has none, and its caller orders its own writers. `None`
    /// when the lock stays busy past [`disk::REUSE_LOCK_WAIT`] or cannot be taken.
    pub(crate) fn with_entry_lock<T>(
        &self,
        key: &CacheKey,
        update: impl FnOnce() -> T,
    ) -> Option<T> {
        use octocode_io::lock::{LOCK_POLL, lock_within, open_lock_file};
        let _lock = match self.entry_lock(key) {
            Some(path) => {
                octocode_io::atomic::create_private_dir_all(path.parent()?).ok()?;
                let file = open_lock_file(&path).ok()?;
                Some(lock_within(file, disk::REUSE_LOCK_WAIT, LOCK_POLL).ok()?)
            }
            None => None,
        };
        Some(update())
    }

    /// The value bytes of this store's link for `key`.
    pub(crate) fn read_link(&self, key: &CacheKey) -> Option<Vec<u8>> {
        self.disk.as_ref()?.read_link(key, &self.config)
    }

    pub(crate) fn clear_memory(&self) {
        if let Some(memory) = &self.memory {
            memory.clear();
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;

#[cfg(test)]
mod current_tests;

#[cfg(test)]
mod json_size_policy_tests;
