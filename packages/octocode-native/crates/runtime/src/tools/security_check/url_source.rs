//! A package page URL or a purl, as a package coordinate. Nothing here makes a
//! request: only known public package hosts parse, and any other URL says it
//! cannot be resolved, with nothing sent anywhere.
use super::ecosystem::{self, Ecosystem};
use super::repository::RepositoryId;
use crate::providers::artifact::ArtifactError;
use crate::providers::artifact::query::artifact_type::osv_ecosystem;
use octocode_contract_types::ArtifactType;
use url::Url;

/// What a URL names.
#[derive(Debug, Eq, PartialEq)]
pub(super) struct Coordinate {
    pub ecosystem: Ecosystem,
    pub name: String,
    pub version: Option<String>,
    /// A distribution release, from a purl's `distro` qualifier.
    pub release: Option<String>,
}

fn coordinate(ecosystem: Ecosystem, name: String, version: Option<String>) -> Option<Coordinate> {
    (!name.is_empty()).then_some(Coordinate {
        ecosystem,
        name,
        version: version.filter(|version| !version.is_empty()),
        release: None,
    })
}

/// A registry page's package: the registry's own ecosystem.
fn page_coordinate(
    kind: ArtifactType,
    name: String,
    version: Option<String>,
) -> Option<Coordinate> {
    coordinate(osv_ecosystem(kind), name, version)
}

/// Why a URL names no package to check.
#[derive(Debug, Eq, PartialEq)]
pub(super) struct Unresolved {
    pub reason: String,
}

fn unresolved(reason: String) -> Unresolved {
    Unresolved { reason }
}

/// The coordinate `text` names, or why it names none. A `github.com/o/r` page
/// is refused here, before any request: asking a registry about it would send a
/// private repository's name there.
pub(super) fn parse(text: &str) -> Result<Coordinate, Unresolved> {
    let text = text.trim();
    if let Some(purl) = text.strip_prefix("pkg:") {
        return match from_purl(purl) {
            Ok(Some(found)) => Ok(found),
            Ok(None) => Err(unresolved(format!(
                "{text} is not a purl this tool reads. Types: {}; for deb, apk and rpm the namespace names the distribution, as in pkg:deb/debian/openssl. Not checked; nothing was sent.",
                ecosystem::purl_types()
                    .map_err(|error| unresolved(error.message))?
                    .join(", ")
            ))),
            Err(error) => Err(unresolved(error.message)),
        };
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
    from_page(&url).ok_or_else(no_package)
}

/// What a github.com URL names: a repository, one commit of it, or an advisory.
#[derive(Debug, Eq, PartialEq)]
pub(super) enum GithubUrl {
    /// `github.com/advisories/GHSA-…`: GitHub's page of one advisory.
    Advisory {
        id: String,
    },
    Repository {
        owner: String,
        repo: String,
    },
    Commit {
        owner: String,
        repo: String,
        sha: String,
    },
}

/// The repository, commit or advisory a github.com URL names; `None` for any
/// other URL. GitHub already hosts what it names, so no registry or OSV hears
/// a repository's name. A `/commit/<sha>` page names the commit only by its
/// full hash: a short one is refused here, as OSV and GitHub both need the
/// whole hash. Only these three shapes are read: `/advisories/…` is GitHub's
/// advisory database, never an account named `advisories`, and a path whose
/// owner or name GitHub's naming rules refuse names no repository. GitHub
/// publishes no list of the other first segments it keeps for its own pages
/// (`orgs`, `settings`, …), so none is guessed: such a page that has the shape
/// of a repository is read as one, and GitHub says it has no such repository.
pub(super) fn github(text: &str) -> Option<Result<GithubUrl, Unresolved>> {
    let url = Url::parse(text.trim()).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str()?.trim_start_matches("www.") != "github.com"
    {
        return None;
    }
    let segments = url
        .path_segments()?
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    let refused = |what: &str| {
        Err(unresolved(format!(
            "{text} {what}. This tool reads github.com/<owner>/<repo>, github.com/<owner>/<repo>/commit/<full hash> and github.com/advisories/GHSA-xxxx-xxxx-xxxx. Not checked; nothing was sent."
        )))
    };
    if let ["advisories", rest @ ..] = segments.as_slice() {
        return Some(match rest {
            [id, ..] if is_ghsa_id(id) => Ok(GithubUrl::Advisory {
                id: (*id).to_owned(),
            }),
            _ => refused("names no advisory"),
        });
    }
    let [owner, repo, rest @ ..] = segments.as_slice() else {
        return None;
    };
    // The one identity of a repository, as `repository::parse` reads it.
    let Some(id) = RepositoryId::of(&format!("{owner}/{repo}")).filter(RepositoryId::on_github)
    else {
        return Some(refused("names no GitHub repository"));
    };
    let (owner, repo) = (id.owner, id.name);
    Some(match rest {
        ["commit" | "commits", sha, ..] if is_full_hash(sha) => Ok(GithubUrl::Commit {
            owner,
            repo,
            sha: sha.to_ascii_lowercase(),
        }),
        ["commit" | "commits", sha, ..] => Err(unresolved(format!(
            "{text} names the commit {sha} by a short or malformed hash. Not checked; nothing was sent. Use the full 40 or 64 character hash."
        ))),
        _ => Ok(GithubUrl::Repository { owner, repo }),
    })
}

/// `GHSA-xxxx-xxxx-xxxx`: GitHub's advisory id, four letters or digits a group.
fn is_ghsa_id(text: &str) -> bool {
    let mut groups = text.split('-');
    groups
        .next()
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("GHSA"))
        && groups
            .by_ref()
            .take(3)
            .filter(|group| {
                group.len() == 4 && group.bytes().all(|byte| byte.is_ascii_alphanumeric())
            })
            .count()
            == 3
        && groups.next().is_none()
}

/// A full commit hash: 40 (SHA-1) or 64 (SHA-256) hex digits.
fn is_full_hash(text: &str) -> bool {
    matches!(text.len(), 40 | 64) && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn from_page(url: &Url) -> Option<Coordinate> {
    let host = url.host_str()?.trim_start_matches("www.");
    let segments: Vec<&str> = url
        .path_segments()?
        .filter(|segment| !segment.is_empty())
        .collect();
    let decoded = |segment: &str| percent_decoded(segment);
    match (host, segments.as_slice()) {
        ("npmjs.com", ["package", scope, name, rest @ ..]) if scope.starts_with('@') => {
            page_coordinate(
                ArtifactType::Npm,
                format!("{}/{}", decoded(scope), decoded(name)),
                version_after_v(rest),
            )
        }
        ("npmjs.com", ["package", name, rest @ ..]) => {
            page_coordinate(ArtifactType::Npm, decoded(name), version_after_v(rest))
        }
        ("pypi.org", ["project", name, rest @ ..]) => page_coordinate(
            ArtifactType::Pypi,
            decoded(name),
            rest.first().map(|version| decoded(version)),
        ),
        ("crates.io", ["crates", name, rest @ ..]) => page_coordinate(
            ArtifactType::Crates,
            decoded(name),
            rest.first().map(|version| decoded(version)),
        ),
        ("nuget.org", ["packages", name, rest @ ..]) => page_coordinate(
            ArtifactType::Nuget,
            decoded(name),
            rest.first().map(|version| decoded(version)),
        ),
        ("rubygems.org", ["gems", name]) => {
            page_coordinate(ArtifactType::Rubygems, decoded(name), None)
        }
        ("rubygems.org", ["gems", name, "versions", version, ..]) => page_coordinate(
            ArtifactType::Rubygems,
            decoded(name),
            Some(decoded(version)),
        ),
        ("packagist.org", ["packages", vendor, name, ..]) => page_coordinate(
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
            page_coordinate(ArtifactType::Go, name, version)
        }
        (
            "mvnrepository.com" | "central.sonatype.com",
            ["artifact", group, artifact, rest @ ..],
        ) => page_coordinate(
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

/// `type/namespace/name@version?distro=…`, without its `#subpath`. The contract
/// maps the type to its ecosystem; a `distro` qualifier names the release.
fn from_purl(purl: &str) -> Result<Option<Coordinate>, ArtifactError> {
    let purl = purl.split('#').next().unwrap_or_default();
    let (purl, qualifiers) = purl.split_once('?').unwrap_or((purl, ""));
    let (path, version) = match purl.rsplit_once('@') {
        // `@` inside the path is an npm scope, which a purl percent-encodes
        // but a hand-written one may not.
        Some((path, version)) if path.contains('/') => (path, Some(percent_decoded(version))),
        _ => (purl, None),
    };
    let Some((kind, rest)) = path.split_once('/') else {
        return Ok(None);
    };
    let rest = percent_decoded(rest);
    let Some((ecosystem, name)) = ecosystem::from_purl(&kind.to_ascii_lowercase(), &rest)? else {
        return Ok(None);
    };
    let name = if ecosystem == Ecosystem::Maven {
        let Some((group, artifact)) = name.split_once('/') else {
            return Ok(None);
        };
        format!("{group}:{artifact}")
    } else {
        name
    };
    let distro = url::form_urlencoded::parse(qualifiers.as_bytes())
        .find(|(key, _)| key == "distro")
        .map(|(_, value)| value.into_owned());
    let release = match distro {
        Some(distro) => ecosystem::purl_release(ecosystem, &distro)?,
        None => None,
    };
    Ok(coordinate(ecosystem, name, version).map(|found| Coordinate { release, ..found }))
}

#[cfg(test)]
#[path = "url_source_tests.rs"]
mod tests;
