//! Runtime dependency declarations and inherited property resolution.
use super::{child_block, patterns, project_children};

/// The project's own `<dependencies>` as `groupId:artifactId[:version]`
/// (absent when a parent or BOM manages the version), compile and runtime
/// scope only: the required ones, then the `<optional>true</optional>` ones.
/// A dependency that states no `<scope>` or `<optional>` takes the nearest
/// `<dependencyManagement>` entry's ([`managed`]), as Maven does; else compile
/// and required. `boms` are imported BOMs in Maven's import order, each the
/// BOM's POM and its parents nearest first (an empty one is a BOM not read
/// yet), applied after the whole parent chain.
/// Managed, build-plugin, profile and reporting dependencies are not the
/// project's requirements. A `${…}` the POM defines ([`pom_properties`]) is
/// resolved; one it does not (a parent POM's property) stays as written and
/// is returned beside the lists, so it is stated, never guessed.
pub(super) fn runtime_dependencies(
    pom: &str,
    ancestors: &[String],
    boms: &[Vec<&str>],
) -> Declared {
    let Some(block) = child_block(pom, "dependencies") else {
        return Declared::default();
    };
    let properties = pom_properties(pom, ancestors);
    let managed = managed(pom, ancestors, boms);
    let mut unresolved = Vec::new();
    let mut unmanaged = 0;
    let mut managed_from = None;
    let (optional, required): (Vec<_>, Vec<_>) = patterns::dependency()
        .captures_iter(block)
        .filter_map(|capture| {
            let entry = patterns::exclusions().replace_all(capture.get(1)?.as_str(), "");
            let coordinate = format!(
                "{}:{}",
                tag(patterns::group_id(), &entry)?,
                tag(patterns::artifact_id(), &entry)?
            );
            let stated = tag(patterns::scope(), &entry);
            // A stated scope that is no runtime one leaves nothing to manage.
            if stated
                .as_deref()
                .is_some_and(|scope| !matches!(scope, "compile" | "runtime"))
            {
                return None;
            }
            let managed = management_key(&entry, &properties).and_then(|key| managed.get(&key));
            // A scope or optional left to management: no entry read so far
            // states it (a POM further up or a BOM may), or the BOM slot
            // whose entry states it.
            if stated.is_none() || tag(patterns::optional(), &entry).is_none() {
                match managed {
                    None => unmanaged += 1,
                    Some((_, _, Some(slot))) => {
                        managed_from =
                            Some(managed_from.map_or(*slot, |from: usize| from.max(*slot)));
                    }
                    Some(_) => {}
                }
            }
            let scope = stated
                .or_else(|| managed.and_then(|(scope, _, _)| scope.clone()))
                .unwrap_or_else(|| "compile".into());
            if !matches!(scope.as_str(), "compile" | "runtime") {
                return None;
            }
            let optional = tag(patterns::optional(), &entry)
                .or_else(|| managed.and_then(|(_, optional, _)| optional.clone()))
                .is_some_and(|value| value == "true");
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
    Declared {
        required: declared(required),
        optional: declared(optional),
        unresolved,
        unmanaged,
        managed_from,
    }
}

/// What [`runtime_dependencies`] found.
#[derive(Default)]
pub(super) struct Declared {
    pub(super) required: Vec<String>,
    pub(super) optional: Vec<String>,
    /// `${…}` names no POM read so far defines.
    pub(super) unresolved: Vec<String>,
    /// Dependencies whose scope or optional is left to a management entry
    /// no POM read so far states.
    pub(super) unmanaged: usize,
    /// The latest `boms` slot a dependency's scope or optional comes from:
    /// a BOM not read yet before it could still manage that dependency.
    pub(super) managed_from: Option<usize>,
}

fn tag(pattern: &regex::Regex, text: &str) -> Option<String> {
    pattern
        .captures(text)
        .and_then(|capture| capture.get(1))
        .map(|value| value.as_str().trim().to_owned())
}

/// Maven's management key of a `<dependency>` entry:
/// `groupId:artifactId:type:classifier`, `jar` and no classifier by default,
/// resolved with the declaring POM's `properties`.
fn management_key(
    entry: &str,
    properties: &std::collections::BTreeMap<String, String>,
) -> Option<String> {
    let key = format!(
        "{}:{}:{}:{}",
        tag(patterns::group_id(), entry)?,
        tag(patterns::artifact_id(), entry)?,
        tag(patterns::dependency_type(), entry).unwrap_or_else(|| "jar".into()),
        tag(patterns::classifier(), entry).unwrap_or_default(),
    );
    Some(resolve_properties(&key, properties, &mut Vec::new()))
}

/// Each `<dependencyManagement>` entry of `xml`, exclusions removed.
fn management_entries(xml: &str) -> Option<Vec<String>> {
    let block = child_block(xml, "dependencyManagement")?;
    let entries = patterns::dependency()
        .captures_iter(block)
        .filter_map(|capture| {
            Some(
                patterns::exclusions()
                    .replace_all(capture.get(1)?.as_str(), "")
                    .into_owned(),
            )
        })
        .collect();
    Some(entries)
}

/// The effective properties of a chain (a POM, then its parents nearest
/// first): Maven interpolates the whole model a POM inherits with them, so
/// a property the POM overrides wins in what its parents declare.
pub(super) fn chain_properties(
    chain: &[impl AsRef<str>],
) -> std::collections::BTreeMap<String, String> {
    match chain.split_first() {
        Some((pom, parents)) => pom_properties(pom.as_ref(), parents),
        None => std::collections::BTreeMap::new(),
    }
}

/// What a management entry states: `<scope>`, `<optional>`, and the `boms`
/// slot it comes from (`None`: the POM or its parent chain).
type Managed = (Option<String>, Option<String>, Option<usize>);

/// Management key to its nearest `<dependencyManagement>` entry: this POM's
/// first, then each ancestor's (`ancestors`, nearest first), then each
/// imported BOM's (`boms`, import order), as Maven applies imports after
/// inheritance. Coordinates resolve with the effective properties of the
/// chain they are inherited into ([`chain_properties`]): the project's for
/// its chain, each BOM's own for its parents (a BOM's parents follow it in
/// its slot). An import entry is a BOM to read ([`imports`]), not a managed
/// dependency.
fn managed(
    pom: &str,
    ancestors: &[String],
    boms: &[Vec<&str>],
) -> std::collections::BTreeMap<String, Managed> {
    let mut managed = std::collections::BTreeMap::new();
    let mut apply = |xml: &str,
                     properties: &std::collections::BTreeMap<String, String>,
                     slot: Option<usize>| {
        let Some(entries) = management_entries(xml) else {
            return;
        };
        for entry in entries {
            let scope = tag(patterns::scope(), &entry);
            if scope.as_deref() == Some("import") {
                continue;
            }
            if let Some(key) = management_key(&entry, properties) {
                managed
                    .entry(key)
                    .or_insert((scope, tag(patterns::optional(), &entry), slot));
            }
        }
    };
    let properties = pom_properties(pom, ancestors);
    for xml in std::iter::once(pom).chain(ancestors.iter().map(String::as_str)) {
        apply(xml, &properties, None);
    }
    for (slot, bom) in boms.iter().enumerate() {
        let properties = chain_properties(bom);
        for xml in bom {
            apply(xml, &properties, Some(slot));
        }
    }
    managed
}

/// The BOMs `xml`'s `<dependencyManagement>` imports (`<scope>import</scope>`)
/// in declared order as `(groupId, artifactId, version)`, resolved with the
/// effective `properties` of the chain `xml` belongs to; one whose
/// coordinate keeps a `${…}` cannot be read, and is `Err` with the
/// coordinate as resolved so far.
pub(super) fn imports(
    xml: &str,
    properties: &std::collections::BTreeMap<String, String>,
) -> Vec<Result<(String, String, String), String>> {
    let Some(entries) = management_entries(xml) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter(|entry| tag(patterns::scope(), entry).as_deref() == Some("import"))
        .filter_map(|entry| {
            let field = |pattern: &regex::Regex| {
                Some(resolve_properties(
                    &tag(pattern, entry)?,
                    properties,
                    &mut Vec::new(),
                ))
            };
            let (group, artifact, version) = (
                field(patterns::group_id())?,
                field(patterns::artifact_id())?,
                field(patterns::version())?,
            );
            Some(
                if [&group, &artifact, &version]
                    .iter()
                    .any(|value| value.contains("${"))
                {
                    Err(format!("{group}:{artifact}:{version}"))
                } else {
                    Ok((group, artifact, version))
                },
            )
        })
        .collect()
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
    ancestors: &[impl AsRef<str>],
) -> std::collections::BTreeMap<String, String> {
    let tag = |pattern: &regex::Regex, text: &str| {
        pattern
            .captures(text)
            .and_then(|capture| capture.get(1))
            .map(|value| value.as_str().trim().to_owned())
    };
    let mut properties = std::collections::BTreeMap::new();
    for ancestor in ancestors.iter().rev() {
        properties.extend(declared_properties(ancestor.as_ref()));
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

#[cfg(test)]
mod tests {
    use super::runtime_dependencies;

    /// log4j-core 2.14.1 lists `junit-vintage-engine` with no `<scope>`; its
    /// parent manages it as `test`, so it is no runtime dependency. A managed
    /// `<optional>` carries over the same way, and a stated scope wins.
    #[test]
    fn an_unscoped_dependency_takes_its_managed_scope_and_optional() {
        let child = "<project><dependencies>\
            <dependency><groupId>org.junit.vintage</groupId><artifactId>junit-vintage-engine</artifactId></dependency>\
            <dependency><groupId>com.lmax</groupId><artifactId>disruptor</artifactId></dependency>\
            <dependency><groupId>org.slf4j</groupId><artifactId>slf4j-api</artifactId><scope>runtime</scope></dependency>\
            <dependency><groupId>org.osgi</groupId><artifactId>core</artifactId></dependency>\
            </dependencies></project>";
        let parent = "<project><dependencyManagement><dependencies>\
            <dependency><groupId>org.junit.vintage</groupId><artifactId>junit-vintage-engine</artifactId><version>5.7.0</version><scope>test</scope></dependency>\
            <dependency><groupId>com.lmax</groupId><artifactId>disruptor</artifactId><version>3.4.2</version><optional>true</optional></dependency>\
            <dependency><groupId>org.slf4j</groupId><artifactId>slf4j-api</artifactId><scope>test</scope></dependency>\
            </dependencies></dependencyManagement></project>";
        let found = runtime_dependencies(child, &[parent.to_owned()], &[]);
        assert_eq!(found.required, ["org.slf4j:slf4j-api", "org.osgi:core"]);
        assert_eq!(found.optional, ["com.lmax:disruptor"]);
        assert!(found.unresolved.is_empty(), "{:?}", found.unresolved);
    }

    /// Maven manages by groupId:artifactId:type:classifier (jar, none): a
    /// managed `test-jar` does not scope the plain jar.
    #[test]
    fn a_managed_test_jar_does_not_scope_the_plain_jar() {
        let child = "<project><dependencies>\
            <dependency><groupId>x</groupId><artifactId>a</artifactId></dependency>\
            <dependency><groupId>x</groupId><artifactId>a</artifactId><type>test-jar</type></dependency>\
            <dependency><groupId>x</groupId><artifactId>b</artifactId><classifier>tests</classifier></dependency>\
            </dependencies></project>";
        let parent = "<project><dependencyManagement><dependencies>\
            <dependency><groupId>x</groupId><artifactId>a</artifactId><type>test-jar</type><scope>test</scope></dependency>\
            <dependency><groupId>x</groupId><artifactId>b</artifactId><scope>test</scope></dependency>\
            </dependencies></dependencyManagement></project>";
        let found = runtime_dependencies(child, &[parent.to_owned()], &[]);
        assert_eq!(found.required, ["x:a", "x:b"]);
    }
}
