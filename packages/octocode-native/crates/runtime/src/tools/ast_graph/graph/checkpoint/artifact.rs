use super::*;
use octocode_engine::text::analysis::{OriginProjection, SourceDescriptors};
use serde::de::IgnoredAny;
use std::io::Write;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Artifact<F, D, O> {
    pub(super) extraction: F,
    descriptors: D,
    origins: O,
    marks: (u8, bool, bool),
}

type Owned =
    Artifact<cache::OwnedArtifact, SourceDescriptors, Vec<((u32, u32, u32), OriginProjection)>>;

impl Owned {
    pub(super) fn into_parts(
        self,
    ) -> (
        GraphFactsTypedEntry,
        super::super::super::classify::SourceMarks,
        capture::CapturedSource,
    ) {
        (
            self.extraction.into_entry(),
            super::super::super::classify::SourceMarks {
                roles: self.marks.0,
                shebang: self.marks.1,
                main_marker: self.marks.2,
            },
            capture::CapturedSource {
                descriptors: self.descriptors,
                origins: self.origins.into_iter().collect(),
            },
        )
    }
}

struct Bounded(Vec<u8>);

impl Write for Bounded {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.0.len().saturating_add(bytes.len()) > LIMITS.artifact_bytes {
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

pub(super) fn encode(
    namespace: &str,
    view: &GraphFactsView<'_>,
    source: &capture::CapturedSource,
) -> Result<Vec<u8>, ToolError> {
    let marks = super::super::super::classify::SourceMarks::of(view.relative_path, view.content);
    let artifact = Artifact {
        extraction: cache::Artifact {
            namespace,
            relative_path: view.relative_path,
            content_digest: view.content_digest,
            facts: view.facts,
            reference_counts: view
                .reference_counts
                .iter()
                .map(|count| (count.declaration_id.as_str(), count.count))
                .collect::<Vec<_>>(),
        },
        descriptors: &source.descriptors,
        origins: source.origins.iter().collect::<Vec<_>>(),
        marks: (marks.roles, marks.shebang, marks.main_marker),
    };
    let mut writer = Bounded(vec![0; 65]);
    serde_json::to_writer(&mut writer, &artifact)
        .map_err(|_| limit("one extraction artifact", LIMITS.artifact_bytes as u64))?;
    let digest = octocode_engine::digest::sha256(&writer.0[65..]);
    writer.0[..64].copy_from_slice(digest.as_bytes());
    writer.0[64] = b'\n';
    Ok(writer.0)
}

pub(super) fn decode(
    bytes: &[u8],
    namespace: &str,
    relative: &str,
    digest: &str,
) -> Result<Owned, ToolError> {
    verified(bytes, namespace, relative, digest)
}

/// The extraction alone of a verified artifact: its source correspondence
/// is checked as JSON but not decoded.
pub(super) fn decode_extraction(
    bytes: &[u8],
    namespace: &str,
    relative: &str,
    digest: &str,
) -> Result<cache::OwnedArtifact, ToolError> {
    verified::<Artifact<cache::OwnedArtifact, IgnoredAny, IgnoredAny>>(
        bytes, namespace, relative, digest,
    )
    .map(|artifact| artifact.extraction)
}

fn verified<A: serde::de::DeserializeOwned + Identity>(
    bytes: &[u8],
    namespace: &str,
    relative: &str,
    digest: &str,
) -> Result<A, ToolError> {
    if bytes.len() < 65
        || bytes[64] != b'\n'
        || octocode_engine::digest::sha256(&bytes[65..]).as_bytes() != &bytes[..64]
    {
        return Err(failure("graph checkpoint checksum mismatch"));
    }
    let artifact: A = serde_json::from_slice(&bytes[65..])
        .map_err(|_| failure("invalid graph checkpoint artifact"))?;
    let extraction = artifact.extraction();
    if extraction.namespace != namespace
        || extraction.relative_path != relative
        || extraction.content_digest != digest
    {
        return Err(failure("graph checkpoint identity mismatch"));
    }
    Ok(artifact)
}

trait Identity {
    fn extraction(&self) -> &cache::OwnedArtifact;
}

impl<D, O> Identity for Artifact<cache::OwnedArtifact, D, O> {
    fn extraction(&self) -> &cache::OwnedArtifact {
        &self.extraction
    }
}
