//! Frozen lexical pages for `noIgnore` searches.
use octocode_engine_core::types::RipgrepParseResult;
use std::{
    collections::VecDeque,
    sync::Mutex,
    time::{Duration, Instant},
};

const MAX_ENTRIES: usize = 64;
const MAX_BYTES: usize = 1024 * 1024;
const TTL: Duration = Duration::from_secs(60);

struct Entry {
    snapshot: String,
    stored: Instant,
    _bytes: usize,
    value: RipgrepParseResult,
}

static STORE: Mutex<VecDeque<Entry>> = Mutex::new(VecDeque::new());

pub fn get(snapshot: &str) -> Option<RipgrepParseResult> {
    let mut store = STORE.lock().unwrap_or_else(|error| error.into_inner());
    store.retain(|entry| entry.stored.elapsed() < TTL);
    store
        .iter()
        .find(|entry| entry.snapshot == snapshot)
        .map(|entry| entry.value.clone())
}

pub fn put(snapshot: String, value: RipgrepParseResult) {
    let bytes = value
        .files
        .iter()
        .map(|file| file.path.len() + file.matches.iter().map(|m| m.value.len()).sum::<usize>())
        .sum::<usize>();
    if bytes > MAX_BYTES {
        return;
    }
    let mut store = STORE.lock().unwrap_or_else(|error| error.into_inner());
    store.retain(|entry| entry.stored.elapsed() < TTL && entry.snapshot != snapshot);
    while store.len() >= MAX_ENTRIES {
        store.pop_front();
    }
    store.push_back(Entry {
        snapshot,
        stored: Instant::now(),
        _bytes: bytes,
        value,
    });
}
