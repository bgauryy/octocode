//! Python code a project installs from a local path: a `pyproject.toml`
//! dependency on a directory (uv and Poetry `path` sources, a PEP 508
//! `name @ file:` requirement). An import of what such a directory holds is
//! this repository's code; when the directory lies outside the scan root, the
//! scope is the gap, not the import.
use super::*;

/// The table `value` holds, if any.
fn table(value: Option<&toml::Value>) -> Option<&toml::Table> {
    value.and_then(toml::Value::as_table)
}

/// The paths `pyproject` (parsed) installs from, as written: relative to its
/// directory or absolute.
fn path_dependencies(pyproject: &toml::Table) -> Vec<String> {
    let path_of = |source: &toml::Value| -> Vec<String> {
        // A source is a table, or an array of them split by markers.
        let tables = match source {
            toml::Value::Array(items) => items.iter().filter_map(toml::Value::as_table).collect(),
            other => other.as_table().into_iter().collect::<Vec<_>>(),
        };
        tables
            .into_iter()
            .filter_map(|table| table.get("path")?.as_str().map(str::to_owned))
            .collect()
    };
    let tool = table(pyproject.get("tool"));
    let uv = table(tool.and_then(|tool| tool.get("uv")));
    let poetry = table(tool.and_then(|tool| tool.get("poetry")));
    let mut source_tables = vec![table(uv.and_then(|uv| uv.get("sources")))];
    for key in ["dependencies", "dev-dependencies"] {
        source_tables.push(table(poetry.and_then(|poetry| poetry.get(key))));
    }
    source_tables.extend(
        table(poetry.and_then(|poetry| poetry.get("group")))
            .into_iter()
            .flat_map(toml::Table::values)
            .map(|group| table(group.get("dependencies"))),
    );
    let mut paths = source_tables
        .into_iter()
        .flatten()
        .flat_map(toml::Table::values)
        .flat_map(path_of)
        .collect::<Vec<_>>();
    let project = table(pyproject.get("project"));
    let requirement_lists = project
        .and_then(|project| project.get("dependencies"))
        .into_iter()
        .chain(
            table(project.and_then(|project| project.get("optional-dependencies")))
                .into_iter()
                .flat_map(toml::Table::values),
        )
        .chain(
            table(pyproject.get("dependency-groups"))
                .into_iter()
                .flat_map(toml::Table::values),
        );
    paths.extend(
        requirement_lists
            .filter_map(toml::Value::as_array)
            .flatten()
            .filter_map(toml::Value::as_str)
            .filter_map(file_requirement),
    );
    paths
}

/// The path of a PEP 508 requirement on a local file URL
/// (`lib @ file:///abs/lib`, `lib @ file:../lib`), without its markers.
fn file_requirement(requirement: &str) -> Option<String> {
    let (_, url) = requirement.split_once('@')?;
    let url = url.split(';').next()?.trim();
    let path = url
        .strip_prefix("file://")
        .or_else(|| url.strip_prefix("file:"))?;
    (!path.is_empty()).then(|| path.to_owned())
}

/// The top-level modules a local path dependency at `dir` puts on the import
/// path: those of its `src/` when it has one (src layout), else its own.
/// A package is a directory holding `__init__.py` or `__init__.pyi`.
fn top_level_modules(dir: &Path) -> Vec<String> {
    let base = if dir.join("src").is_dir() {
        dir.join("src")
    } else {
        dir.to_path_buf()
    };
    std::fs::read_dir(base)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_str()?;
            if path.is_dir() {
                (path.join("__init__.py").is_file() || path.join("__init__.pyi").is_file())
                    .then(|| name.to_owned())
            } else {
                let (stem, ext) = name.rsplit_once('.')?;
                (matches!(ext, "py" | "pyi") && stem != "__init__").then(|| stem.to_owned())
            }
        })
        .collect()
}

impl ResolveContext {
    /// Records the top-level modules of the local path dependencies outside
    /// the scan root that the scanned projects' `pyproject.toml` files and the
    /// nearest one above the root name.
    pub(super) fn load_python_path_dependencies(
        &mut self,
        reader: &ConfigReader<'_>,
        py_dirs: &BTreeSet<String>,
    ) {
        let enclosing = dirs_above_within_repository(reader.root)
            .into_iter()
            .find(|dir| dir.join("pyproject.toml").is_file());
        let projects = py_dirs
            .iter()
            .map(|dir| reader.root.join(dir))
            .chain(enclosing)
            .filter(|dir| dir.join("pyproject.toml").is_file());
        for project in projects {
            let Some(pyproject) = read_config_text(
                reader.paths,
                reader.security,
                &project.join("pyproject.toml"),
            )
            .and_then(|text| text.parse::<toml::Table>().ok()) else {
                continue;
            };
            for path in path_dependencies(&pyproject) {
                let dir = lexical(&project.join(path));
                if !dir.starts_with(reader.root) {
                    self.python.outside_modules.extend(top_level_modules(&dir));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
