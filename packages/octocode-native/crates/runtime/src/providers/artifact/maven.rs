use super::http::{JSON, RegistryClient};
use super::util::{endpoint, object_for, parse_url, required, rows, string, total};
use super::{
    ArtifactError, ArtifactItem, ArtifactProviderPage, ArtifactProviderState, ArtifactSearchQuery,
    ArtifactType, TerminalLimit,
};
use crate::num::usize_of;
use crate::providers::artifact::types::artifact_search_query;
use octocode_contract_types::ErrorCode;
/// Compile-once XML/coordinate patterns. `maven-metadata.xml` and POM parsing
/// run per lookup; compiling these constant patterns on every call (and once
/// per extracted tag in the old `field` closure) was pure waste.
mod patterns {
    use regex::Regex;
    use std::sync::LazyLock;

    macro_rules! pattern {
        ($name:ident, $src:expr) => {
            pub(super) fn $name() -> &'static Regex {
                static RE: LazyLock<Regex> = LazyLock::new(|| {
                    Regex::new($src).expect(concat!("static maven pattern: ", stringify!($name)))
                });
                &RE
            }
        };
    }

    pattern!(valid_coordinate, r"^[A-Za-z0-9_][A-Za-z0-9_.-]*$");
    pattern!(doctype, r"(?i)<!DOCTYPE|<!ENTITY");
    pattern!(comment, r"(?s)<!--.*?-->");
    pattern!(url_tag, r"<url>\s*([^<]+?)\s*</url>");
    pattern!(scm, r"(?s)<scm>(.*?)</scm>");
    pattern!(group_id, r"<groupId>\s*([^<]+?)\s*</groupId>");
    pattern!(artifact_id, r"<artifactId>\s*([^<]+?)\s*</artifactId>");
    pattern!(release, r"<release>\s*([^<]+?)\s*</release>");
    pattern!(latest, r"<latest>\s*([^<]+?)\s*</latest>");
    pattern!(version, r"<version>\s*([^<]+?)\s*</version>");
    pattern!(
        inherited_blocks,
        r"(?s)<(dependencyManagement|build|profiles|reporting)>.*?</(dependencyManagement|build|profiles|reporting)>"
    );
    pattern!(dependencies, r"(?s)<dependencies>(.*?)</dependencies>");
    pattern!(dependency, r"(?s)<dependency>(.*?)</dependency>");
    pattern!(exclusions, r"(?s)<exclusions>.*?</exclusions>");
    pattern!(scope, r"<scope>\s*([^<]+?)\s*</scope>");
    pattern!(optional, r"<optional>\s*([^<]+?)\s*</optional>");
    pattern!(parent, r"(?s)<parent>(.*?)</parent>");
    pattern!(properties, r"(?s)<properties>(.*?)</properties>");
    pattern!(
        property,
        r"<([A-Za-z0-9_.\-]+)>\s*([^<]*?)\s*</([A-Za-z0-9_.\-]+)>"
    );
    pattern!(reference, r"\$\{([^}]+)\}");

    /// The bounded set of `<tag>value</tag>` fields extracted from
    /// `maven-metadata.xml`. Unknown tags return `None` (the `regex` crate has
    /// no backreferences, so a single generic `<(\w+)>…</\1>` is unavailable).
    pub(super) fn field(tag: &str) -> Option<&'static Regex> {
        Some(match tag {
            "groupId" => group_id(),
            "artifactId" => artifact_id(),
            "release" => release(),
            "latest" => latest(),
            _ => return None,
        })
    }
}

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
/// answer serves up to the contract's `pageSize` maximum of rows
/// (`rows=100` → all 87; measured 2026-10-16): the rows a discovery reaches
/// end here.
pub(super) const REACHABLE_END: u64 =
    crate::contracts::query_limits::artifact_search::PAGE_SIZE_MAXIMUM as u64;

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
                    &artifact.name,
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
        let (group, name) = artifact.name.split_once(':')?;
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

/// What a version's own POM declares.
struct Pom {
    /// Upstream source link: `<scm><url>`, else the project-level `<url>`.
    repository: Option<String>,
    /// Required runtime dependencies (see [`runtime_dependencies`]).
    dependencies: Vec<String>,
    /// `<optional>true</optional>` runtime dependencies, in the same form.
    optional_dependencies: Vec<String>,
    /// Properties those dependencies name that this POM does not define.
    unresolved: Vec<String>,
}

/// The versioned POM's facts. Enrichment is best-effort: a missing POM or
/// suspicious XML (a DOCTYPE or entity) is `None` instead of failing a
/// lookup that already resolved.
async fn read_pom(
    group_path: &str,
    name: &str,
    version: &str,
    client: &RegistryClient<'_>,
) -> Result<Option<Pom>, ArtifactError> {
    let encoded_name = super::util::encode_component(name);
    let encoded_version = super::util::encode_component(version);
    let url = parse_url(&format!(
        "https://repo1.maven.org/maven2/{group_path}/{encoded_name}/{encoded_version}/{encoded_name}-{encoded_version}.pom"
    ))?;
    let Some(xml) = client.text(ArtifactType::Maven, url, true).await? else {
        return Ok(None);
    };
    if patterns::doctype().is_match(&xml) {
        return Ok(None);
    }
    let clean = patterns::comment().replace_all(&xml, "");
    let first_url = |text: &str| -> Option<String> {
        patterns::url_tag()
            .captures(text)
            .and_then(|capture| capture.get(1))
            .map(|value| value.as_str().trim().to_owned())
    };
    let scm_url = patterns::scm()
        .captures(&clean)
        .and_then(|capture| capture.get(1))
        .and_then(|block| first_url(block.as_str()));
    let (dependencies, optional_dependencies, unresolved) = runtime_dependencies(&clean);
    Ok(Some(Pom {
        repository: scm_url
            .or_else(|| first_url(&clean))
            .and_then(|value| super::util::canonical_repository(&value)),
        dependencies,
        optional_dependencies,
        unresolved,
    }))
}

/// The project's own `<dependencies>` as `groupId:artifactId[:version]`
/// (absent when a parent or BOM manages the version), compile and runtime
/// scope only: the required ones, then the `<optional>true</optional>` ones.
/// Managed, build-plugin, profile and reporting dependencies are not the
/// project's requirements. A `${…}` the POM defines ([`pom_properties`]) is
/// resolved; one it does not (a parent POM's property) stays as written and
/// is returned beside the lists, so it is stated, never guessed.
fn runtime_dependencies(pom: &str) -> (Vec<String>, Vec<String>, Vec<String>) {
    let project = patterns::inherited_blocks().replace_all(pom, "");
    let Some(block) = patterns::dependencies()
        .captures(&project)
        .and_then(|capture| capture.get(1))
    else {
        return (Vec::new(), Vec::new(), Vec::new());
    };
    let tag = |pattern: &regex::Regex, text: &str| {
        pattern
            .captures(text)
            .and_then(|capture| capture.get(1))
            .map(|value| value.as_str().trim().to_owned())
    };
    let properties = pom_properties(&project);
    let mut unresolved = Vec::new();
    let (optional, required): (Vec<_>, Vec<_>) = patterns::dependency()
        .captures_iter(block.as_str())
        .filter_map(|capture| {
            let entry = patterns::exclusions().replace_all(capture.get(1)?.as_str(), "");
            let scope = tag(patterns::scope(), &entry).unwrap_or_else(|| "compile".into());
            if !matches!(scope.as_str(), "compile" | "runtime") {
                return None;
            }
            let optional = tag(patterns::optional(), &entry).is_some_and(|value| value == "true");
            let coordinate = format!(
                "{}:{}",
                tag(patterns::group_id(), &entry)?,
                tag(patterns::artifact_id(), &entry)?
            );
            let declared = match tag(patterns::version(), &entry) {
                Some(version) => {
                    let version = resolve_properties(&version, &properties, &mut unresolved);
                    format!("{coordinate}:{version}")
                }
                None => coordinate,
            };
            Some((optional, declared))
        })
        .partition(|(optional, _)| *optional);
    let declared = |rows: Vec<(bool, String)>| rows.into_iter().map(|(_, entry)| entry).collect();
    (declared(required), declared(optional), unresolved)
}

/// The properties a POM defines for itself: its `<properties>` entries and
/// its own coordinates (`project.version`, `project.groupId`, …; a version
/// or group it inherits is its parent's, `project.parent.*`).
fn pom_properties(project: &str) -> std::collections::BTreeMap<String, String> {
    let tag = |pattern: &regex::Regex, text: &str| {
        pattern
            .captures(text)
            .and_then(|capture| capture.get(1))
            .map(|value| value.as_str().trim().to_owned())
    };
    let mut properties = std::collections::BTreeMap::new();
    if let Some(block) = patterns::properties()
        .captures(project)
        .and_then(|capture| capture.get(1))
    {
        for capture in patterns::property().captures_iter(block.as_str()) {
            if capture.get(1).map(|open| open.as_str())
                == capture.get(3).map(|close| close.as_str())
            {
                properties.insert(capture[1].to_owned(), capture[2].to_owned());
            }
        }
    }
    let parent = patterns::parent()
        .captures(project)
        .and_then(|capture| capture.get(1))
        .map(|block| block.as_str().to_owned());
    // The project's own coordinates sit outside every nested block.
    let own = [
        patterns::parent(),
        patterns::properties(),
        patterns::dependencies(),
    ]
    .into_iter()
    .fold(project.to_owned(), |text, block| {
        block.replace_all(&text, "").into_owned()
    });
    for (field, pattern) in [
        ("version", patterns::version()),
        ("groupId", patterns::group_id()),
        ("artifactId", patterns::artifact_id()),
    ] {
        let inherited = parent.as_deref().and_then(|parent| tag(pattern, parent));
        if let Some(value) = inherited.clone() {
            properties.insert(format!("project.parent.{field}"), value);
        }
        if let Some(value) = tag(pattern, &own).or(inherited) {
            properties.insert(format!("project.{field}"), value.clone());
            properties.entry(field.to_owned()).or_insert(value);
        }
    }
    properties
}

/// `text` with each `${name}` the POM defines replaced (a property naming
/// another resolves too); names it does not define stay and are added to
/// `unresolved` once.
fn resolve_properties(
    text: &str,
    properties: &std::collections::BTreeMap<String, String>,
    unresolved: &mut Vec<String>,
) -> String {
    let mut resolved = text.to_owned();
    // Bounded: a property cycle stops here instead of expanding forever.
    for _ in 0..8 {
        let next = patterns::reference().replace_all(&resolved, |capture: &regex::Captures<'_>| {
            properties
                .get(&capture[1])
                .cloned()
                .unwrap_or_else(|| capture[0].to_owned())
        });
        if next == resolved {
            break;
        }
        resolved = next.into_owned();
    }
    for capture in patterns::reference().captures_iter(&resolved) {
        let name = capture[0].to_owned();
        if !unresolved.contains(&name) {
            unresolved.push(name);
        }
    }
    resolved
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
mod tests {
    use super::super::test_http::{KnownTags, RouteHttp};
    use super::super::types::{artifact_query, test_budget};
    use super::*;
    use serde_json::json;

    const METADATA: &str = "/maven2/com/google/guava/guava/maven-metadata.xml";
    const GUAVA_METADATA: &str = concat!(
        r#"<?xml version="1.0" encoding="UTF-8"?>"#,
        "<metadata>",
        "<groupId>com.google.guava</groupId>",
        "<artifactId>guava</artifactId>",
        "<versioning><release>33.7.1-jre</release></versioning>",
        "</metadata>"
    );

    /// The guava POM path of `version`.
    fn pom_path(version: &str) -> String {
        format!("/maven2/com/google/guava/guava/{version}/guava-{version}.pom")
    }

    fn guava_query() -> ArtifactSearchQuery {
        artifact_query(
            json!({"ecosystem": ArtifactType::Maven, "packageName": "com.google.guava:guava"}),
            None,
        )
    }

    async fn exact_with(http: &RouteHttp) -> ArtifactItem {
        let b = test_budget();
        let client = RegistryClient::uncached(http, &b);
        let page = maven(&guava_query(), &ArtifactProviderState::default(), &client)
            .await
            .expect("maven exact");
        assert_eq!(page.artifacts.len(), 1);
        page.artifacts.into_iter().next().expect("one artifact")
    }

    /// The guava metadata and the latest release's POM.
    fn guava(pom: &str) -> RouteHttp {
        RouteHttp::new()
            .bytes(METADATA, GUAVA_METADATA)
            .bytes(&pom_path("33.7.1-jre"), pom)
    }

    /// Maven Central's Solr search answers HTTP 400 to whitespace-
    /// separated terms ("markdown parser"), which surfaced as an
    /// `invalidInput` row for a schema-valid query. Words are ANDed, or
    /// name the artifactId they spell hyphen-joined (Central indexes
    /// `jackson-databind` as one token, so `jackson AND databind` alone
    /// misses it); query-syntax characters split words.
    #[tokio::test]
    async fn maven_discovery_ands_terms_and_splits_query_syntax() {
        for (keywords, expected) in [
            (
                json!(["markdown parser"]),
                "(markdown AND parser) OR a:markdown-parser",
            ),
            (
                json!(["jackson", "databind"]),
                "(jackson AND databind) OR a:jackson-databind",
            ),
            (json!(["a:b (c)"]), "(a AND b AND c) OR a:a-b-c"),
            (json!(["spring-boot"]), "spring-boot"),
            (json!(["guava"]), "guava"),
        ] {
            let http = RouteHttp::always(json!({"response":{"numFound":0,"docs":[]}}));
            let b = test_budget();
            let client = RegistryClient::uncached(&http, &b);
            let query = artifact_query(
                json!({"ecosystem": ArtifactType::Maven, "keywords": keywords}),
                None,
            );
            maven(&query, &ArtifactProviderState::default(), &client)
                .await
                .expect("maven search");
            let q = http.seen()[0]
                .url
                .query_pairs()
                .find(|(key, _)| key == "q")
                .map(|(_, value)| value.into_owned());
            assert_eq!(q.as_deref(), Some(expected), "{keywords}");
        }
    }

    /// A Central double for `q=yaml`: 87 matches, rows only from start 0,
    /// as many as `rows` asks.
    fn yaml_central() -> RouteHttp {
        (1..=super::REACHABLE_END).fold(RouteHttp::new(), |http, rows| {
            let docs = (0..rows.min(87))
                .map(|n| json!({"g": "g", "a": format!("a{n}"), "latestVersion": "1"}))
                .collect::<Vec<_>>();
            http.json(
                &format!("/solrsearch/select?q=yaml&rows={rows}&start=0&wt=json"),
                json!({"response": {"numFound": 87, "docs": docs}}),
            )
        })
    }

    /// Central serves no row past start 0, yet every match within one
    /// answer is reachable: 20-row pages walk all 87 rows, each page sliced
    /// from a start-0 answer, then stop.
    #[tokio::test]
    async fn maven_pages_walk_every_row_central_serves_from_its_first() {
        let http = yaml_central();
        let b = test_budget();
        let client = RegistryClient::uncached(&http, &b);
        let mut names = Vec::new();
        let mut state = ArtifactProviderState::default();
        for page in 1.. {
            let query = artifact_query(
                json!({"ecosystem": ArtifactType::Maven, "keywords": ["yaml"],
                    "pageSize": 20, "page": page}),
                None,
            );
            let served = maven(&query, &state, &client).await.expect("page");
            assert_eq!(served.terminal_limit, None, "page {page}");
            names.extend(served.artifacts.into_iter().map(|row| row.name));
            let Some(next) = served.next_state else {
                break;
            };
            state = next;
        }
        let unique = names.iter().collect::<std::collections::BTreeSet<_>>();
        assert_eq!((names.len(), unique.len()), (87, 87), "{names:?}");
        assert!(
            http.seen()
                .iter()
                .filter(|seen| seen.url.path() == "/solrsearch/select")
                .all(|seen| seen
                    .url
                    .query_pairs()
                    .any(|(key, value)| key == "start" && value == "0")),
            "every request starts at 0"
        );
    }

    #[tokio::test]
    async fn maven_exact_maps_scm_url_from_pom() {
        let pom = concat!(
            "<project>",
            "<url>https://guava.dev</url>",
            "<scm><connection>scm:git:git://github.com/google/guava.git</connection>",
            "<url>https://github.com/google/guava.git</url></scm>",
            "</project>"
        );
        let item = exact_with(&guava(pom)).await;
        assert_eq!(item.version.as_deref(), Some("33.7.1-jre"));
        assert_eq!(
            item.repository.as_deref(),
            Some("https://github.com/google/guava")
        );
    }

    #[tokio::test]
    async fn maven_exact_falls_back_to_project_url_without_scm() {
        // Live guava POMs carry no <scm> in the artifact POM (it lives in the
        // parent); the project-level <url> is the upstream link.
        let pom = concat!(
            "<project>",
            "<artifactId>guava</artifactId>",
            "<url>https://github.com/google/guava</url>",
            "</project>"
        );
        let item = exact_with(&guava(pom)).await;
        assert_eq!(
            item.repository.as_deref(),
            Some("https://github.com/google/guava")
        );
    }

    /// Without the POM nothing it declares is known: no repository and no
    /// dependency list (absent, not empty).
    #[tokio::test]
    async fn maven_exact_survives_missing_pom() {
        let item = exact_with(&RouteHttp::new().bytes(METADATA, GUAVA_METADATA)).await;
        assert_eq!(item.version.as_deref(), Some("33.7.1-jre"));
        assert_eq!(item.repository, None);
        assert_eq!(item.dependencies, None);
        assert_eq!(item.published_at, None);
    }

    /// An exact row lists the POM's required runtime dependencies as
    /// declared (`groupId:artifactId[:version]`): compile and runtime scope,
    /// never test, provided or managed ones, nor a dependency's exclusions;
    /// optional ones are listed apart. Its release day is the one Central's
    /// version search records; a POM without dependencies declares none.
    #[tokio::test]
    async fn maven_exact_lists_required_and_optional_dependencies_and_the_release_day() {
        let pom = r#"<project>
            <parent><groupId>com.google.guava</groupId><artifactId>guava-parent</artifactId><version>33.7.1-jre</version></parent>
            <dependencyManagement><dependencies>
              <dependency><groupId>managed</groupId><artifactId>only</artifactId><version>1</version></dependency>
            </dependencies></dependencyManagement>
            <dependencies>
              <dependency><groupId>com.google.guava</groupId><artifactId>failureaccess</artifactId><version>1.0.2</version>
                <exclusions><exclusion><groupId>x</groupId><artifactId>excluded</artifactId></exclusion></exclusions>
              </dependency>
              <dependency><groupId>com.google.code.findbugs</groupId><artifactId>jsr305</artifactId></dependency>
              <dependency><groupId>org.runtime</groupId><artifactId>rt</artifactId><version>2</version><scope>runtime</scope></dependency>
              <dependency><groupId>org.opt</groupId><artifactId>opt</artifactId><version>3</version><optional>true</optional></dependency>
              <dependency><groupId>junit</groupId><artifactId>junit</artifactId><version>4</version><scope>test</scope></dependency>
              <dependency><groupId>javax</groupId><artifactId>servlet</artifactId><scope>provided</scope></dependency>
              <!-- <dependency><groupId>commented</groupId><artifactId>out</artifactId></dependency> -->
            </dependencies>
            <build><plugins><plugin><dependencies>
              <dependency><groupId>plugin</groupId><artifactId>dep</artifactId></dependency>
            </dependencies></plugin></plugins></build>
        </project>"#;
        let http = guava(pom).json(
            "/solrsearch/select",
            json!({"response": {"docs": [{"v": "33.7.1-jre", "timestamp": 1_734_386_196_686_i64}]}}),
        );
        let item = exact_with(&http).await;
        assert_eq!(
            item.dependencies,
            Some(vec![
                "com.google.guava:failureaccess:1.0.2".to_owned(),
                "com.google.code.findbugs:jsr305".to_owned(),
                "org.runtime:rt:2".to_owned(),
            ])
        );
        assert_eq!(
            item.optional_dependencies,
            Some(vec!["org.opt:opt:3".to_owned()])
        );
        assert_eq!(item.published_at.as_deref(), Some("2024-12-16"));
        let version = http
            .seen()
            .into_iter()
            .find(|seen| seen.url.path() == "/solrsearch/select")
            .and_then(|seen| {
                seen.url
                    .query_pairs()
                    .find(|(key, _)| key == "q")
                    .map(|(_, value)| value.into_owned())
            });
        assert_eq!(
            version.as_deref(),
            Some(r#"g:"com.google.guava" AND a:"guava" AND v:"33.7.1-jre""#)
        );

        let bare = exact_with(&guava(
            "<project><url>https://github.com/google/guava</url></project>",
        ))
        .await;
        assert_eq!(bare.dependencies, Some(Vec::new()));
        assert_eq!(bare.optional_dependencies, None);
        // A search row for another version is not this release's day.
        let other = guava("<project/>").json(
            "/solrsearch/select",
            json!({"response": {"docs": [{"v": "1.0", "timestamp": 1_734_386_196_686_i64}]}}),
        );
        assert_eq!(exact_with(&other).await.published_at, None);
    }

    #[tokio::test]
    async fn maven_parses_exact_xml_lookup() {
        let xml = concat!(
            r#"<?xml version="1.0" encoding="UTF-8"?>"#,
            "<metadata>",
            "<groupId>com.fasterxml.jackson.core</groupId>",
            "<artifactId>jackson-databind</artifactId>",
            "<versioning><release>2.15.2</release></versioning>",
            "</metadata>"
        );
        let http = RouteHttp::new().bytes(
            "/maven2/com/fasterxml/jackson/core/jackson-databind/maven-metadata.xml",
            xml,
        );
        let b = test_budget();
        let client = RegistryClient::uncached(&http, &b);
        let q = artifact_query(
            json!({"ecosystem": ArtifactType::Maven, "packageName": "com.fasterxml.jackson.core:jackson-databind"}),
            None,
        );
        let page = maven(&q, &ArtifactProviderState::default(), &client)
            .await
            .expect("maven exact");
        assert_eq!(page.artifacts.len(), 1);
        let item = &page.artifacts[0];
        assert_eq!(item.name, "com.fasterxml.jackson.core:jackson-databind");
        assert_eq!(item.version.as_deref(), Some("2.15.2"));
        assert_eq!(
            item.registry_url,
            "https://central.sonatype.com/artifact/com.fasterxml.jackson.core/jackson-databind"
        );
    }

    /// An exact Maven version reads that version's POM and pins the
    /// upstream tag without the `-jre` classifier; an unlisted version is
    /// `versionNotFound` with the nearest published versions.
    #[tokio::test]
    async fn maven_exact_version_reads_that_release_and_its_tag() {
        let metadata = concat!(
            "<metadata><groupId>com.google.guava</groupId><artifactId>guava</artifactId>",
            "<versioning><release>33.7.1-jre</release><versions>",
            "<version>32.1.3-jre</version><version>33.0.0-jre</version><version>33.7.1-jre</version>",
            "</versions></versioning></metadata>"
        );
        let pom = "<project><url>https://github.com/google/guava</url></project>";
        let query = |version: &str| {
            artifact_query(
                json!({"ecosystem": ArtifactType::Maven,
                    "packageName": "com.google.guava:guava", "version": version}),
                None,
            )
        };
        let http = RouteHttp::new()
            .bytes(METADATA, metadata)
            .bytes(&pom_path("33.0.0-jre"), pom);
        let b = test_budget();
        let tags = KnownTags::new(vec!["google/guava@v33.0.0"]);
        let client = RegistryClient {
            http: &http,
            budget: &b,
            cache: None,
            tags: Some(&tags),
        };
        let page = maven(
            &query("33.0.0-jre"),
            &ArtifactProviderState::default(),
            &client,
        )
        .await
        .expect("exact version");
        let item = &page.artifacts[0];
        assert_eq!(item.version.as_deref(), Some("33.0.0-jre"));
        assert_eq!(item.source_ref.as_deref(), Some("v33.0.0"));
        assert!(item.source_tag);

        let error = maven(
            &query("33.0.1-jre"),
            &ArtifactProviderState::default(),
            &client,
        )
        .await
        .expect_err("unlisted version");
        assert_eq!(error.code, ErrorCode::VersionNotFound);
        assert!(
            error.nearest.iter().any(|version| version == "33.0.0-jre"),
            "{error:?}"
        );
        assert_eq!(release_version("33.0.0-android"), "33.0.0");
        assert_eq!(release_version("2.0.0-rc1"), "2.0.0-rc1");
    }

    /// A coordinate that is not `groupId:artifactId` is rejected before any
    /// request, naming the field; only a bare artifact name is marked as a
    /// name a keyword search finds.
    #[tokio::test]
    async fn maven_rejects_invalid_coordinate() {
        let http = RouteHttp::new();
        let b = test_budget();
        let client = RegistryClient::uncached(&http, &b);
        for (name, searchable) in [("not-a-maven-coordinate", true), ("g:a:1.0", false)] {
            let q = artifact_query(
                json!({"ecosystem": ArtifactType::Maven, "packageName": name}),
                None,
            );
            let err = maven(&q, &ArtifactProviderState::default(), &client)
                .await
                .expect_err("invalid maven coordinate");
            assert_eq!(err.code, ErrorCode::InvalidInput);
            assert!(err.message.starts_with("packageName "), "{err:?}");
            assert_eq!(err.searchable_name, searchable, "{name}");
        }
        assert!(http.seen().is_empty());
    }

    /// Dependency versions written as `${…}` resolve from the POM's own
    /// `<properties>` and coordinates (`project.version`, a property naming
    /// another); a property it does not define (a parent POM's) stays as
    /// written and is named once, never guessed.
    #[tokio::test]
    async fn maven_dependency_properties_resolve_or_are_named() {
        let pom = r#"<project>
            <parent><groupId>com.google.guava</groupId><artifactId>guava-parent</artifactId><version>33.7.1-jre</version></parent>
            <artifactId>guava</artifactId>
            <properties><base>2.0</base><jsr.version>${base}.1</jsr.version></properties>
            <dependencies>
              <dependency><groupId>com.google.guava</groupId><artifactId>failureaccess</artifactId><version>${failureaccess.version}</version></dependency>
              <dependency><groupId>com.google.guava</groupId><artifactId>sibling</artifactId><version>${project.version}</version></dependency>
              <dependency><groupId>com.google.code.findbugs</groupId><artifactId>jsr305</artifactId><version>${jsr.version}</version></dependency>
              <dependency><groupId>x</groupId><artifactId>again</artifactId><version>${failureaccess.version}</version></dependency>
            </dependencies>
        </project>"#;
        let item = exact_with(&guava(pom)).await;
        assert_eq!(
            item.dependencies,
            Some(vec![
                "com.google.guava:failureaccess:${failureaccess.version}".to_owned(),
                "com.google.guava:sibling:33.7.1-jre".to_owned(),
                "com.google.code.findbugs:jsr305:2.0.1".to_owned(),
                "x:again:${failureaccess.version}".to_owned(),
            ])
        );
        assert_eq!(item.unresolved_properties, ["${failureaccess.version}"]);
    }

    /// Maven metadata and POMs are XML answers cached like JSON ones: a
    /// repeated exact lookup asks the registry nothing.
    #[tokio::test]
    async fn maven_xml_answers_are_cached() {
        let http = guava("<project><url>https://github.com/google/guava</url></project>");
        let cache = super::super::ArtifactCache::new(None);
        let b = test_budget();
        let client = RegistryClient {
            cache: Some(&cache),
            ..RegistryClient::uncached(&http, &b)
        };
        for _ in 0..2 {
            let page = maven(&guava_query(), &ArtifactProviderState::default(), &client)
                .await
                .expect("maven exact");
            assert_eq!(
                page.artifacts[0].repository.as_deref(),
                Some("https://github.com/google/guava")
            );
        }
        let xml = http
            .seen()
            .iter()
            .filter(|seen| seen.accept == "application/xml")
            .count();
        assert_eq!(xml, 2, "metadata and POM once each");
    }
}
