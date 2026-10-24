//! Reading tsconfig/jsconfig files: bounded reads, `extends` chains and
//! JSON-with-comments parsing.
use super::*;

pub(super) fn config_file_in(directory: &Path) -> Option<PathBuf> {
    ["tsconfig.json", "jsconfig.json"]
        .iter()
        .map(|name| directory.join(name))
        .find(|path| path.is_file())
}

/// A tsconfig's own options merged over its `extends` chain.
#[derive(Clone, Debug, Default)]
pub(super) struct RawTsConfig {
    pub(super) base_url: Option<PathBuf>,
    /// `paths` entries plus the directory of the config that declared them
    /// (the substitution base when no `baseUrl` is in effect).
    pub(super) paths: Option<(PatternTargets, PathBuf)>,
    pub(super) references: Vec<PathBuf>,
}

/// A project manifest or config the graph reads beside its sources: inside
/// the path policy, a regular file of at most [`MAX_CONFIG_BYTES`], decoded
/// and scanned by the content policy.
pub(crate) fn read_config_text(
    paths: &PathPolicy,
    security: &ContentSecurity,
    path: &Path,
) -> Option<String> {
    // Most probes name a manifest that is absent: one stat answers them
    // before the policy resolves the path.
    if !path.is_file() {
        return None;
    }
    let validated = paths.validate_read(path).ok()?;
    let bytes = crate::tools::source::read_bounded(&validated.canonical, MAX_CONFIG_BYTES).ok()?;
    security
        .validate_text_bytes(&bytes, Some(&validated.canonical), MAX_CONFIG_BYTES)
        .ok()
        .map(|safe| safe.content)
}

pub(super) struct ConfigReader<'a> {
    pub(super) root: &'a Path,
    pub(super) paths: &'a PathPolicy,
    pub(super) security: &'a ContentSecurity,
    pub(super) cache: HashMap<PathBuf, Option<Arc<RawTsConfig>>>,
}

impl ConfigReader<'_> {
    pub(super) fn read_json(&self, path: &Path) -> Option<Value> {
        parse_jsonc(&read_config_text(self.paths, self.security, path)?)
    }

    pub(super) fn raw(&mut self, path: &Path, depth: usize) -> Option<Arc<RawTsConfig>> {
        if depth > MAX_EXTENDS_DEPTH {
            return None;
        }
        if let Some(cached) = self.cache.get(path) {
            return cached.clone();
        }
        // Mark in-progress so an `extends` cycle terminates.
        self.cache.insert(path.to_path_buf(), None);
        let parsed = self.read_json(path).map(|value| {
            let directory = path.parent().unwrap_or(self.root).to_path_buf();
            let mut merged = RawTsConfig::default();
            let extends = match value.get("extends") {
                Some(Value::String(one)) => vec![one.clone()],
                Some(Value::Array(many)) => many
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect(),
                _ => Vec::new(),
            };
            for base in extends {
                let Some(base_path) = resolve_extends(&directory, &base) else {
                    continue;
                };
                if let Some(parent) = self.raw(&base_path, depth + 1) {
                    if parent.base_url.is_some() {
                        merged.base_url.clone_from(&parent.base_url);
                    }
                    if parent.paths.is_some() {
                        merged.paths.clone_from(&parent.paths);
                    }
                }
            }
            let options = value.get("compilerOptions");
            if let Some(base_url) = options
                .and_then(|options| options.get("baseUrl"))
                .and_then(Value::as_str)
            {
                merged.base_url = Some(lexical(&directory.join(base_url)));
            }
            if let Some(Value::Object(map)) = options.and_then(|options| options.get("paths")) {
                let entries = map
                    .iter()
                    .map(|(pattern, targets)| {
                        let targets = match targets {
                            Value::Array(items) => items
                                .iter()
                                .filter_map(Value::as_str)
                                .map(str::to_owned)
                                .collect(),
                            Value::String(one) => vec![one.clone()],
                            _ => Vec::new(),
                        };
                        (pattern.clone(), targets)
                    })
                    .collect();
                merged.paths = Some((entries, directory.clone()));
            }
            if let Some(Value::Array(references)) = value.get("references") {
                for reference in references {
                    let Some(target) = reference.get("path").and_then(Value::as_str) else {
                        continue;
                    };
                    let target = lexical(&directory.join(target));
                    merged.references.push(if target.is_dir() {
                        target.join("tsconfig.json")
                    } else {
                        target
                    });
                }
            }
            Arc::new(merged)
        });
        self.cache.insert(path.to_path_buf(), parsed.clone());
        parsed
    }

    pub(super) fn compile(&self, raw: &RawTsConfig) -> TsPaths {
        let mut compiled = TsPaths {
            base_url: raw
                .base_url
                .as_deref()
                .and_then(|base| relative_to_root(self.root, base)),
            patterns: Vec::new(),
        };
        if let Some((entries, declared_in)) = &raw.paths {
            let base = raw.base_url.as_deref().unwrap_or(declared_in);
            for (pattern, targets) in entries {
                if pattern.matches('*').count() > 1 {
                    continue;
                }
                let (prefix, suffix, wildcard) = match pattern.split_once('*') {
                    Some((prefix, suffix)) => (prefix.to_owned(), suffix.to_owned(), true),
                    None => (pattern.clone(), String::new(), false),
                };
                let targets = targets
                    .iter()
                    .filter(|target| target.matches('*').count() <= 1)
                    .filter_map(|target| relative_to_root(self.root, &lexical(&base.join(target))))
                    .collect::<Vec<_>>();
                if !targets.is_empty() {
                    compiled.patterns.push(PathPattern {
                        prefix,
                        suffix,
                        wildcard,
                        targets,
                    });
                }
            }
        }
        compiled
    }

    pub(super) fn effective(&mut self, config: &Path) -> TsPaths {
        let Some(raw) = self.raw(config, 0) else {
            return TsPaths::default();
        };
        let mut compiled = self.compile(&raw);
        if compiled.patterns.is_empty() && compiled.base_url.is_none() {
            // Solution-style config (`files: []` + references, e.g. Vite's
            // tsconfig.json → tsconfig.app.json): adopt the referenced
            // projects' aliases.
            for reference in &raw.references {
                if let Some(referenced) = self.raw(reference, 1) {
                    let referenced = self.compile(&referenced);
                    compiled.patterns.extend(referenced.patterns);
                    if compiled.base_url.is_none() {
                        compiled.base_url = referenced.base_url;
                    }
                }
            }
        }
        compiled
    }
}

/// Resolve an `extends` value: relative/absolute files (with implied
/// `.json`), then `node_modules` packages up the tree. Unfound → `None`.
pub(super) fn resolve_extends(directory: &Path, value: &str) -> Option<PathBuf> {
    let probe = |base: PathBuf| {
        let with_json = PathBuf::from(format!("{}.json", base.to_string_lossy()));
        [base.clone(), with_json, base.join("tsconfig.json")]
            .into_iter()
            .find(|candidate| candidate.is_file())
    };
    if value.starts_with("./") || value.starts_with("../") || Path::new(value).is_absolute() {
        return probe(lexical(&directory.join(value)));
    }
    directory
        .ancestors()
        .take(12)
        .find_map(|ancestor| probe(ancestor.join("node_modules").join(value)))
}

/// Scan-root-relative slash path (`.` for the root); `None` outside it.
pub(super) fn relative_to_root(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let normalized = normalize(&relative.to_string_lossy());
    Some(if normalized.is_empty() {
        ".".to_owned()
    } else {
        normalized
    })
}

/// Parse JSON with comments and trailing commas (tsconfig's JSONC dialect).
pub(super) fn parse_jsonc(text: &str) -> Option<Value> {
    serde_json::from_str(&strip_jsonc(text)).ok()
}

pub(super) fn strip_jsonc(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    let mut chunk_start = 0;
    let mut in_string = false;
    // Index in `out` of a pending comma that may turn out to be trailing.
    let mut pending_comma: Option<usize> = None;
    while index < bytes.len() {
        let byte = bytes[index];
        if in_string {
            if byte == b'\\' {
                index += 2;
                continue;
            }
            if byte == b'"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        match byte {
            b'"' => {
                pending_comma = None;
                in_string = true;
                index += 1;
            }
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                out.push_str(&text[chunk_start..index]);
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
                chunk_start = index;
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                out.push_str(&text[chunk_start..index]);
                index += 2;
                while index < bytes.len()
                    && !(bytes[index] == b'*' && bytes.get(index + 1) == Some(&b'/'))
                {
                    index += 1;
                }
                index = (index + 2).min(bytes.len());
                out.push(' ');
                chunk_start = index;
            }
            b',' => {
                out.push_str(&text[chunk_start..index]);
                pending_comma = Some(out.len());
                out.push(',');
                index += 1;
                chunk_start = index;
            }
            b'}' | b']' => {
                out.push_str(&text[chunk_start..index]);
                if let Some(comma) = pending_comma.take() {
                    out.replace_range(comma..comma + 1, " ");
                }
                chunk_start = index;
                index += 1;
            }
            b' ' | b'\t' | b'\r' | b'\n' => index += 1,
            _ => {
                pending_comma = None;
                index += 1;
            }
        }
    }
    out.push_str(&text[chunk_start.min(text.len())..]);
    out
}
