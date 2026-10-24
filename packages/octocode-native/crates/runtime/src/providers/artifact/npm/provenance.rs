//! An npm provenance attestation's source commit, accepted only when the
//! attestation binds the exact tarball and the manifest's repository.
use crate::providers::artifact::http::{DnsPin, JSON, RegistryClient};
use crate::providers::artifact::util::{canonical_repository, commit_sha};
use crate::providers::artifact::{ArtifactItem, ResolvedNpmRegistry};
use octocode_contract_types::ArtifactType;
use serde_json::Value;
use url::Url;

/// The source commit of an npm provenance attestation, accepted only when the
/// attestation binds this exact tarball (subject digest = `dist.integrity`)
/// and its source repository is the manifest's repository. The registry
/// verifies the Sigstore bundle at publish; this does not re-verify
/// signatures. Any failure leaves the unverified `gitHead` lead.
pub(super) async fn attested_commit(
    artifact: &ArtifactItem,
    version: &str,
    row: &serde_json::Map<String, Value>,
    registry: &ResolvedNpmRegistry,
    client: &RegistryClient<'_>,
    dns_pin: Option<DnsPin>,
) -> Option<String> {
    let dist = row.get("dist")?;
    let url = Url::parse(dist.pointer("/attestations/url")?.as_str()?).ok()?;
    // Attestations must stay on the registry origin; credential path scope is checked below.
    if url.origin() != registry.base.origin()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    let integrity = dist.get("integrity")?.as_str()?;
    let authorization = registry.authorization_for(&url);
    let bundle = client
        .json(ArtifactType::Npm, url, true, authorization, dns_pin, JSON)
        .await
        .ok()??;
    provenance_commit(
        &bundle,
        &artifact.package_name,
        version,
        integrity,
        artifact.repository.as_deref()?,
    )
}

fn provenance_commit(
    bundle: &Value,
    name: &str,
    version: &str,
    integrity: &str,
    repository: &str,
) -> Option<String> {
    use base64::Engine as _;
    let engine = base64::engine::general_purpose::STANDARD;
    let tarball_sha512 = hex::encode(engine.decode(integrity.strip_prefix("sha512-")?).ok()?);
    let purl = format!("pkg:npm/{}@{version}", name.replacen('@', "%40", 1));
    let same_repo = |uri: &str| {
        let uri = uri.strip_prefix("git+").unwrap_or(uri);
        let uri = uri.split_once('@').map_or(uri, |(repo, _)| repo);
        canonical_repository(uri).is_some_and(|uri| {
            uri.trim_end_matches('/')
                .eq_ignore_ascii_case(repository.trim_end_matches('/'))
        })
    };
    bundle
        .get("attestations")?
        .as_array()?
        .iter()
        .filter(|attestation| {
            attestation.get("predicateType").and_then(Value::as_str)
                == Some("https://slsa.dev/provenance/v1")
        })
        .find_map(|attestation| {
            let payload = attestation
                .pointer("/bundle/dsseEnvelope/payload")?
                .as_str()?;
            let statement: Value = serde_json::from_slice(&engine.decode(payload).ok()?).ok()?;
            let bound = statement.get("subject")?.as_array()?.iter().any(|subject| {
                subject.get("name").and_then(Value::as_str) == Some(purl.as_str())
                    && subject
                        .pointer("/digest/sha512")
                        .and_then(Value::as_str)
                        .is_some_and(|digest| digest.eq_ignore_ascii_case(&tarball_sha512))
            });
            if !bound {
                return None;
            }
            let source = statement
                .pointer("/predicate/buildDefinition/resolvedDependencies")?
                .as_array()?
                .first()?;
            if !same_repo(source.get("uri")?.as_str()?) {
                return None;
            }
            commit_sha(source.pointer("/digest/gitCommit")).filter(|sha| sha.len() == 40)
        })
}
