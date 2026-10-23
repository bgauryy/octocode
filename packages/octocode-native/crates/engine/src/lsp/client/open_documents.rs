//! Bounded open-document bookkeeping and the on-disk comparison of what
//! was synced.

use super::snippets::read_bounded_regular_file;
use crate::stamp::SourceStamp;
use std::collections::{HashMap, VecDeque};
use std::path::Path;

/// Upper bound on documents kept in the `didOpen` lifecycle at once. Without a
/// cap, `open_docs` (and the server-side document set it mirrors) grows
/// monotonically over a long session until the server's RLIMIT_AS / Job memory
/// cap kills it. When exceeded, the least-recently-synced document is evicted
/// and a `didClose` is issued for it.
pub(super) const MAX_OPEN_DOCUMENTS: usize = 64;

/// What the last successful sync of a document sent: the file it was read
/// from, the digest of its text, and, once [`NativeLspClient::sync_open_documents`]
/// has proven that text is still the file's, the file's settled stamp.
#[derive(Clone, Debug)]
pub(super) struct Synced {
    path: String,
    digest: u64,
    stamp: Option<SourceStamp>,
}

/// One open document compared with its file on disk.
pub(super) enum DiskCheck {
    /// The file still holds the synced text; `stamp` lets a later check skip
    /// the read.
    Same {
        uri: String,
        digest: u64,
        stamp: Option<SourceStamp>,
    },
    /// The file now holds other text.
    Changed {
        uri: String,
        path: String,
        content: String,
        stamp: Option<SourceStamp>,
    },
    /// The file is gone, or no longer a readable UTF-8 file within the
    /// source bound.
    Gone { uri: String },
}

impl DiskCheck {
    /// `synced` against its file now: `None` when its stamp proves the file
    /// unchanged without reading it. The stat comes before the read, so a
    /// write racing the read leaves a stamp the next check sees changed.
    pub(super) fn of(uri: String, synced: Synced) -> Option<Self> {
        let Ok(meta) = std::fs::metadata(&synced.path) else {
            return Some(Self::Gone { uri });
        };
        if synced.stamp.is_some() && SourceStamp::of(&meta) == synced.stamp {
            return None;
        }
        let stamp = SourceStamp::settled(&meta, crate::stamp::settle_clock());
        let Some(content) =
            read_bounded_regular_file(Path::new(&synced.path), crate::lsp::MAX_LSP_SOURCE_BYTES)
                .ok()
        else {
            return Some(Self::Gone { uri });
        };
        Some(if content_digest(&content) == synced.digest {
            Self::Same {
                uri,
                digest: synced.digest,
                stamp,
            }
        } else {
            Self::Changed {
                uri,
                path: synced.path,
                content,
                stamp,
            }
        })
    }
}

/// Digest of a document's text, compared to skip a resync of text the
/// server already holds.
pub(super) fn content_digest(content: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

/// Bounded open-document lifecycle bookkeeping: `uri -> last sent version`
/// plus LRU recency. Capped so a long session cannot grow the open-document set
/// (and the mirrored server-side document memory) without bound. All state is
/// synchronous/pure so the open/evict decision is unit-testable without a real
/// server; the async `didClose` for an evicted URI is issued by the caller.
pub(super) struct OpenDocuments {
    versions: HashMap<String, i32>,
    /// What the last successful sync sent, per URI.
    synced: HashMap<String, Synced>,
    /// Recency order, least-recently-synced at the front.
    lru: VecDeque<String>,
    cap: usize,
}

impl OpenDocuments {
    pub(super) fn new(cap: usize) -> Self {
        Self {
            versions: HashMap::new(),
            synced: HashMap::new(),
            lru: VecDeque::new(),
            cap: cap.max(1),
        }
    }

    pub(super) fn clear(&mut self) {
        self.versions.clear();
        self.synced.clear();
        self.lru.clear();
    }

    pub(super) fn version(&self, uri: &str) -> Option<i32> {
        self.versions.get(uri).copied()
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.versions.len()
    }

    /// Reserve the next version for a sync of `uri`, refreshing its recency.
    /// Returns the version to send and, when the cap is exceeded, the URI of the
    /// evicted least-recently-used document (never the document being synced),
    /// which the caller must `didClose`.
    pub(super) fn reserve(&mut self, uri: &str) -> (i32, Option<String>) {
        self.synced.remove(uri);
        let version = self.versions.get(uri).copied().unwrap_or(0) + 1;
        self.versions.insert(uri.to_owned(), version);
        self.touch(uri);
        let evicted = if self.versions.len() > self.cap {
            self.evict_lru(uri)
        } else {
            None
        };
        (version, evicted)
    }

    fn touch(&mut self, uri: &str) {
        self.lru.retain(|candidate| candidate != uri);
        self.lru.push_back(uri.to_owned());
    }

    /// Evict the least-recently-used document, skipping the one currently being
    /// synced so a cap of 1 never closes the document we are opening.
    fn evict_lru(&mut self, current: &str) -> Option<String> {
        while let Some(candidate) = self.lru.pop_front() {
            if candidate == current {
                self.lru.push_back(candidate);
                if self.lru.len() <= 1 {
                    return None;
                }
                continue;
            }
            self.versions.remove(&candidate);
            self.synced.remove(&candidate);
            return Some(candidate);
        }
        None
    }

    /// Undo a `reserve` whose notification failed to send, restoring the prior
    /// version (or removing the URI entirely for a failed first `didOpen`).
    pub(super) fn rollback(&mut self, uri: &str, applied_version: i32) {
        self.synced.remove(uri);
        if self.versions.get(uri).copied() != Some(applied_version) {
            return;
        }
        if applied_version <= 1 {
            self.versions.remove(uri);
            self.lru.retain(|candidate| candidate != uri);
        } else {
            self.versions.insert(uri.to_owned(), applied_version - 1);
        }
    }

    /// The open version of `uri` when its last successful sync sent content
    /// with `digest`: the server already holds it, so no resync is needed.
    pub(super) fn unchanged(&mut self, uri: &str, digest: u64) -> Option<i32> {
        if self.synced.get(uri).map(|synced| synced.digest) != Some(digest) {
            return None;
        }
        let version = self.version(uri)?;
        self.touch(uri);
        Some(version)
    }

    /// Remember what a sync of `version` sent: `path`'s text with `digest`.
    pub(super) fn record(&mut self, uri: &str, version: i32, path: &str, digest: u64) {
        if self.version(uri) == Some(version) {
            self.synced.insert(
                uri.to_owned(),
                Synced {
                    path: path.to_owned(),
                    digest,
                    stamp: None,
                },
            );
        }
    }

    /// Every document whose last sync completed, to compare with disk.
    pub(super) fn synced(&self) -> Vec<(String, Synced)> {
        self.synced
            .iter()
            .map(|(uri, synced)| (uri.clone(), synced.clone()))
            .collect()
    }

    /// The file's `stamp` once its text proved to be the synced `digest`
    /// (a sync since then keeps its own).
    pub(super) fn stamp(&mut self, uri: &str, digest: u64, stamp: Option<SourceStamp>) {
        if let Some(synced) = self
            .synced
            .get_mut(uri)
            .filter(|synced| synced.digest == digest)
        {
            synced.stamp = stamp;
        }
    }

    /// Drop `uri` from the open set; `true` when it was open.
    pub(super) fn forget(&mut self, uri: &str) -> bool {
        self.synced.remove(uri);
        self.lru.retain(|candidate| candidate != uri);
        self.versions.remove(uri).is_some()
    }
}
