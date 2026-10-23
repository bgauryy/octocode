//! Package manifests: `exports`/`imports` subpath maps, condition preference
//! and build-output to source mapping.
use super::*;

impl Manifest {
    /// Re-anchor package-relative targets of a manifest that sits `offset`
    /// above the scan root (`./src/x.ts` with offset `src` → `./x.ts`);
    /// targets outside the root are dropped.
    pub(super) fn rebased(mut self, offset: &str) -> Self {
        let prefix = format!("{}/", offset.trim_matches('/'));
        let rebase = |targets: &mut Vec<String>| {
            targets.retain_mut(|target| {
                let relative = target.strip_prefix("./").unwrap_or(target);
                let Some(rest) = relative.strip_prefix(&prefix) else {
                    return false;
                };
                *target = format!("./{rest}");
                true
            });
        };
        rebase(&mut self.entries);
        for (_, targets) in self.exports.iter_mut().chain(self.imports.iter_mut()) {
            rebase(targets);
        }
        self.exports.retain(|(_, targets)| !targets.is_empty());
        self.imports.retain(|(_, targets)| !targets.is_empty());
        self
    }

    pub(super) fn parse(dir: &str, value: &Value) -> Self {
        let mut manifest = Self {
            dir: dir.to_owned(),
            ..Default::default()
        };
        if let Some(exports) = value.get("exports") {
            match exports {
                Value::Object(map) if map.keys().any(|key| key.starts_with('.')) => {
                    for (key, target) in map {
                        if !key.starts_with('.') {
                            continue;
                        }
                        let targets = relative_targets(target);
                        if key == "." {
                            manifest.entries.extend(targets);
                        } else {
                            manifest.exports.push((key.clone(), targets));
                        }
                    }
                }
                other => manifest.entries.extend(relative_targets(other)),
            }
        }
        for field in ["source", "module", "main", "types", "typings"] {
            if let Some(target) = value.get(field).and_then(Value::as_str)
                && !manifest.entries.iter().any(|entry| entry == target)
            {
                manifest.entries.push(target.to_owned());
            }
        }
        if let Some(Value::Object(map)) = value.get("imports") {
            for (key, target) in map {
                if key.starts_with('#') {
                    manifest
                        .imports
                        .push((key.clone(), relative_targets(target)));
                }
            }
        }
        manifest
    }
}

/// `./`-relative leaves of an export/import target in condition preference
/// order (bare package targets are not files of this package).
pub(super) fn relative_targets(value: &Value) -> Vec<String> {
    let mut out = Vec::new();
    condition_targets(value, &mut out, 0);
    out.retain(|target| target.starts_with("./"));
    let mut seen = BTreeSet::new();
    out.retain(|target| seen.insert(target.clone()));
    out
}

pub(super) fn condition_targets(value: &Value, out: &mut Vec<String>, depth: usize) {
    if depth > MAX_CONDITION_DEPTH {
        return;
    }
    match value {
        Value::String(target) => out.push(target.clone()),
        Value::Array(items) => {
            for item in items {
                condition_targets(item, out, depth + 1);
            }
        }
        Value::Object(map) => {
            for condition in CONDITIONS {
                if let Some(nested) = map.get(*condition) {
                    condition_targets(nested, out, depth + 1);
                }
            }
            for (condition, nested) in map {
                if !CONDITIONS.contains(&condition.as_str()) {
                    condition_targets(nested, out, depth + 1);
                }
            }
        }
        _ => {}
    }
}

/// Node subpath-pattern matching: exact key, else the longest `*` pattern
/// prefix, else a legacy `./dir/` folder mapping. Returns substituted targets.
pub(super) fn match_subpath(patterns: &[(String, Vec<String>)], key: &str) -> Option<Vec<String>> {
    if let Some((_, targets)) = patterns.iter().find(|(pattern, _)| pattern == key) {
        return Some(targets.clone());
    }
    let wildcard = patterns
        .iter()
        .filter_map(|(pattern, targets)| {
            let (prefix, suffix) = pattern.split_once('*')?;
            (key.len() >= prefix.len() + suffix.len()
                && key.starts_with(prefix)
                && key.ends_with(suffix))
            .then_some((prefix, suffix, targets))
        })
        .max_by_key(|(prefix, _, _)| prefix.len());
    if let Some((prefix, suffix, targets)) = wildcard {
        let matched = &key[prefix.len()..key.len() - suffix.len()];
        return Some(
            targets
                .iter()
                .map(|target| target.replace('*', matched))
                .collect(),
        );
    }
    patterns
        .iter()
        .filter(|(pattern, _)| pattern.ends_with('/') && key.starts_with(pattern.as_str()))
        .max_by_key(|(pattern, _)| pattern.len())
        .map(|(pattern, targets)| {
            let rest = &key[pattern.len()..];
            targets
                .iter()
                .filter(|target| target.ends_with('/'))
                .map(|target| format!("{target}{rest}"))
                .collect()
        })
}

pub(super) fn split_package(spec: &str) -> Option<(&str, &str)> {
    if spec.is_empty() {
        return None;
    }
    if spec.starts_with('@') {
        let mut slashes = spec.match_indices('/').map(|(index, _)| index);
        let _scope_end = slashes.next()?;
        return Some(match slashes.next() {
            Some(end) => (&spec[..end], &spec[end + 1..]),
            None => (spec, ""),
        });
    }
    Some(spec.split_once('/').unwrap_or((spec, "")))
}

pub(super) fn resolve_package(
    manifest: &Manifest,
    subpath: &str,
    known: &BTreeSet<String>,
) -> Option<String> {
    let entry = || {
        resolve_targets(&manifest.dir, &manifest.entries, known)
            .or_else(|| probe_js(&join(&manifest.dir, "src/index"), known))
            .or_else(|| probe_js(&join(&manifest.dir, "index"), known))
    };
    if subpath.is_empty() {
        return entry();
    }
    if let Some(targets) = match_subpath(&manifest.exports, &format!("./{subpath}"))
        && let Some(found) = resolve_targets(&manifest.dir, &targets, known)
    {
        return Some(found);
    }
    // Deep imports without (or outside) an exports map: the package file,
    // its build-output source equivalent, `src/<subpath>`, then relative to
    // the resolved entry's directory.
    join_within_root(&manifest.dir, subpath)
        .and_then(|direct| probe_js(&direct, known))
        .or_else(|| source_equivalent(&manifest.dir, subpath, known))
        .or_else(|| {
            join_within_root(&join(&manifest.dir, "src"), subpath)
                .and_then(|candidate| probe_js(&candidate, known))
        })
        .or_else(|| {
            let entry = entry()?;
            probe_js(&join_within_root(dirname(&entry), subpath)?, known)
        })
}

/// Link the first target that names a parsed file: direct source hits
/// first, then build-output targets mapped back to sources, then
/// declaration files as a last resort.
pub(super) fn resolve_targets(
    dir: &str,
    targets: &[String],
    known: &BTreeSet<String>,
) -> Option<String> {
    let joined = targets
        .iter()
        .filter_map(|target| Some((target, join_within_root(dir, target)?)))
        .collect::<Vec<_>>();
    joined
        .iter()
        .filter(|(_, path)| !is_declaration(path))
        .find_map(|(_, path)| probe_js(path, known))
        .or_else(|| {
            joined
                .iter()
                .find_map(|(target, _)| source_equivalent(dir, target, known))
        })
        .or_else(|| {
            joined
                .iter()
                .find(|(_, path)| is_declaration(path) && known.contains(path))
                .map(|(_, path)| path.clone())
        })
}

pub(super) fn probe_js_or_source(candidate: &str, known: &BTreeSet<String>) -> Option<String> {
    probe_js(candidate, known).or_else(|| {
        // An alias into a package's build output (`packages/x/dist/*`).
        let segments = candidate.split('/').collect::<Vec<_>>();
        let build = segments
            .iter()
            .position(|segment| BUILD_DIRS.contains(segment))?;
        let dir = if build == 0 {
            ".".to_owned()
        } else {
            segments[..build].join("/")
        };
        source_equivalent(&dir, &segments[build..].join("/"), known)
    })
}
