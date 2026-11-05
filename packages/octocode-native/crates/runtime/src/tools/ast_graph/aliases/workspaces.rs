//! Package names that are local code though the scan does not hold them, so an
//! import of one is an unscanned sibling and not a third-party package.
use super::*;

/// The dependencies `value` (a `package.json`) takes from this repository:
/// `workspace:`, `file:`, `link:` and `portal:` version specs.
pub(super) fn local_dependencies(value: &Value) -> impl Iterator<Item = String> {
    [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ]
    .into_iter()
    .filter_map(|field| value.get(field).and_then(Value::as_object))
    .flat_map(|map| map.iter())
    .filter(|(_, version)| {
        version.as_str().is_some_and(|version| {
            ["workspace:", "file:", "link:", "portal:"]
                .iter()
                .any(|protocol| version.starts_with(protocol))
        })
    })
    .map(|(name, _)| name.clone())
    .collect::<Vec<_>>()
    .into_iter()
}

/// The patterns a `package.json` `workspaces` field lists, array or object form.
fn workspace_patterns(value: &Value) -> Vec<String> {
    let field = value.get("workspaces");
    field
        .and_then(Value::as_array)
        .or_else(|| field?.get("packages")?.as_array())
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

/// The list items of `pnpm-workspace.yaml`'s `packages` key, by its plain shape.
fn pnpm_patterns(text: &str) -> Vec<String> {
    text.lines()
        .skip_while(|line| line.trim_end() != "packages:")
        .skip(1)
        .map_while(|line| line.trim_start().strip_prefix("- "))
        .map(|item| item.trim().trim_matches(['"', '\'']).to_owned())
        .collect()
}

/// The directories one workspace `pattern` names under `base`: a literal
/// directory, or the children of `dir/*` and `dir/**`. A `!` exclusion or any
/// other glob names none, which only leaves its packages unproven.
fn pattern_dirs(base: &Path, pattern: &str) -> Vec<PathBuf> {
    if let Some(parent) = pattern
        .strip_suffix("/**")
        .or_else(|| pattern.strip_suffix("/*"))
        .filter(|parent| !parent.contains('*'))
    {
        return std::fs::read_dir(base.join(parent))
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .map(|entry| entry.path())
            .collect();
    }
    if pattern.contains(['*', '!']) {
        return Vec::new();
    }
    vec![base.join(pattern)]
}

impl ResolveContext {
    /// A scan below a JS workspace root knows its sibling packages by name.
    pub(super) fn load_outer_workspaces(&mut self, reader: &ConfigReader<'_>) {
        for dir in dirs_above_within_repository(reader.root) {
            let patterns = reader
                .read_json(&dir.join("package.json"))
                .map(|value| workspace_patterns(&value))
                .filter(|patterns| !patterns.is_empty())
                .or_else(|| {
                    read_config_text(
                        reader.paths,
                        reader.security,
                        &dir.join("pnpm-workspace.yaml"),
                    )
                    .map(|text| pnpm_patterns(&text))
                });
            let Some(patterns) = patterns else {
                continue;
            };
            for package in patterns
                .iter()
                .flat_map(|pattern| pattern_dirs(&dir, pattern))
            {
                if let Some(name) = reader
                    .read_json(&package.join("package.json"))
                    .as_ref()
                    .and_then(|value| value.get("name")?.as_str())
                {
                    self.local_packages.insert(name.to_owned());
                }
            }
            return;
        }
    }
}
