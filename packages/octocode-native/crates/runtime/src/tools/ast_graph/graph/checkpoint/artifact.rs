//! A checkpoint artifact: the file's extraction record, exactly what a
//! restart reads ([`super::Job::load`]); its source correspondence and marks
//! are captured again from the current source, so they are never written.
use super::*;
use std::io::Write;

/// An artifact's bytes, refused past `.1` (one artifact's bound).
struct Bounded(Vec<u8>, usize);

impl Write for Bounded {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.0.len().saturating_add(bytes.len()) > self.1 {
            return Err(std::io::Error::other(
                "graph extraction artifact exceeds its byte limit",
            ));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The artifact of `view`, `None` past `max_bytes` (one artifact's bound).
pub(super) fn encode(
    namespace: &str,
    view: &GraphFactsView<'_>,
    max_bytes: usize,
) -> Option<Vec<u8>> {
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
    let mut writer = Bounded(vec![0; 65], max_bytes);
    serde_json::to_writer(&mut writer, &artifact).ok()?;
    let digest = octocode_engine::digest::sha256(&writer.0[65..]);
    writer.0[..64].copy_from_slice(digest.as_bytes());
    writer.0[64] = b'\n';
    Some(writer.0)
}

/// The verified extraction record of `relative` at `digest`.
pub(super) fn decode(
    bytes: &[u8],
    namespace: &str,
    relative: &str,
    digest: &str,
) -> Result<cache::OwnedArtifact, ToolError> {
    if bytes.len() < 65
        || bytes[64] != b'\n'
        || octocode_engine::digest::sha256(&bytes[65..]).as_bytes() != &bytes[..64]
    {
        return Err(failure("graph checkpoint checksum mismatch"));
    }
    let artifact: cache::OwnedArtifact = serde_json::from_slice(&bytes[65..])
        .map_err(|_| failure("invalid graph checkpoint artifact"))?;
    if artifact.namespace != namespace
        || artifact.relative_path != relative
        || artifact.content_digest != digest
    {
        return Err(failure("graph checkpoint identity mismatch"));
    }
    Ok(artifact)
}
