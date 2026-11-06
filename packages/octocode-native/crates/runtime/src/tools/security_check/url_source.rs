//! A package page URL or a purl, as a package coordinate. Nothing here makes a
//! request: only known public package hosts parse, and any other URL says it
//! cannot be resolved, with nothing sent anywhere.
use octocode_contract_types::ArtifactType;
use url::Url;

/// What a URL names.
#[derive(Debug, Eq, PartialEq)]
pub(super) struct Coordinate {
    pub kind: ArtifactType,
    pub name: String,
    pub version: Option<String>,
}

fn coordinate(kind: ArtifactType, name: String, version: Option<String>) -> Option<Coordinate> {
    (!name.is_empty()).then_some(Coordinate {
        kind,
        name,
        version: version.filter(|version| !version.is_empty()),
    })
}

/// Why a URL names no package to check.
#[derive(Debug, Eq, PartialEq)]
pub(super) struct Unresolved {
    pub reason: String,
    /// A repository page's `(owner, repo)`: its manifest names the package
    /// it publishes, in whatever ecosystem.
    pub repository: Option<(String, String)>,
}

fn unresolved(reason: String) -> Unresolved {
    Unresolved {
        reason,
        repository: None,
    }
}

/// The coordinate `text` names, or why it names none. A `github.com/o/r` page
/// is refused here, before any request: asking a registry about it would send a
/// private repository's name there.
pub(super) fn parse(text: &str) -> Result<Coordinate, Unresolved> {
    let text = text.trim();
    if let Some(purl) = text.strip_prefix("pkg:") {
        return from_purl(purl).ok_or_else(|| {
            unresolved(format!(
                "{text} is not a purl of npm, pypi, cargo, maven, nuget, golang, composer or gem. Not checked; nothing was sent."
            ))
        });
    }
    let no_package = || {
        unresolved(format!(
            "{text} cannot be resolved to a package: this tool reads package pages on npmjs.com, pypi.org, crates.io, nuget.org, rubygems.org, packagist.org, pkg.go.dev, mvnrepository.com and central.sonatype.com, and purls. Not checked; nothing was sent. Name it with operation package instead."
        ))
    };
    let url = Url::parse(text).map_err(|_| no_package())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(no_package());
    }
    if let Some(repository) = github_repository(&url) {
        return Err(Unresolved {
            reason: format!(
                "{text} is a repository, not a package. Not checked; nothing was sent. Its manifest (package.json, go.mod, Cargo.toml, pyproject.toml, ...) names the package it publishes: check that with operation package."
            ),
            repository: Some(repository),
        });
    }
    from_page(&url).ok_or_else(no_package)
}

/// `(owner, repo)` of a github.com page.
fn github_repository(url: &Url) -> Option<(String, String)> {
    if url.host_str()?.trim_start_matches("www.") != "github.com" {
        return None;
    }
    let mut segments = url.path_segments()?.filter(|segment| !segment.is_empty());
    let (owner, repo) = (segments.next()?, segments.next()?);
    Some((owner.to_owned(), repo.trim_end_matches(".git").to_owned()))
}

fn from_page(url: &Url) -> Option<Coordinate> {
    let host = url.host_str()?.trim_start_matches("www.");
    let segments: Vec<&str> = url
        .path_segments()?
        .filter(|segment| !segment.is_empty())
        .collect();
    let decoded = |segment: &str| percent_decoded(segment);
    match (host, segments.as_slice()) {
        ("npmjs.com", ["package", scope, name, rest @ ..]) if scope.starts_with('@') => coordinate(
            ArtifactType::Npm,
            format!("{}/{}", decoded(scope), decoded(name)),
            version_after_v(rest),
        ),
        ("npmjs.com", ["package", name, rest @ ..]) => {
            coordinate(ArtifactType::Npm, decoded(name), version_after_v(rest))
        }
        ("pypi.org", ["project", name, rest @ ..]) => coordinate(
            ArtifactType::Pypi,
            decoded(name),
            rest.first().map(|version| decoded(version)),
        ),
        ("crates.io", ["crates", name, rest @ ..]) => coordinate(
            ArtifactType::Crates,
            decoded(name),
            rest.first().map(|version| decoded(version)),
        ),
        ("nuget.org", ["packages", name, rest @ ..]) => coordinate(
            ArtifactType::Nuget,
            decoded(name),
            rest.first().map(|version| decoded(version)),
        ),
        ("rubygems.org", ["gems", name]) => coordinate(ArtifactType::Rubygems, decoded(name), None),
        ("rubygems.org", ["gems", name, "versions", version, ..]) => coordinate(
            ArtifactType::Rubygems,
            decoded(name),
            Some(decoded(version)),
        ),
        ("packagist.org", ["packages", vendor, name, ..]) => coordinate(
            ArtifactType::Packagist,
            format!("{}/{}", decoded(vendor), decoded(name)),
            // A package page names a release only by its fragment
            // (`…/guzzle#6.3.0`); without it the page is the latest.
            url.fragment()
                .map(|fragment| fragment.trim_start_matches('v'))
                .filter(|fragment| fragment.starts_with(|c: char| c.is_ascii_digit()))
                .map(str::to_owned),
        ),
        ("pkg.go.dev", path) if !path.is_empty() => {
            let joined = path
                .iter()
                .map(|part| decoded(part))
                .collect::<Vec<_>>()
                .join("/");
            let (name, version) = joined
                .split_once('@')
                .map_or((joined.clone(), None), |(name, version)| {
                    (name.to_owned(), Some(version.to_owned()))
                });
            coordinate(ArtifactType::Go, name, version)
        }
        (
            "mvnrepository.com" | "central.sonatype.com",
            ["artifact", group, artifact, rest @ ..],
        ) => coordinate(
            ArtifactType::Maven,
            format!("{}:{}", decoded(group), decoded(artifact)),
            rest.first().map(|version| decoded(version)),
        ),
        _ => None,
    }
}

fn version_after_v(rest: &[&str]) -> Option<String> {
    match rest {
        ["v", version, ..] => Some(percent_decoded(version)),
        _ => None,
    }
}

fn percent_decoded(segment: &str) -> String {
    let mut bytes = Vec::with_capacity(segment.len());
    let raw = segment.as_bytes();
    let mut index = 0;
    while index < raw.len() {
        let hex = (raw[index] == b'%')
            .then(|| raw.get(index + 1..index + 3))
            .flatten()
            .and_then(|pair| std::str::from_utf8(pair).ok())
            .and_then(|pair| u8::from_str_radix(pair, 16).ok());
        match hex {
            Some(byte) => {
                bytes.push(byte);
                index += 3;
            }
            None => {
                bytes.push(raw[index]);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// `type/namespace/name@version`, without its `?qualifiers` and `#subpath`.
fn from_purl(purl: &str) -> Option<Coordinate> {
    let purl = purl.split(['?', '#']).next()?;
    let (path, version) = match purl.rsplit_once('@') {
        // `@` inside the path is an npm scope, which a purl percent-encodes
        // but a hand-written one may not.
        Some((path, version)) if path.contains('/') => (path, Some(percent_decoded(version))),
        _ => (purl, None),
    };
    let (kind, rest) = path.split_once('/')?;
    let rest = percent_decoded(rest);
    match kind {
        "npm" => coordinate(ArtifactType::Npm, rest, version),
        "pypi" => coordinate(ArtifactType::Pypi, rest, version),
        "cargo" => coordinate(ArtifactType::Crates, rest, version),
        "nuget" => coordinate(ArtifactType::Nuget, rest, version),
        "gem" => coordinate(ArtifactType::Rubygems, rest, version),
        "golang" => coordinate(ArtifactType::Go, rest, version),
        "composer" => coordinate(ArtifactType::Packagist, rest, version),
        "maven" => {
            let (group, artifact) = rest.split_once('/')?;
            coordinate(ArtifactType::Maven, format!("{group}:{artifact}"), version)
        }
        _ => None,
    }
}

#[cfg(test)]
#[path = "url_source_tests.rs"]
mod tests;
