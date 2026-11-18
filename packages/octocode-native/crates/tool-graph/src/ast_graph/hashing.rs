//! A fast hash for the lookups a graph build makes over strings it wrote
//! itself: interning, dictionaries, the artifact's string index. The keys are
//! this build's own data, never chosen by an attacker, so the std hasher's
//! protection against collision floods (SipHash, over every byte of a long
//! id) buys nothing here, and no output depends on a lookup table's order.
use std::hash::{BuildHasherDefault, Hasher};

const K: u64 = 0x517c_c1b7_2722_0a95;

/// `bytes` folded eight at a time.
pub(super) fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut hash = bytes.len() as u64;
    let (words, rest) = bytes.as_chunks::<8>();
    for word in words {
        hash = (hash.rotate_left(5) ^ u64::from_le_bytes(*word)).wrapping_mul(K);
    }
    let mut tail = 0_u64;
    for (at, byte) in rest.iter().enumerate() {
        tail |= u64::from(*byte) << (8 * at);
    }
    // The product keeps its entropy in the high bits; a table masks the low.
    (hash.rotate_left(5) ^ tail).wrapping_mul(K).rotate_left(26)
}

/// [`hash_bytes`] as a [`Hasher`], for `HashMap`s keyed by strings.
#[derive(Default)]
pub(super) struct FastHasher(u64);

impl Hasher for FastHasher {
    fn write(&mut self, bytes: &[u8]) {
        self.0 = (self.0.rotate_left(5) ^ hash_bytes(bytes)).wrapping_mul(K);
    }
    fn finish(&self) -> u64 {
        self.0.rotate_left(26)
    }
}

/// A key that is already a hash: kept as it is.
#[derive(Default)]
pub(super) struct Identity(u64);

impl Hasher for Identity {
    fn write(&mut self, bytes: &[u8]) {
        self.0 = hash_bytes(bytes);
    }
    fn write_u64(&mut self, value: u64) {
        self.0 = value;
    }
    fn finish(&self) -> u64 {
        self.0
    }
}

pub(super) type FastState = BuildHasherDefault<FastHasher>;
pub(super) type IdentityState = BuildHasherDefault<Identity>;

#[cfg(test)]
mod tests;
