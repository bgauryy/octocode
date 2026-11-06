//! A Maven version's own POM: its source link, runtime dependencies and properties.
use super::super::ArtifactError;
use super::super::http::RegistryClient;
use super::super::util::{canonical_repository, github_slug, parse_url};
use super::patterns;
use octocode_contract_types::{ArtifactType, ErrorCode};

mod dependencies;

use dependencies::runtime_dependencies;

/// What a version's own POM declares.
pub(super) struct Pom {
    /// Upstream source link: the nearest `<scm>` of the POM or its parent
    /// chain, else the nearest project-level `<url>`.
    pub(super) repository: Option<String>,
    /// The release tag the `<scm>` declares for this very version (`HEAD` is
    /// no tag, and a tag a parent declares for its own version says nothing
    /// about this one). A lead to check, not proof: the caller probes it.
    pub(super) release_tag: Option<String>,
    /// Required runtime dependencies (see [`runtime_dependencies`]).
    pub(super) dependencies: Vec<String>,
    /// `<optional>true</optional>` runtime dependencies, in the same form.
    pub(super) optional_dependencies: Vec<String>,
    /// Properties those dependencies name that this POM does not define.
    pub(super) unresolved: Vec<String>,
    /// Parent POMs that exist but could not be read, each stated whole: what
    /// they would have said (a source link, a property) is unknown, not absent.
    pub(super) unread: Vec<String>,
}

/// A POM's `<parent>`: the POM its `<scm>` and `<url>` are inherited from.
struct Parent {
    group: String,
    artifact: String,
    version: String,
}

/// What one POM states about where its source lives.
#[derive(Default)]
struct Located {
    /// The `<scm>` repository, a GitHub link preferred among its `<url>`,
    /// `<connection>` and `<developerConnection>`.
    scm: Option<String>,
    /// The `<scm><tag>`, unless it is `HEAD`.
    tag: Option<String>,
    /// The project-level `<url>`, as a repository link.
    project_url: Option<String>,
    parent: Option<Parent>,
    /// The POM's own `<version>`, else its parent's.
    version: Option<String>,
}

fn first_capture(pattern: &regex::Regex, text: &str) -> Option<String> {
    pattern
        .captures(text)
        .and_then(|capture| capture.get(1))
        .map(|value| value.as_str().trim().to_owned())
}

/// The direct children of a POM's root element as `(name, inner text)`, in
/// order. A nested element of the same name (an `<url>` inside
/// `<issueManagement>`) is not a child.
fn project_children(xml: &str) -> Vec<(&str, &str)> {
    let mut children = Vec::new();
    let mut depth = 0usize;
    let mut open: Option<(&str, usize)> = None;
    for tag in patterns::element().captures_iter(xml) {
        let (Some(whole), Some(name)) = (tag.get(0), tag.get(2)) else {
            continue;
        };
        let closing = !tag[1].is_empty();
        let self_closing = !tag[3].is_empty();
        if closing {
            depth = depth.saturating_sub(1);
            if depth == 1
                && let Some((name, start)) = open.take()
            {
                // Both edges come from successive regex matches in this XML.
                let inner = xml.split_at(start).1.split_at(whole.start() - start).0;
                children.push((name, inner));
            }
        } else {
            if depth == 1 {
                if self_closing {
                    children.push((name.as_str(), ""));
                } else {
                    open = Some((name.as_str(), whole.end()));
                }
            }
            if !self_closing {
                depth += 1;
            }
        }
    }
    children
}

/// The inner text of the project's own `<name>` child, the first one: a
/// developer's, a profile's or a plugin's element of that name is not it.
fn child_block<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    project_children(xml)
        .into_iter()
        .find_map(|(child, inner)| (child == name).then_some(inner))
}

fn located(xml: &str) -> Located {
    let mut found = Located::default();
    let mut own_version = None;
    for (name, inner) in project_children(xml) {
        match name {
            "parent" => {
                found.parent = found.parent.take().or_else(|| {
                    Some(Parent {
                        group: first_capture(patterns::group_id(), inner)?,
                        artifact: first_capture(patterns::artifact_id(), inner)?,
                        version: first_capture(patterns::version(), inner)?,
                    })
                });
            }
            "scm" if found.scm.is_none() => {
                let links = [
                    patterns::url_tag(),
                    patterns::connection(),
                    patterns::developer_connection(),
                ]
                .into_iter()
                .filter_map(|pattern| first_capture(pattern, inner))
                .filter_map(|link| canonical_repository(&link))
                .collect::<Vec<_>>();
                found.scm = links
                    .iter()
                    .find(|link| github_slug(link).is_some())
                    .or(links.first())
                    .cloned();
                found.tag = first_capture(patterns::scm_tag(), inner)
                    .filter(|tag| !tag.eq_ignore_ascii_case("HEAD"));
            }
            "url" if found.project_url.is_none() => {
                found.project_url = canonical_repository(inner.trim());
            }
            "version" if own_version.is_none() => own_version = Some(inner.trim().to_owned()),
            _ => {}
        }
    }
    found.version = own_version.or_else(|| found.parent.as_ref().map(|p| p.version.clone()));
    found
}

/// A POM's text, comments removed. Enrichment is best-effort: a missing POM
/// or suspicious XML (a DOCTYPE or entity) is `None` instead of failing a
/// lookup that already resolved.
async fn fetch_pom(
    group_path: &str,
    name: &str,
    version: &str,
    client: &RegistryClient<'_>,
) -> Result<Option<String>, ArtifactError> {
    let encoded_name = super::super::util::encode_component(name);
    let encoded_version = super::super::util::encode_component(version);
    let url = parse_url(&format!(
        "https://repo1.maven.org/maven2/{group_path}/{encoded_name}/{encoded_version}/{encoded_name}-{encoded_version}.pom"
    ))?;
    let Some(xml) = client.text(ArtifactType::Maven, url, true).await? else {
        return Ok(None);
    };
    if patterns::doctype().is_match(&xml) {
        return Ok(None);
    }
    Ok(Some(patterns::comment().replace_all(&xml, "").into_owned()))
}

/// Reads the POM `at` names as its parent and makes it `at`; `None` ends the
/// chain: no parent, a parent whose version is a property, one the chain
/// already read, one the repository does not have, or one that could not be
/// read, which `unread` then states. A cancelled call is not a parent that
/// failed to read: it fails the lookup.
async fn read_parent(
    at: &mut Located,
    visited: &mut Vec<String>,
    unread: &mut Vec<String>,
    client: &RegistryClient<'_>,
) -> Result<Option<String>, ArtifactError> {
    let Some(parent) = at.parent.take().filter(|p| !p.version.contains("${")) else {
        return Ok(None);
    };
    let parent_path = super::group_path(&parent.group);
    let key = format!("{parent_path}/{}/{}", parent.artifact, parent.version);
    if visited.contains(&key) {
        return Ok(None);
    }
    visited.push(key);
    match fetch_pom(&parent_path, &parent.artifact, &parent.version, client).await {
        Ok(Some(xml)) => {
            *at = located(&xml);
            Ok(Some(xml))
        }
        Ok(None) => Ok(None),
        Err(error) if error.code == ErrorCode::Cancelled => Err(error),
        Err(error) => {
            unread.push(format!(
                "Parent POM {}:{}:{} could not be read ({}): a source link or property it defines is unknown, not absent.",
                parent.group,
                parent.artifact,
                parent.version,
                error.message.trim_end_matches('.')
            ));
            Ok(None)
        }
    }
}

/// The versioned POM's facts. The source link is the nearest `<scm>` up the
/// `<parent>` chain (a module POM seldom repeats it), read one POM at a time
/// until one names it; a parent that is missing, whose version is a property,
/// or that the chain already read ends the walk without a link, and so does
/// one that fails to read, which the result states in `unread`.
pub(super) async fn read_pom(
    group_path: &str,
    name: &str,
    version: &str,
    client: &RegistryClient<'_>,
) -> Result<Option<Pom>, ArtifactError> {
    let Some(clean) = fetch_pom(group_path, name, version, client).await? else {
        return Ok(None);
    };
    let mut at = located(&clean);
    let mut project_url = at.project_url.clone();
    let mut visited = vec![format!("{group_path}/{name}/{version}")];
    let mut unread = Vec::new();
    // The POMs read up the chain, nearest first.
    let mut ancestors: Vec<String> = Vec::new();
    let (repository, release_tag) = loop {
        if let Some(scm) = at.scm.take() {
            let tag = at
                .tag
                .take()
                .filter(|_| at.version.as_deref() == Some(version));
            break (Some(scm), tag);
        }
        let Some(xml) = read_parent(&mut at, &mut visited, &mut unread, client).await? else {
            break (project_url, None);
        };
        ancestors.push(xml);
        project_url = project_url.or_else(|| at.project_url.clone());
    };
    // A property the POM does not define is a parent's: read further up the
    // chain until the dependencies resolve or the chain ends.
    let mut found = runtime_dependencies(&clean, &ancestors);
    while !found.2.is_empty()
        && let Some(xml) = read_parent(&mut at, &mut visited, &mut unread, client).await?
    {
        ancestors.push(xml);
        found = runtime_dependencies(&clean, &ancestors);
    }
    let (dependencies, optional_dependencies, unresolved) = found;
    Ok(Some(Pom {
        repository,
        release_tag,
        dependencies,
        optional_dependencies,
        unresolved,
        unread,
    }))
}
