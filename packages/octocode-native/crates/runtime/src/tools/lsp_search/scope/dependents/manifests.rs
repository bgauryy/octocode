//! The anchor's project as other build files name it, and whether one of them
//! names it as a dependency.
use std::path::{Path, PathBuf};

/// How build files name the anchor's project.
#[derive(Debug)]
pub(super) enum Identity {
    /// A Go module path (`module` in `go.mod`).
    Go(String),
    /// A Cargo package: its name and its directory (canonical), the source
    /// a dependency must resolve to.
    Cargo { name: String, package: PathBuf },
    /// Maven coordinates; `version` when the pom states one.
    Maven {
        group: String,
        artifact: String,
        version: Option<String>,
    },
}

impl Identity {
    /// The identity of the project holding `anchor`, by its nearest build file,
    /// and that file. `None` when the language names dependencies some other
    /// way, or the build file does not say: every other project may then
    /// depend on it.
    pub(super) fn of(anchor: &Path, extension: &str) -> Option<(Self, PathBuf)> {
        let name = match extension {
            "go" => "go.mod",
            "rs" => "Cargo.toml",
            "java" => "pom.xml",
            _ => return None,
        };
        let file = nearest(anchor, name)?;
        let identity = match extension {
            "go" => {
                let text = octocode_engine::lsp::config_file::read_utf8(&file).ok()?;
                text.lines()
                    .find_map(|line| line.trim().strip_prefix("module "))
                    .map(|path| Self::Go(path.trim().trim_matches('"').to_owned()))?
            }
            "rs" => {
                let package = file.parent()?;
                let table = octocode_engine::lsp::workspace::cargo_manifest(package)?;
                let name = table.get("package")?.get("name")?.as_str()?;
                Self::Cargo {
                    name: name.to_owned(),
                    package: std::fs::canonicalize(package).ok()?,
                }
            }
            _ => {
                let pom = octocode_engine::lsp::config_file::read_utf8(&file).ok()?;
                let (own, parent) = project_tags(&pom);
                Self::Maven {
                    group: tag(&own, "groupId").or_else(|| tag(&parent, "groupId"))?,
                    artifact: tag(&own, "artifactId")?,
                    version: tag(&own, "version").or_else(|| tag(&parent, "version")),
                }
            }
        };
        Some((identity, file))
    }

    /// The build files that state a project's dependencies; a Gradle build file
    /// is listed so that its project counts, unread.
    pub(super) fn build_files(&self) -> &'static [&'static str] {
        match self {
            Self::Go(_) => &["go.mod"],
            Self::Cargo { .. } => &["Cargo.toml"],
            Self::Maven { .. } => &["pom.xml", "build.gradle", "build.gradle.kts"],
        }
    }
}

/// Whether the build file at `path` names `identity` as a dependency, or may:
/// a file this does not read counts.
pub(super) fn depends_on(path: &Path, identity: &Identity) -> bool {
    match identity {
        Identity::Go(module) => octocode_engine::lsp::config_file::read_utf8(path)
            .map_or(true, |text| go_requires(&text, module)),
        // The one Cargo dependency rule of reach: by local path only to the
        // anchor's package, directly or through local packages.
        Identity::Cargo { name, package } => path.parent().is_none_or(|dir| {
            octocode_engine::lsp::workspace::cargo_manifest(dir).is_none()
                || super::super::reach::depends_on(dir, name, package)
        }),
        Identity::Maven {
            group,
            artifact,
            version,
        } => {
            if path.file_name().is_some_and(|name| name != "pom.xml") {
                return true;
            }
            octocode_engine::lsp::config_file::read_utf8(path).map_or(true, |pom| {
                maven_requires(&pom, group, artifact, version.as_deref())
            })
        }
    }
}

/// The nearest `name` in `anchor`'s directory or above it.
fn nearest(anchor: &Path, name: &str) -> Option<PathBuf> {
    anchor
        .parent()?
        .ancestors()
        .map(|dir| dir.join(name))
        .find(|file| file.is_file())
}

/// `require` and `replace` lines, single or in a block, naming `module`.
fn go_requires(text: &str, module: &str) -> bool {
    let mut block = false;
    text.lines().any(|line| {
        let line = line.split("//").next().unwrap_or("").trim();
        let mut words = line.split_whitespace();
        let first = words.next().unwrap_or("");
        if block {
            block = first != ")";
            return block && first == module;
        }
        match first {
            "require" | "replace" if words.clone().next() == Some("(") => {
                block = true;
                false
            }
            "require" | "replace" => words.next() == Some(module),
            _ => false,
        }
    })
}

/// A `<dependency>` of `group:artifact` at the anchor's `version`. A version the
/// pom leaves to dependency management, or to a property this does not
/// resolve, may be the anchor's, so it counts.
fn maven_requires(pom: &str, group: &str, artifact: &str, version: Option<&str>) -> bool {
    let (own, parent) = project_tags(pom);
    let own_group = tag(&own, "groupId").or_else(|| tag(&parent, "groupId"));
    let own_version = tag(&own, "version").or_else(|| tag(&parent, "version"));
    let resolve = |value: String, own: &Option<String>| match value.as_str() {
        "${project.groupId}" | "${groupId}" | "${project.version}" | "${version}" => own.clone(),
        "${project.parent.version}" | "${parent.version}" => tag(&parent, "version"),
        _ => Some(value),
    };
    blocks(pom, "dependency").any(|dependency| {
        tag(dependency, "artifactId").as_deref() == Some(artifact)
            && tag(dependency, "groupId")
                .and_then(|value| resolve(value, &own_group))
                .is_none_or(|value| value == group)
            && match (tag(dependency, "version"), version) {
                (Some(stated), Some(anchor)) => resolve(stated, &own_version)
                    .is_none_or(|stated| stated.contains("${") || stated == anchor),
                _ => true,
            }
    })
}

/// The pom's own top-level tags (nested sections cut out) and its `<parent>`.
fn project_tags(pom: &str) -> (String, String) {
    let parent = blocks(pom, "parent").next().unwrap_or("").to_owned();
    let mut own = pom.to_owned();
    for section in [
        "parent",
        "dependencies",
        "dependencyManagement",
        "build",
        "profiles",
        "reporting",
        "modules",
        "properties",
    ] {
        let (open, close) = (format!("<{section}>"), format!("</{section}>"));
        while let Some(start) = own.find(&open)
            && let Some(end) = own.get(start..).and_then(|rest| rest.find(&close))
        {
            own.replace_range(start..start + end + close.len(), "");
        }
    }
    (own, parent)
}

/// The text inside each `<name>…</name>`.
fn blocks<'a>(text: &'a str, name: &str) -> impl Iterator<Item = &'a str> + 'a {
    let (open, close) = (format!("<{name}>"), format!("</{name}>"));
    let mut rest = text;
    std::iter::from_fn(move || {
        let (_, content) = rest.split_once(&open)?;
        let (inner, tail) = content.split_once(&close)?;
        rest = tail;
        Some(inner)
    })
}

/// The first `<name>` value of `text`, trimmed.
fn tag(text: &str, name: &str) -> Option<String> {
    blocks(text, name)
        .next()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests;
