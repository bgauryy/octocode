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

/// Whether Node, resolving the package's own `name` from its directory
/// `dir`, finds the package itself: the first `node_modules/<name>` at or
/// above `dir`, up to the repository's boundary, links to `dir`.
pub(super) fn links_itself(dir: &Path, name: &str) -> bool {
    let Ok(own) = std::fs::canonicalize(dir) else {
        return false;
    };
    for ancestor in dir.ancestors() {
        let entry = ancestor.join("node_modules").join(name);
        if entry.symlink_metadata().is_ok() {
            return std::fs::canonicalize(&entry).is_ok_and(|real| real == own);
        }
        if octocode_engine::search::repository::is_root(ancestor) {
            break;
        }
    }
    false
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

/// The workspace patterns of the root at `dir` whose `package.json` is
/// `value`: its `workspaces` field, else its `pnpm-workspace.yaml`.
fn root_patterns(
    reader: &ConfigReader<'_>,
    dir: &Path,
    value: Option<&Value>,
) -> Option<Vec<String>> {
    value
        .map(workspace_patterns)
        .filter(|patterns| !patterns.is_empty())
        .or_else(|| {
            read_config_text(
                reader.paths,
                reader.security,
                &dir.join("pnpm-workspace.yaml"),
            )
            .map(|text| pnpm_patterns(&text))
        })
}

impl ResolveContext {
    /// The member directories of a scanned workspace root at `dir`.
    pub(super) fn load_workspace_members(
        &mut self,
        reader: &ConfigReader<'_>,
        dir: &str,
        value: &Value,
    ) {
        let base = reader.root.join(dir);
        let Some(patterns) = root_patterns(reader, &base, Some(value)) else {
            return;
        };
        for member in patterns
            .iter()
            .flat_map(|pattern| pattern_dirs(&base, pattern))
        {
            if let Some(relative) = relative_to_root(reader.root, &lexical(&member)) {
                self.workspace_members.insert(relative);
            }
        }
    }

    /// A scan below a JS workspace root knows its sibling packages by name.
    /// A manifest above the scan that the path policy keeps unread leaves
    /// membership unknown ([`ResolveContext::outer_workspace_unread`]).
    pub(super) fn load_outer_workspaces(&mut self, reader: &ConfigReader<'_>) {
        for dir in dirs_above_within_repository(reader.root) {
            let manifest = dir.join("package.json");
            let value = reader.read_json(&manifest);
            let pnpm = dir.join("pnpm-workspace.yaml");
            self.outer_workspace_unread |= (value.is_none() && manifest.is_file())
                || (pnpm.is_file()
                    && read_config_text(reader.paths, reader.security, &pnpm).is_none());
            let Some(patterns) = root_patterns(reader, &dir, value.as_ref()) else {
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
