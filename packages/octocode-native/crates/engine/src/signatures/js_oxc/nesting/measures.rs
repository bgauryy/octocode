use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, PoisonError};

static PARSES: LazyLock<Mutex<HashMap<[u8; 32], usize>>> = LazyLock::new(Mutex::default);

pub(super) fn note(digest: [u8; 32]) {
    *PARSES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .entry(digest)
        .or_default() += 1;
}

/// The nesting parses of `content`.
pub(crate) fn of(content: &str) -> usize {
    PARSES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&crate::digest::sha256_digest(content.as_bytes()))
        .copied()
        .unwrap_or(0)
}
