//! Content identity: the canonical lowercase-hex SHA-256 digest (64 chars)
//! that graph identities and cache keys are built from.
use serde_json::Value;
use sha2::{Digest, Sha256};

pub fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Copy `reader` into `writer` in fixed-size blocks, returning the byte count
/// and the [`sha256`] of exactly the bytes copied, so a large file is hashed
/// without being held in memory.
pub fn copy_hashed(
    mut reader: impl std::io::Read,
    mut writer: impl std::io::Write,
) -> std::io::Result<(u64, String)> {
    let mut hasher = Sha256::new();
    let mut block = vec![0_u8; 64 * 1024];
    let mut len = 0_u64;
    loop {
        let read = match reader.read(&mut block) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        hasher.update(&block[..read]);
        writer.write_all(&block[..read])?;
        len += read as u64;
    }
    Ok((len, hex::encode(hasher.finalize())))
}

/// What canonical JSON does with `null` object members.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Nulls {
    /// Keep them: for configuration a server reads, where `null` is a value.
    Keep,
    /// Drop them, so equal identities serialize to equal bytes.
    Drop,
}

/// Keys sorted recursively, with `null` object members handled by `nulls`
/// (array items stay as they are). The one canonical form of a JSON value.
#[must_use]
pub fn canonical_json(value: Value, nulls: Nulls) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries = map
                .into_iter()
                .filter(|(_, v)| nulls == Nulls::Keep || !v.is_null())
                .collect::<Vec<_>>();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, value)| (key, canonical_json(value, nulls)))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|item| canonical_json(item, nulls))
                .collect(),
        ),
        scalar => scalar,
    }
}

#[cfg(test)]
mod tests {
    use super::{Nulls, canonical_json, sha256};
    use serde_json::json;

    #[test]
    fn copy_hashed_reports_what_it_wrote() {
        let mut out = Vec::new();
        let (len, digest) = super::copy_hashed(&b"abc"[..], &mut out).expect("copy");
        assert_eq!((len, out.as_slice()), (3, &b"abc"[..]));
        assert_eq!(digest, sha256(b"abc"));
        let (len, digest) = super::copy_hashed(&b""[..], std::io::sink()).expect("empty");
        assert_eq!((len, digest), (0, sha256(b"")));
    }

    #[test]
    fn keep_nulls_sorts_keys_and_keeps_explicit_nulls() {
        let value = canonical_json(json!({"b": null, "a": {"y": 1, "x": [null]}}), Nulls::Keep);
        assert_eq!(
            serde_json::to_string(&value).unwrap(),
            r#"{"a":{"x":[null],"y":1},"b":null}"#
        );
    }

    #[test]
    fn dropping_nulls_makes_equal_identities_serialize_equally() {
        let a = canonical_json(
            json!({"b": 1, "a": {"y": [{"q": 1, "p": null}], "x": 2}}),
            Nulls::Drop,
        );
        let b = canonical_json(
            json!({"a": {"x": 2, "y": [{"q": 1}]}, "b": 1, "c": null}),
            Nulls::Drop,
        );
        assert_eq!(
            serde_json::to_vec(&a).unwrap(),
            serde_json::to_vec(&b).unwrap()
        );
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            r#"{"a":{"x":2,"y":[{"q":1}]},"b":1}"#
        );
    }

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
