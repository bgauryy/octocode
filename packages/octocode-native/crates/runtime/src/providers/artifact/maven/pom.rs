//! A Maven version's own POM: its source link, runtime dependencies and properties.
use super::super::http::RegistryClient;
use super::super::util::parse_url;
use super::super::{ArtifactError, ArtifactType};
use super::patterns;

/// What a version's own POM declares.
pub(super) struct Pom {
    /// Upstream source link: `<scm><url>`, else the project-level `<url>`.
    pub(super) repository: Option<String>,
    /// Required runtime dependencies (see [`runtime_dependencies`]).
    pub(super) dependencies: Vec<String>,
    /// `<optional>true</optional>` runtime dependencies, in the same form.
    pub(super) optional_dependencies: Vec<String>,
    /// Properties those dependencies name that this POM does not define.
    pub(super) unresolved: Vec<String>,
}

/// The versioned POM's facts. Enrichment is best-effort: a missing POM or
/// suspicious XML (a DOCTYPE or entity) is `None` instead of failing a
/// lookup that already resolved.
pub(super) async fn read_pom(
    group_path: &str,
    name: &str,
    version: &str,
    client: &RegistryClient<'_>,
) -> Result<Option<Pom>, ArtifactError> {
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
            .and_then(|value| super::super::util::canonical_repository(&value)),
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
