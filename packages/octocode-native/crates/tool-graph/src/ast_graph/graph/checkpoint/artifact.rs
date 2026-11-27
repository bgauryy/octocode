//! A checkpoint artifact: the file's extraction record, exactly what a
//! restart reads ([`super::Job::load`]); its source correspondence and marks
//! are captured again from the current source, so they are never written.
//!
//! Layout: the hex SHA-256 of the body, a newline, then one raw deflate
//! stream of the record's size as JSON (a varint: what its facts cost
//! decoded) and the record in [`super::compact`] form. The cache keeps one
//! artifact per file of every scope; a source's facts are many times its
//! size, and the form writes each repeated id and field name once before
//! deflate holds the rest at a fraction. The body's SHA-256 is the integrity
//! check, so the stream carries no checksum of its own (gzip's and zlib's
//! would be a second pass over the same bytes for the same 18 or 6 bytes).
use super::*;
use flate2::Compression;
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use std::io::{Read, Write};

/// The header: a hex SHA-256 and a newline.
const HEADER: usize = 65;

/// Compression speed over ratio: the artifact is a cache that every build
/// writes whole once and reads once, so its cost is paid per build, while
/// the size it saves is bounded by the snapshot's own retention.
const LEVEL: u32 = 1;

/// An encoded artifact and the size of the JSON it holds, the size facts
/// cost decoded (what admission and retention charge).
pub(super) struct Encoded {
    pub(super) bytes: Vec<u8>,
    pub(super) json_bytes: usize,
}

/// The artifact of `view`, `None` past `max_bytes` (one artifact's bound,
/// counted on its JSON).
pub(super) fn encode(
    namespace: &str,
    view: &GraphFactsView<'_>,
    max_bytes: usize,
) -> Option<Encoded> {
    let artifact = cache::Artifact {
        namespace,
        relative_path: view.relative_path,
        content_digest: view.content_digest,
        facts: view.facts,
        reference_counts: view
            .reference_counts
            .iter()
            .map(|count| (count.declaration_id.as_str(), count.count))
            .collect::<Vec<_>>(),
    };
    // Past the bound the record is no artifact: `pack` refuses it as soon as
    // its JSON passes `max_bytes`, which is what the bound counts.
    let packed = compact::pack(&artifact, max_bytes).ok()?;
    let mut encoder = DeflateEncoder::new(vec![0; HEADER], Compression::new(LEVEL));
    let mut prefix = Vec::with_capacity(10);
    compact::put_varint(&mut prefix, packed.json_bytes as u64);
    encoder.write_all(&prefix).ok()?;
    encoder.write_all(&packed.bytes).ok()?;
    let json_bytes = packed.json_bytes;
    let mut bytes = encoder.finish().ok()?;
    seal(&mut bytes);
    Some(Encoded { bytes, json_bytes })
}

/// Writes the header over the body that follows its reserved bytes.
fn seal(bytes: &mut [u8]) {
    let digest = octocode_engine::digest::sha256(&bytes[HEADER..]);
    bytes[..HEADER - 1].copy_from_slice(digest.as_bytes());
    bytes[HEADER - 1] = b'\n';
}

/// The size as JSON and the compact record of an artifact whose checksum
/// holds, inflated no further than a record of `max_bytes` of JSON takes: a
/// stream that inflates past it is no artifact this build wrote.
fn open(bytes: &[u8], max_bytes: usize) -> Result<(usize, Vec<u8>), ToolError> {
    if bytes.len() < HEADER
        || bytes[HEADER - 1] != b'\n'
        || octocode_engine::digest::sha256(&bytes[HEADER..]).as_bytes() != &bytes[..HEADER - 1]
    {
        return Err(failure("graph checkpoint checksum mismatch"));
    }
    // The form is never longer than its JSON but by the length byte a string
    // of 128 bytes or more takes beyond its quotes.
    let cap = max_bytes.saturating_add(max_bytes / 64 + 32);
    let mut body = Vec::new();
    DeflateDecoder::new(&bytes[HEADER..])
        .take(u64::try_from(cap).unwrap_or(u64::MAX).saturating_add(1))
        .read_to_end(&mut body)
        .map_err(|_| failure("invalid graph checkpoint artifact"))?;
    if body.len() > cap {
        return Err(failure("graph checkpoint artifact exceeds its byte limit"));
    }
    let (json_bytes, record) = compact::take_varint(&body)
        .and_then(|(json, rest)| Some((usize::try_from(json).ok()?, body.len() - rest.len())))
        .ok_or_else(|| failure("invalid graph checkpoint artifact"))?;
    if json_bytes > max_bytes {
        return Err(failure("graph checkpoint artifact exceeds its byte limit"));
    }
    body.drain(..record);
    Ok((json_bytes, body))
}

/// The verified extraction record of `relative` at `digest`, and the size of
/// its JSON.
pub(super) fn decode(
    bytes: &[u8],
    namespace: &str,
    relative: &str,
    digest: &str,
    max_bytes: usize,
) -> Result<(cache::OwnedArtifact, usize), ToolError> {
    let (json_bytes, record) = open(bytes, max_bytes)?;
    let artifact: cache::OwnedArtifact =
        compact::unpack(&record).map_err(|_| failure("invalid graph checkpoint artifact"))?;
    if artifact.namespace != namespace
        || artifact.relative_path != relative
        || artifact.content_digest != digest
    {
        return Err(failure("graph checkpoint identity mismatch"));
    }
    Ok((artifact, json_bytes))
}

/// The JSON of an artifact on disk, for tests that read or rewrite it.
#[cfg(test)]
pub(super) fn json_of(bytes: &[u8]) -> Vec<u8> {
    let (_, record) = open(bytes, usize::MAX >> 8).expect("a verified artifact");
    let value: serde_json::Value = compact::unpack(&record).expect("a record");
    serde_json::to_vec(&value).expect("json")
}

/// An artifact holding `json`, sealed as this module writes them.
#[cfg(test)]
pub(super) fn artifact_of(json: &[u8]) -> Vec<u8> {
    let value: serde_json::Value = serde_json::from_slice(json).expect("json");
    let packed = compact::pack(&value, usize::MAX >> 8).expect("a record");
    let mut encoder = DeflateEncoder::new(vec![0; HEADER], Compression::new(LEVEL));
    let mut prefix = Vec::new();
    compact::put_varint(&mut prefix, packed.json_bytes as u64);
    encoder.write_all(&prefix).expect("in memory");
    encoder.write_all(&packed.bytes).expect("in memory");
    let mut bytes = encoder.finish().expect("in memory");
    seal(&mut bytes);
    bytes
}
