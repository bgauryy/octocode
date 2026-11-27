//! What the rows of one call share. The runtime runs a call's rows at once,
//! each with its own transport and budget, so rows that need the same
//! upstream answer would each ask for it: two versions of one package listing
//! the same advisories, a distribution package with and without its release,
//! the same registry document or release tag. A call's share makes each such
//! read once (a single flight: the first row to ask makes the request, every
//! other row asking for it, then or later in the call, takes that answer) and
//! holds the rows' requests to an advisory database under one bound. Nothing
//! in it outlives the call: answers kept across calls stay the saved answers'
//! own. It sits with the providers, below both tools, so each tool imports it
//! and it imports neither.
use super::{ArtifactError, ArtifactHttpResponse};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use tokio::sync::{OnceCell, Semaphore, SemaphorePermit};

/// The share of one call's rows. The runtime makes one per call of
/// artifactSearch or securityCheck and hands it to every row; a row run alone
/// makes its own.
#[derive(Default)]
pub struct CallShare {
    /// Upstream requests by what they ask: a registry read (origin,
    /// credential, URL, media type and validator), an OSV query body or
    /// record read. The raw answer is shared; each row reads it as its own.
    pub(crate) requests: Flights<Result<ArtifactHttpResponse, ArtifactError>>,
    /// Facts derived from one immutable registry resource (a crate
    /// release's VCS info).
    pub(crate) facts: Flights<Option<String>>,
    /// Release-tag, tag-list, commit and tree checks on GitHub.
    pub(crate) tags: super::release_ref::TagShare,
    /// The call's bound on requests to an advisory database in flight,
    /// sized by its first asker.
    bound: OnceLock<Semaphore>,
}

impl CallShare {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A place among the call's advisory-database requests in flight, of
    /// `permits` places, held while one request is.
    pub(crate) async fn advisory_permit(&self, permits: usize) -> Option<SemaphorePermit<'_>> {
        self.bound
            .get_or_init(|| Semaphore::new(permits))
            .acquire()
            .await
            .ok()
    }
}

/// Reads made once per call, by key: the first asker runs its read, and every
/// other asker of the key waits for that answer and gets a copy. A read whose
/// asker gave up (its future dropped) leaves the key to the next asker.
pub(crate) struct Flights<T> {
    cells: Mutex<HashMap<String, Arc<OnceCell<T>>>>,
}

impl<T> Default for Flights<T> {
    fn default() -> Self {
        Self {
            cells: Mutex::new(HashMap::new()),
        }
    }
}

impl<T: Clone> Flights<T> {
    /// `read`'s answer under `key`, read once in this call.
    pub(crate) async fn once(&self, key: String, read: impl Future<Output = T>) -> T {
        let cell = self
            .cells
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(key)
            .or_default()
            .clone();
        cell.get_or_init(|| read).await.clone()
    }
}

#[cfg(test)]
#[path = "share_tests.rs"]
mod tests;
