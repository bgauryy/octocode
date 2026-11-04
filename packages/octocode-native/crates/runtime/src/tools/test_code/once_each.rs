//! A memo one walk's threads share: each key's value is computed once by
//! whoever asks first, the others wait for it, and threads asking about
//! different keys rarely meet on a lock.
use super::lock;
use std::collections::HashMap;
use std::hash::{BuildHasher, BuildHasherDefault, DefaultHasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

const SHARDS: usize = 16;

pub(super) struct OnceEach<V> {
    shards: [Mutex<HashMap<PathBuf, Arc<OnceLock<V>>>>; SHARDS],
}

impl<V: Clone> OnceEach<V> {
    pub(super) fn new() -> Self {
        Self {
            shards: std::array::from_fn(|_| Mutex::default()),
        }
    }

    /// The value of `key`. `compute` may ask for other keys, as long as what
    /// they compute never asks for `key`: no lock is held while it runs, but
    /// a key waits for its own first asker.
    pub(super) fn of(&self, key: &Path, compute: impl FnOnce() -> V) -> V {
        let hash = BuildHasherDefault::<DefaultHasher>::default()
            .hash_one(key.as_os_str().as_encoded_bytes());
        let shard = &self.shards[usize::from(hash.to_le_bytes()[0]) % SHARDS];
        let cell = {
            let mut cells = lock(shard);
            match cells.get(key) {
                Some(cell) => Arc::clone(cell),
                None => {
                    let cell = Arc::<OnceLock<V>>::default();
                    cells.insert(key.to_path_buf(), Arc::clone(&cell));
                    cell
                }
            }
        };
        cell.get_or_init(compute).clone()
    }
}
