//! Runtime dependency declarations and inherited property resolution.
use super::{child_block, patterns, project_children};

/// The project's own `<dependencies>` as `groupId:artifactId[:version]`
/// (absent when a parent or BOM manages the version), compile and runtime
/// scope only: the required ones, then the `<optional>true</optional>` ones.
/// Managed, build-plugin, profile and reporting dependencies are not the
/// project's requirements. A `${…}` the POM defines ([`pom_properties`]) is
/// resolved; one it does not (a parent POM's property) stays as written and
/// is returned beside the lists, so it is stated, never guessed.
pub(super) fn runtime_dependencies(
    pom: &str,
    ancestors: &[String],
) -> (Vec<String>, Vec<String>, Vec<String>) {
    let Some(block) = child_block(pom, "dependencies") else {
        return (Vec::new(), Vec::new(), Vec::new());
    };
    let tag = |pattern: &regex::Regex, text: &str| {
        pattern
            .captures(text)
            .and_then(|capture| capture.get(1))
            .map(|value| value.as_str().trim().to_owned())
    };
    let properties = pom_properties(pom, ancestors);
    let mut unresolved = Vec::new();
    let (optional, required): (Vec<_>, Vec<_>) = patterns::dependency()
        .captures_iter(block)
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

/// The `<properties>` entries a POM declares: the project's own child, not
/// a developer's, a profile's or a plugin's `<properties>` nested elsewhere.
fn declared_properties(pom: &str) -> std::collections::BTreeMap<String, String> {
    let mut properties = std::collections::BTreeMap::new();
    if let Some((_, block)) = project_children(pom)
        .into_iter()
        .find(|(name, _)| *name == "properties")
    {
        for capture in patterns::property().captures_iter(block) {
            if capture.get(1).map(|open| open.as_str())
                == capture.get(3).map(|close| close.as_str())
            {
                properties.insert(capture[1].to_owned(), capture[2].to_owned());
            }
        }
    }
    properties
}

/// The properties a POM defines for itself: the `<properties>` its ancestors
/// declare (`ancestors`, nearest first; a nearer POM overrides a farther
/// one), its own `<properties>` entries and its own coordinates
/// (`project.version`, `project.groupId`, …; a version or group it inherits
/// is its parent's, `project.parent.*`). An ancestor's coordinates are not
/// the project's, so only its `<properties>` count.
fn pom_properties(
    project: &str,
    ancestors: &[String],
) -> std::collections::BTreeMap<String, String> {
    let tag = |pattern: &regex::Regex, text: &str| {
        pattern
            .captures(text)
            .and_then(|capture| capture.get(1))
            .map(|value| value.as_str().trim().to_owned())
    };
    let mut properties = std::collections::BTreeMap::new();
    for ancestor in ancestors.iter().rev() {
        properties.extend(declared_properties(ancestor));
    }
    properties.extend(declared_properties(project));
    let parent = child_block(project, "parent");
    for (field, pattern) in [
        ("version", patterns::version()),
        ("groupId", patterns::group_id()),
        ("artifactId", patterns::artifact_id()),
    ] {
        let inherited = parent.and_then(|parent| tag(pattern, parent));
        if let Some(value) = inherited.clone() {
            properties.insert(format!("project.parent.{field}"), value);
        }
        let own = child_block(project, field)
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        if let Some(value) = own.or(inherited) {
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
