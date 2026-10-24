use super::http::{JSON, RegistryClient};
use super::util::{endpoint, object_for, parse_url, required, rows, string, total};
use super::{
    ArtifactError, ArtifactItem, ArtifactProviderPage, ArtifactProviderState, TerminalLimit,
};
use crate::num::usize_of;
use crate::providers::artifact::query::artifact_search_query;
use octocode_contract_types::{ArtifactSearchQuery, ArtifactType, ErrorCode};
/// Compile-once XML/coordinate patterns. `maven-metadata.xml` and POM parsing
/// run per lookup; compiling these constant patterns on every call (and once
/// per extracted tag in the old `field` closure) was pure waste.
mod patterns;
mod pom;

use pom::read_pom;

pub(crate) async fn maven(
    query: &ArtifactSearchQuery,
    state: &ArtifactProviderState,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    if let Some(package_name) = artifact_search_query::package_name(query) {
        return exact(package_name, artifact_search_query::version(query), client).await;
    }
    let offset = state.offset.unwrap_or(0);
    let size = artifact_search_query::page_size(query);
    // Central answers rows only at `start=0`: a page is the slice of one
    // window from the first row that ends with the page, within the rows
    // one answer serves.
    let rows_through = offset.saturating_add(size as u64).min(REACHABLE_END);
    let url = endpoint(
        "https://central.sonatype.com/solrsearch/select",
        &[
            ("q", Some(solr_terms(&artifact_search_query::terms(query)))),
            ("rows", Some(rows_through.to_string())),
            ("start", Some("0".into())),
            ("wt", Some("json".into())),
        ],
    )?;
    let response = client
        .json(ArtifactType::Maven, url, false, None, None, JSON)
        .await?
        .ok_or_else(|| super::util::invalid(ArtifactType::Maven))?;
    let data = object_for(
        object_for(&response, ArtifactType::Maven)?
            .get("response")
            .ok_or_else(|| super::util::invalid(ArtifactType::Maven))?,
        ArtifactType::Maven,
    )?;
    let mut artifacts = rows(
        data.get("docs")
            .ok_or_else(|| super::util::invalid(ArtifactType::Maven))?,
        ArtifactType::Maven,
    )?
    .iter()
    .skip(usize_of(offset))
    .take(size)
    .map(|value| {
        let row = object_for(value, ArtifactType::Maven)?;
        let group = required(row.get("g"), ArtifactType::Maven)?;
        let name = required(row.get("a"), ArtifactType::Maven)?;
        let mut artifact = ArtifactItem::new(
            ArtifactType::Maven,
            format!("{group}:{name}"),
            format!(
                "https://central.sonatype.com/artifact/{}/{}",
                super::util::encode_component(&group),
                super::util::encode_component(&name)
            ),
        );
        artifact.version = string(row.get("latestVersion")).or_else(|| string(row.get("v")));
        Ok(artifact)
    })
    .collect::<Result<Vec<_>, ArtifactError>>()?;
    discovery_repositories(&mut artifacts, client).await;
    let count = total(data.get("numFound"));
    let more = count
        .map(|count| offset.saturating_add(artifacts.len() as u64) < count)
        .unwrap_or(artifacts.len() == size);
    Ok(ArtifactProviderPage {
        next_state: (more && !artifacts.is_empty()).then(|| ArtifactProviderState {
            offset: Some(offset + artifacts.len() as u64),
            ..Default::default()
        }),
        terminal_limit: (more && artifacts.is_empty()).then(|| TerminalLimit::short_page("Maven")),
        artifacts,
        total: count,
        registry: None,
    })
}

/// Central's search answers no row for any `start` past 0 while it still
/// counts every match (`q=yaml&start=20` → numFound 87, 0 docs), but one
/// answer serves at most 100 rows
/// (`rows=100` → all 87; measured 2026-10-16): the rows a discovery reaches
/// end here.
pub(super) const REACHABLE_END: u64 = 100;

/// Keyword terms as a Central search query: the words ANDed, or the
/// artifactId they spell hyphen-joined (`jackson databind` finds
/// `jackson-databind`, which Central indexes as one token). The endpoint
/// rejects bare whitespace with HTTP 400, and a single word stays bare (a
/// one-word group answers 404). Query-syntax characters (field `:`, quotes,
/// groups, wildcards, operators) separate words rather than reach the
/// endpoint; `-`, `.` and `_` stay, as artifact names use them.
fn solr_terms(terms: &str) -> String {
    let words = terms
        .split(|c: char| c.is_whitespace() || "+&|!(){}[]^\"~*?:\\/".contains(c))
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    match words.as_slice() {
        [] | [_] => words.concat(),
        _ => format!("({}) OR a:{}", words.join(" AND "), words.join("-")),
    }
}

async fn exact(
    package_name: &str,
    version: Option<&str>,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    let parts = package_name.split(':').collect::<Vec<_>>();
    let valid = patterns::valid_coordinate();
    if parts.len() != 2 || parts.iter().any(|part| !valid.is_match(part)) {
        return Err(ArtifactError {
            // A bare artifact name (`guava`) is what a keyword search finds.
            searchable_name: parts.len() == 1 && valid.is_match(package_name),
            ..ArtifactError::new(
                ErrorCode::InvalidInput,
                format!("packageName \"{package_name}\" is not a Maven groupId:artifactId."),
            )
        });
    }
    let group = parts[0];
    let name = parts[1];
    let group_path = group_path(group);
    let url = parse_url(&format!(
        "https://repo.maven.apache.org/maven2/{group_path}/{}/maven-metadata.xml",
        super::util::encode_component(name)
    ))?;
    let Some(xml) = client.text(ArtifactType::Maven, url, true).await? else {
        return Ok(ArtifactProviderPage::empty(Some(0)));
    };
    if patterns::doctype().is_match(&xml) {
        return Err(super::util::invalid(ArtifactType::Maven));
    }
    let clean = patterns::comment().replace_all(&xml, "");
    let field = |tag: &str| -> Option<String> {
        patterns::field(tag)?
            .captures(&clean)
            .and_then(|capture| capture.get(1))
            .map(|value| value.as_str().trim().to_owned())
    };
    let returned_group =
        field("groupId").ok_or_else(|| super::util::invalid(ArtifactType::Maven))?;
    let returned_name =
        field("artifactId").ok_or_else(|| super::util::invalid(ArtifactType::Maven))?;
    let mut artifact = ArtifactItem::new(
        ArtifactType::Maven,
        format!("{returned_group}:{returned_name}"),
        format!(
            "https://central.sonatype.com/artifact/{}/{}",
            super::util::encode_component(&returned_group),
            super::util::encode_component(&returned_name)
        ),
    );
    artifact.version = match version.filter(|version| *version != "latest") {
        // An exact version is one the metadata lists (Maven has no ranges
        // here: a bracketed range is not a published version).
        Some(wanted) => {
            let published = patterns::version()
                .captures_iter(&clean)
                .filter_map(|capture| capture.get(1))
                .map(|value| value.as_str().trim().to_owned())
                .collect::<Vec<_>>();
            if !published.iter().any(|version| version == wanted) {
                return Err(super::versions::version_not_found(
                    &artifact.package_name,
                    wanted,
                    &published,
                ));
            }
            Some(wanted.to_owned())
        }
        None => field("release").or_else(|| field("latest")),
    };
    if let Some(version) = artifact.version.clone() {
        let (pom, published_at) = futures_util::join!(
            read_pom(&group_path, name, &version, client),
            published_day(&returned_group, &returned_name, &version, client)
        );
        if let Some(pom) = pom? {
            artifact.repository = pom.repository;
            artifact.dependencies = Some(pom.dependencies);
            artifact.optional_dependencies =
                (!pom.optional_dependencies.is_empty()).then_some(pom.optional_dependencies);
            artifact.unresolved_properties = pom.unresolved;
        }
        artifact.published_at = published_at;
        // Maven names no commit; an upstream tag of this release pins the
        // lead (a `-jre`/`-android` classifier is not part of the tag).
        super::release_ref::pin_release_tag(&mut artifact, name, release_version(&version), client)
            .await;
    }
    Ok(ArtifactProviderPage::single(artifact))
}

/// A groupId as its repository directory (`com.google.guava` →
/// `com/google/guava`).
fn group_path(group: &str) -> String {
    group
        .split('.')
        .map(super::util::encode_component)
        .collect::<Vec<_>>()
        .join("/")
}

/// Central search names no source repository, so each discovery row reads
/// its listed version's POM (concurrently, like an exact lookup does): a
/// row on GitHub then carries the `owner`/`repo` every GitHub tool takes.
/// A POM that is missing or names no SCM leaves the row without one.
async fn discovery_repositories(artifacts: &mut [ArtifactItem], client: &RegistryClient<'_>) {
    let found = futures_util::future::join_all(artifacts.iter().map(|artifact| async move {
        let (group, name) = artifact.package_name.split_once(':')?;
        let version = artifact.version.as_deref()?;
        read_pom(&group_path(group), name, version, client)
            .await
            .ok()
            .flatten()?
            .repository
    }))
    .await;
    for (artifact, repository) in artifacts.iter_mut().zip(found) {
        artifact.repository = repository;
    }
}

/// The release a Maven version names without its variant classifier:
/// `33.0.0-jre` and `33.0.0-android` are both release `33.0.0`. A
/// pre-release qualifier (`2.0.0-rc1`, `1.0-SNAPSHOT`) stays.
fn release_version(version: &str) -> &str {
    version
        .rsplit_once('-')
        .filter(|(_, classifier)| matches!(*classifier, "jre" | "android"))
        .map_or(version, |(release, _)| release)
}

/// The day Central received `group:name:version`, from its version search
/// (`timestamp`, epoch milliseconds). Best-effort: `None` when the search
/// fails or does not list the version.
async fn published_day(
    group: &str,
    name: &str,
    version: &str,
    client: &RegistryClient<'_>,
) -> Option<String> {
    let url = endpoint(
        "https://central.sonatype.com/solrsearch/select",
        &[
            (
                "q",
                Some(format!(
                    "g:\"{group}\" AND a:\"{name}\" AND v:\"{version}\""
                )),
            ),
            ("core", Some("gav".into())),
            ("rows", Some("1".into())),
            ("wt", Some("json".into())),
        ],
    )
    .ok()?;
    let response = client
        .json(ArtifactType::Maven, url, true, None, None, JSON)
        .await
        .ok()??;
    let doc = response.pointer("/response/docs/0")?;
    (doc.get("v").and_then(serde_json::Value::as_str) == Some(version))
        .then(|| doc.get("timestamp")?.as_i64())
        .flatten()
        .map(crate::civil_date::date_millis)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod discovery_tests;
