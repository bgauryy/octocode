//! Content identity: the canonical lowercase-hex SHA-256 digest (64 chars)
//! that graph identities and cache keys are built from.
use serde_json::Value;
use sha2::{Digest, Sha256};

mod listed;

pub use listed::ListedFiles;
pub(crate) use listed::stamp_of;

pub fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// The raw SHA-256 of `bytes`.
#[must_use]
pub fn sha256_digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// The one streaming SHA-256: feed it pieces with [`HashWriter::update`],
/// or write an encoding into it ([`std::io::Write`]) so it is hashed
/// without being built.
#[derive(Clone, Default)]
pub struct HashWriter(Sha256);

impl HashWriter {
    /// Hash `bytes` next.
    pub fn update(&mut self, bytes: impl AsRef<[u8]>) {
        self.0.update(bytes);
    }

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
mod tests;
