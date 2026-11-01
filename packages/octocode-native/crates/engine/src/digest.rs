//! Content identity: the canonical lowercase-hex SHA-256 digest (64 chars)
//! that graph identities and cache keys are built from.
use serde_json::Value;
use sha2::{Digest, Sha256};

pub fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// A [`std::io::Write`] sink that SHA-256 hashes every byte written to it,
/// so an encoding is hashed without being built.
#[derive(Default)]
pub struct HashWriter(Sha256);

impl HashWriter {
    /// The SHA-256 of everything written.
    #[must_use]
    pub fn finalize(self) -> [u8; 32] {
        self.0.finalize().into()
    }
}

impl std::io::Write for HashWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The SHA-256 of `value`'s compact JSON rendering (the bytes
/// `serde_json::to_vec` produces), hashed while it serializes. `None` if
/// serialization failed.
pub fn json_sha256(value: &(impl serde::Serialize + ?Sized)) -> Option<[u8; 32]> {
    let mut writer = HashWriter::default();
    serde_json::to_writer(&mut writer, value).ok()?;
    Some(writer.finalize())
}

/// What one streaming pass over a value's compact JSON rendering learned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonFingerprint {
    /// The [`sha256`] of the rendering's bytes.
    pub digest: String,
    /// The rendering's length in UTF-16 units.
    pub utf16_units: usize,
}

/// The [`JsonFingerprint`] of `value`'s compact rendering (`serde_json::to_string`),
/// without building the rendering: the digest and length are of exactly the
/// bytes `to_string` would produce. `None` if serialization failed.
pub fn fingerprint_json(value: &(impl serde::Serialize + ?Sized)) -> Option<JsonFingerprint> {
    struct Tee {
        hasher: Sha256,
        utf16_units: usize,
    }
    impl std::io::Write for Tee {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.hasher.update(bytes);
            self.utf16_units += crate::json_size::utf16_units_of_utf8(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    // serde_json writes many tiny pieces; hash them in blocks.
    let mut tee = std::io::BufWriter::with_capacity(
        64 * 1024,
        Tee {
            hasher: Sha256::new(),
            utf16_units: 0,
        },
    );
    serde_json::to_writer(&mut tee, value).ok()?;
    let tee = tee.into_inner().ok()?;
    Some(JsonFingerprint {
        digest: hex::encode(tee.hasher.finalize()),
        utf16_units: tee.utf16_units,
    })
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
    use super::{Nulls, canonical_json, fingerprint_json, json_sha256, sha256};
    use serde_json::json;

    /// The streamed JSON digest is the [`sha256`] of the bytes `to_vec`
    /// builds, and the fingerprint's digest.
    #[test]
    fn the_json_digest_is_that_of_the_serialized_bytes() {
        for value in [
            json!(null),
            json!({"b": [1, "é"], "a": null, "t": "quote \" 中文 😀"}),
        ] {
            let digest = hex::encode(json_sha256(&value).expect("serializes"));
            assert_eq!(digest, sha256(&serde_json::to_vec(&value).unwrap()));
            assert_eq!(digest, fingerprint_json(&value).expect("serializes").digest);
        }
    }

    /// The streamed fingerprint is the digest and UTF-16 length of the
    /// rendering `to_string` builds, whatever the value holds.
    #[test]
    fn a_json_fingerprint_is_that_of_the_rendered_string() {
        for value in [
            serde_json::json!(null),
            serde_json::json!({"a": [1, 2.5, true, null], "b": "quote \" tab \t é 中文 😀"}),
            serde_json::json!({"rows": (0..2000).map(|i| serde_json::json!({"i": i, "t": "😀é".repeat(i % 7)})).collect::<Vec<_>>()}),
        ] {
            let rendered = serde_json::to_string(&value).unwrap();
            let fingerprint = fingerprint_json(&value).unwrap();
            assert_eq!(fingerprint.digest, sha256(rendered.as_bytes()));
            assert_eq!(fingerprint.utf16_units, rendered.encode_utf16().count());
        }
        let invalid = std::collections::BTreeMap::from([((1, 2), "value")]);
        assert_eq!(fingerprint_json(&invalid), None);
    }

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
