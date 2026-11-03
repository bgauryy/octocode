//! Retained continuation state. Each store has one authoritative location:
//! disk for persistent sessions, or process memory for memory-only sessions.
//! Fresh tool requests never use this store to reuse computed results.
mod disk;
pub(crate) use disk::Disk;

use octocode_io::memo::{Memo, MemoryBudget};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::fs;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Issued work remains reachable while pages are being consumed.
pub(crate) const CONTINUATION_IDLE: Duration = Duration::from_secs(600);
pub(crate) const CONTINUATION_RETENTION: CacheClass = CacheClass::Idle {
    ttl: CONTINUATION_IDLE,
};

/// The bytes every in-process store holds together: each tool's page memo
/// ([`crate::tools::page_memo::PageMemo`]) and the memory tier of every
/// [`Store`]. Past it the least recently used entries of all of them go
/// first, and a value larger than all of it is not kept.
pub(crate) static MEMORY: MemoryBudget = MemoryBudget::new(memory_budget);

/// The budget's floor: what a host whose memory cannot be read, or a small
/// one, still gets, so one large scan's continuation stays retained.
pub(crate) const MIN_MEMORY_BUDGET: usize = 512 * 1024 * 1024;
/// The budget's ceiling: retained continuation state is a convenience, so a
/// large host keeps most of its memory for language servers and its users.
pub(crate) const MAX_MEMORY_BUDGET: usize = 2 * 1024 * 1024 * 1024;

fn memory_budget() -> usize {
    memory_budget_for(octocode_engine::host::memory_mb())
}

/// An eighth of `host_mb` MiB, within [`MIN_MEMORY_BUDGET`] and
/// [`MAX_MEMORY_BUDGET`]; the floor when the host's memory is unknown.
pub(crate) fn memory_budget_for(host_mb: Option<u64>) -> usize {
    host_mb.map_or(MIN_MEMORY_BUDGET, |mb| {
        usize::try_from(mb.saturating_mul(1024 * 1024) / 8)
            .unwrap_or(usize::MAX)
            .clamp(MIN_MEMORY_BUDGET, MAX_MEMORY_BUDGET)
    })
}

/// Persistent store directories: construction and maintenance share this inventory.
#[derive(Clone, Copy)]
pub(crate) enum Directory {
    ResponsePages,
    Continuations,
    SectionAssessments,
    LspAnswers,
    GraphRows,
}
impl Directory {
    pub(crate) const ALL: [Self; 5] = [
        Self::ResponsePages,
        Self::Continuations,
        Self::SectionAssessments,
        Self::LspAnswers,
        Self::GraphRows,
    ];
    pub(crate) fn path(self, home: &std::path::Path) -> PathBuf {
        home.join("tmp").join(match self {
            Self::ResponsePages => "response-pages",
            Self::Continuations => "continuations",
            Self::SectionAssessments => "section-assessments",
            Self::LspAnswers => "lsp-answers",
            Self::GraphRows => "graph-rows",
        })
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
    fn ttl(self) -> Option<Duration> {
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

/// A store's bounds. Memory counts against [`MEMORY`] alone, which refuses
/// an entry heavier than the whole budget; the disk tier has this one budget.
#[derive(Clone, Debug)]
pub struct CacheConfig {
    /// One store directory's budget, pruned oldest-first by modification
    /// time. It is also the largest single entry the disk accepts.
    pub disk_bytes: u64,
}

impl CacheConfig {
    /// The policy every retained-state store runs under: its disk budget is
    /// the memory budget ([`MEMORY`]), so an entry that fits in memory also
    /// fits on disk and a larger host retains larger work.
    pub(crate) fn derived() -> Self {
        Self::for_memory(MEMORY.limit())
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
            memory: disk.is_none().then(|| Memo::new(&MEMORY)),
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
    /// continuation must check this before advertising saved pages.
    pub(crate) fn put_confirmed(
        &self,
        key: CacheKey,
        value: V,
        bytes: usize,
        class: CacheClass,
    ) -> (Arc<V>, bool) {
        if !class.is_live(Duration::ZERO) {
            return (Arc::new(value), false);
        }
        if let Some(disk) = &self.disk {
            let saved = disk.write::<V, C>(&key, &value, class, &self.config);
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
