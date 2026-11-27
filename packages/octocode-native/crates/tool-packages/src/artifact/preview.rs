//! Registry and auxiliary destinations shared by offline package previews.
use super::{ResolvedNpmRegistry, query::artifact_type};
use octocode_contract_types::ArtifactType;
use serde_json::{Value, json};

fn origin(base: &str) -> Option<String> {
    let url = url::Url::parse(base).ok()?;
    url.has_host().then(|| url.origin().ascii_serialization())
}

pub(crate) fn destination(
    provider: &str,
    base: Option<&str>,
    sends: &[String],
    when: Option<&str>,
) -> Value {
    let mut item = json!({"provider":provider,"sends":sends});
    if provider == "github"
        && let Some(fields) = item["sends"].as_array_mut()
    {
        fields.push(json!("GitHub authorization header when authenticated"));
    }
    if let Some(base) = base.and_then(origin) {
        item["origin"] = json!(base);
    }
    if let Some(when) = when {
        item["when"] = json!(when);
    }
    item
}

/// The registry and any auxiliary service that can receive the package
/// coordinate. Metadata-selected mirrors and repositories stay conditional.
pub(crate) fn registries(
    kind: ArtifactType,
    npm: Option<&url::Url>,
    lookup: bool,
    sends: &[String],
) -> Vec<Value> {
    let npmjs = ResolvedNpmRegistry::npmjs();
    let bases: &[&str] = match kind {
        ArtifactType::Npm => &[npm.unwrap_or(&npmjs.base).as_str()],
        ArtifactType::Pypi => &["https://pypi.org"],
        ArtifactType::Crates => &["https://crates.io"],
        ArtifactType::Maven if lookup => &[
            "https://repo.maven.apache.org",
            "https://central.sonatype.com",
        ],
        ArtifactType::Maven => &["https://central.sonatype.com"],
        ArtifactType::Nuget => &["https://api.nuget.org"],
        ArtifactType::Go if lookup => &["https://pkg.go.dev", "https://proxy.golang.org"],
        ArtifactType::Go => &["https://pkg.go.dev"],
        ArtifactType::Packagist if lookup => &["https://repo.packagist.org"],
        ArtifactType::Packagist => &["https://packagist.org"],
        ArtifactType::Rubygems => &["https://rubygems.org"],
    };
    let provider = artifact_type::as_str(kind);
    let mut out: Vec<_> = bases
        .iter()
        .map(|base| destination(provider, Some(base), sends, None))
        .collect();
    if lookup && kind == ArtifactType::Crates {
        out.push(destination(
            provider,
            Some("https://static.crates.io"),
            sends,
            Some("reading publication provenance from the released crate"),
        ));
    }
    if lookup && kind == ArtifactType::Maven {
        out.push(destination(
            provider,
            Some("https://repo1.maven.org"),
            sends,
            Some("reading the released POM"),
        ));
    }
    if kind == ArtifactType::Nuget {
        out.push(destination(
            provider,
            None,
            sends,
            Some("service endpoints selected by NuGet's service index"),
        ));
    }
    out
}

/// The two registries that batch counts of similar names. Scoped packages
/// and custom npm registries do not use the public npm downloads service.
pub(crate) fn lookalike_destination(
    kind: ArtifactType,
    name: &str,
    registry: Option<&ResolvedNpmRegistry>,
) -> Option<Value> {
    let (label, origin) = match kind {
        ArtifactType::Npm
            if !name.starts_with('@')
                && registry.is_none_or(|r| r.base == ResolvedNpmRegistry::npmjs().base) =>
        {
            ("npm downloads", "https://api.npmjs.org")
        }
        ArtifactType::Crates => ("crates", "https://crates.io"),
        _ => return None,
    };
    Some(destination(
        label,
        Some(origin),
        &[format!("packageName={name} and similar names")],
        Some("lookalike download comparison"),
    ))
}
