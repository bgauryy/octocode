//! Content identity: the canonical lowercase-hex SHA-256 digest (64 chars)
//! that graph identities and cache keys are built from.
use sha2::{Digest, Sha256};

pub fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::sha256;

    #[test]
    fn sha256_matches_known_vectors() {
        // NIST/standard SHA-256 known-answer vectors — proves byte-for-byte
        // parity with the previous hand-rolled implementation and any external
        // consumer of these digests.
        assert_eq!(
            sha256(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256(b"The quick brown fox jumps over the lazy dog"),
            "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592"
        );
    }

    #[test]
    fn sha256_handles_multi_block_input() {
        // > 64 bytes forces multiple compression blocks; digest stays 64 hex chars.
        let input = "x".repeat(200);
        let out = sha256(input.as_bytes());
        assert_eq!(out.len(), 64);
        assert!(out.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
