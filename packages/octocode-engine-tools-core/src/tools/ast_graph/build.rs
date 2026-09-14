use super::types::*;
use crate::{
    policy::path::PathPolicy, security::ContentSecurity, tools::local_fetch::CancellationCheck,
};
use octocode_engine::types::GraphFactsScanOptions;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
};

const EXCLUDES: &[&str] = &[
    "node_modules",
    "dist",
    "build",
    "out",
    "coverage",
    ".git",
    "target",
    ".next",
    ".cache",
];

pub(crate) fn build_graph(
    q: &AstGraphQuery,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
) -> Result<BuiltGraph, AstGraphError> {
    cancel
        .check()
        .map_err(|e| AstGraphError::new("ast.cancelled", e))?;
    let requested_root=q.path.as_deref().map(PathBuf::from).or_else(|| infer_root(q)).ok_or_else(||AstGraphError::new("invalidGraphQuery","path is required — or provide an absolute file path to infer its nearest Cargo.toml (Rust) or package.json root"))?;
    let validated = paths
        .validate(&requested_root)
        .map_err(|e| AstGraphError::new("ast.path.invalid", e.message))?;
    let mut exclude = EXCLUDES.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    if let Some(extra) = &q.exclude_dir {
        for e in extra {
            if !exclude.contains(e) {
                exclude.push(e.clone())
            }
        }
    }
    let max_files = q.max_files.unwrap_or(20_000).clamp(1, 50_000);
    let scan = octocode_engine::portable::scan_graph_facts_filtered(
        GraphFactsScanOptions {
            path: validated.canonical.to_string_lossy().into_owned(),
            exclude_dir: Some(exclude),
            max_files: Some(max_files),
            max_file_bytes: Some(1_000_000),
        },
        &|path| {
            cancel
                .check()
                .map_err(|message| format!("[ast.execution.cancelled] {message}"))?;
            Ok(paths.permits_discovery(path))
        },
    )
    .map_err(|e| {
        let message = e.to_string();
        let code = if message.starts_with("[ast.execution.cancelled]") {
            "ast.cancelled"
        } else {
            "ast.graph.scanFailed"
        };
        AstGraphError::new(code, message)
    })?;
    cancel
        .check()
        .map_err(|e| AstGraphError::new("ast.cancelled", e))?;
    let known: BTreeSet<String> = scan.candidate_paths.iter().map(|x| normalize(x)).collect();
    let mut built = BuiltGraph {
        root: validated.canonical,
        display_path: paths.redact(&requested_root),
        files_skipped: scan.files_skipped,
        truncated: scan.truncated,
        ..Default::default()
    };
    let rust_cargo_unavailable = q.rust_workspace.as_deref() == Some("cargo")
        && known.iter().any(|file| file.ends_with(".rs"))
        && !has_cargo_manifest(&built.root, &known);
    if rust_cargo_unavailable {
        built.diagnostics.push(Diagnostic {
            file: ".".into(),
            line: None,
            code: "unsupported-linking".into(),
            message: "Cargo metadata cargo-manifest-missing: No Cargo.toml was found at the scan root or known Rust-file ancestors.".into(),
        });
    }
    let cargo_crates = if q.rust_workspace.as_deref() == Some("cargo") && !rust_cargo_unavailable {
        match load_cargo_crates(&built.root) {
            Ok(map) => map,
            Err(message) => {
                built.diagnostics.push(Diagnostic {
                    file: ".".into(),
                    line: None,
                    code: "unsupported-linking".into(),
                    message: format!("Cargo metadata failed: {message}"),
                });
                BTreeMap::new()
            }
        }
    } else {
        BTreeMap::new()
    };
    let workspace_packages = load_workspace_packages(&built.root, &known);
    for skipped in scan.skipped {
        built.diagnostics.push(Diagnostic {
            file: normalize(&skipped.relative_path),
            line: None,
            code: "scan-skip".into(),
            message: sanitize(security, &format!("{}: {}", skipped.code, skipped.message)),
        });
    }
    if scan.schema_version != 1 {
        built.files_skipped = known.len() as u32;
        built.diagnostics.push(Diagnostic {
            file: ".".into(),
            line: None,
            code: "facts-schema-unsupported".into(),
            message: format!(
                "unsupported graph scan schema version: {}",
                scan.schema_version
            ),
        });
        return Ok(built);
    }
    for entry in scan.entries {
        cancel
            .check()
            .map_err(|e| AstGraphError::new("ast.cancelled", e))?;
        let file = normalize(&entry.relative_path);
        let parsed: RawFacts = match serde_json::from_str(&entry.facts_json) {
            Ok(v) => v,
            Err(_) => {
                built.files_skipped += 1;
                built.diagnostics.push(Diagnostic {
                    file,
                    line: None,
                    code: "facts-decode-failed".into(),
                    message: "native graph facts could not be decoded".into(),
                });
                continue;
            }
        };
        if let Some(version) = parsed.schema_version.filter(|version| *version != 1) {
            built.files_skipped += 1;
            built.diagnostics.push(Diagnostic {
                file,
                line: None,
                code: "facts-schema-unsupported".into(),
                message: format!("unsupported graph-fact schema version: {}", version),
            });
            continue;
        }
        link_file(
            &mut built,
            &known,
            file,
            parsed,
            entry
                .reference_counts
                .into_iter()
                .map(|x| (x.name, x.count))
                .collect(),
            security,
            rust_cargo_unavailable,
            &cargo_crates,
            &workspace_packages,
        );
    }
    built.diagnostics.sort();
    built.diagnostics.dedup();
    Ok(built)
}

fn has_cargo_manifest(root: &Path, known: &BTreeSet<String>) -> bool {
    if root.join("Cargo.toml").is_file() {
        return true;
    }
    known
        .iter()
        .filter(|file| file.ends_with(".rs"))
        .any(|file| {
            let mut directory = root.join(file).parent().map(Path::to_path_buf);
            while let Some(candidate) = directory {
                if !candidate.starts_with(root) {
                    return false;
                }
                if candidate.join("Cargo.toml").is_file() {
                    return true;
                }
                if candidate == root {
                    return false;
                }
                directory = candidate.parent().map(Path::to_path_buf);
            }
            false
        })
}

fn infer_root(q: &AstGraphQuery) -> Option<PathBuf> {
    let candidate = q
        .file
        .iter()
        .chain(q.target.iter())
        .chain(q.entrypoints.iter().flatten())
        .map(PathBuf::from)
        .find(|p| p.is_absolute())?;
    let rust = q.rust_workspace.as_deref() == Some("cargo")
        || candidate.extension().is_some_and(|x| x == "rs");
    let mut dir = candidate.parent()?.to_path_buf();
    let mut pkg = None;
    loop {
        if rust && dir.join("Cargo.toml").exists() {
            return Some(dir);
        }
        if pkg.is_none() && dir.join("package.json").exists() {
            if !rust {
                return Some(dir);
            }
            pkg = Some(dir.clone())
        }
        if !dir.pop() {
            break;
        }
    }
    pkg.or_else(|| candidate.parent().map(Path::to_path_buf))
}
fn sanitize(security: &ContentSecurity, text: &str) -> String {
    security.sanitize_text(text, None).content
}
fn extension(file: &str) -> &str {
    file.rsplit('.')
        .next()
        .filter(|x| !x.contains('/'))
        .unwrap_or("")
}
fn linking(ext: &str) -> &'static str {
    if matches!(
        ext,
        "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" | "mts" | "cts"
    ) {
        "javascript-relative"
    } else if ext == "rs" {
        "rust-modules"
    } else if matches!(ext, "py" | "pyi") {
        "python-modules"
    } else if matches!(ext, "c" | "h" | "cc" | "cpp" | "cxx" | "hh" | "hpp" | "hxx") {
        "c-relative-includes"
    } else {
        "unsupported"
    }
}
fn edge_kind(ext: &str, kind: &str) -> &'static str {
    if ext == "rs" {
        if kind == "module" {
            "rust-module"
        } else {
            "rust-use"
        }
    } else if kind == "type" {
        "type-import"
    } else if matches!(ext, "py" | "pyi") {
        "python-import"
    } else if matches!(ext, "c" | "h" | "cc" | "cpp" | "cxx" | "hh" | "hpp" | "hxx") {
        "c-include"
    } else {
        "static-import"
    }
}

fn link_file(
    b: &mut BuiltGraph,
    known: &BTreeSet<String>,
    file: String,
    p: RawFacts,
    counts: BTreeMap<String, u32>,
    security: &ContentSecurity,
    rust_cargo_unavailable: bool,
    cargo_crates: &BTreeMap<String, String>,
    workspace_packages: &BTreeMap<String, String>,
) {
    let ext = extension(&file).to_owned();
    let language = p.language.clone().unwrap_or_else(|| ext.clone());
    let link = linking(&ext).to_owned();
    if let Some((_, files, _)) = b
        .languages
        .iter_mut()
        .find(|(name, _, _)| name == &language)
    {
        *files += 1;
    } else {
        b.languages.push((language.clone(), 1, link.clone()));
    }
    for message in &p.diagnostics {
        let code = if message.starts_with("unsupported ") {
            "unsupported-linking"
        } else if message.starts_with("tree-sitter graph facts are syntax-only;") {
            "syntax-only"
        } else {
            "parse-recovery"
        };
        b.diagnostics.push(Diagnostic {
            file: file.clone(),
            line: None,
            code: code.into(),
            message: sanitize(security, message),
        });
    }
    if link == "unsupported" {
        b.diagnostics.push(Diagnostic {
            file: file.clone(),
            line: None,
            code: "unsupported-linking".into(),
            message: format!(
                "Import linking is unsupported for {language}; declarations are syntax facts only."
            ),
        });
    }
    let mut facts = FileFacts {
        reference_counts: counts,
        ..Default::default()
    };
    let mut node = Node::default();
    for d in p.declarations {
        let _ = &d.id;
        facts.declarations.push(Declaration {
            name: d.name,
            kind: d.kind,
            line: d.line,
            exported: d.exported,
        });
    }
    for i in p.imports {
        let target = if ext == "rs" && rust_cargo_unavailable {
            None
        } else {
            resolve(
                &i.specifier,
                &file,
                &ext,
                i.resolution_hint.as_deref(),
                i.imported_name.as_str(),
                known,
                cargo_crates,
                workspace_packages,
            )
        };
        record_resolution(
            b,
            &file,
            i.line,
            &i.specifier,
            &target,
            link == "unsupported" || ext == "rs" && rust_cargo_unavailable,
            security,
        );
        if let Some(t) = &target {
            add_edge(
                &mut node,
                t,
                edge_kind(&ext, i.import_kind.as_deref().unwrap_or("value")),
            );
            if i.imported_name == "*"
                || matches!(
                    ext.as_str(),
                    "c" | "h" | "cc" | "cpp" | "cxx" | "hh" | "hpp" | "hxx"
                )
            {
                b.namespace_targets.insert(t.clone());
            }
        }
        facts.imports.push(Import {
            imported_name: i.imported_name,
            line: i.line,
            target,
        });
        let _ = i.local_name;
    }
    for x in p.exports {
        if let Some(spec) = x.source {
            let target = resolve(
                &spec,
                &file,
                &ext,
                None,
                x.local_name.as_deref().unwrap_or(&x.name),
                known,
                cargo_crates,
                workspace_packages,
            );
            record_resolution(
                b,
                &file,
                x.line,
                &spec,
                &target,
                link == "unsupported",
                security,
            );
            if x.name == "*" {
                if let Some(t) = target {
                    let kind = if ext == "rs" {
                        "rust-use"
                    } else if x.export_kind.as_deref() == Some("type") {
                        "type-star-reexport"
                    } else {
                        "star-reexport"
                    };
                    add_edge(&mut node, &t, kind);
                    b.star_reexporters.entry(t).or_default().push(file.clone());
                }
            } else {
                if let Some(t) = &target {
                    add_edge(
                        &mut node,
                        t,
                        if x.export_kind.as_deref() == Some("type") {
                            "type-named-reexport"
                        } else {
                            "named-reexport"
                        },
                    );
                }
                facts.reexports.push(Reexport {
                    local_name: x.name.clone(),
                    imported_name: x.local_name.unwrap_or(x.name),
                    target,
                });
            }
        }
    }
    for c in p.calls {
        if c.kind.as_deref() == Some("dynamic-import") {
            let target = resolve(
                &c.callee,
                &file,
                &ext,
                None,
                "*",
                known,
                cargo_crates,
                workspace_packages,
            );
            record_resolution(
                b,
                &file,
                c.line.unwrap_or(0),
                &c.callee,
                &target,
                false,
                security,
            );
            if let Some(t) = target {
                add_edge(&mut node, &t, "dynamic-import");
                node.dynamic_only.insert(t.clone());
                b.namespace_targets.insert(t);
            }
        } else {
            facts.calls.push(Call {
                caller: c.caller,
                callee: c.callee,
            });
        }
    }
    for c in p.common_js {
        match c.specifier {
            Some(spec) => {
                let target = resolve(
                    &spec,
                    &file,
                    &ext,
                    None,
                    "*",
                    known,
                    cargo_crates,
                    workspace_packages,
                );
                record_resolution(b, &file, c.line, &spec, &target, false, security);
                if let Some(t) = target {
                    add_edge(
                        &mut node,
                        &t,
                        if c.binding.as_deref() == Some("create-require") {
                            "create-require"
                        } else {
                            "commonjs-require"
                        },
                    );
                    b.namespace_targets.insert(t);
                }
            }
            None => {
                b.imports[3] += 1;
                b.diagnostics.push(Diagnostic {
                    file: file.clone(),
                    line: Some(c.line),
                    code: "unsupported-linking".into(),
                    message: format!(
                        "CommonJS require cannot be linked ({}).",
                        c.reason.as_deref().unwrap_or("missing-native-provenance")
                    ),
                });
            }
        }
    }
    b.facts.insert(file.clone(), facts);
    b.nodes.insert(file, node);
}
fn add_edge(node: &mut Node, target: &str, kind: &str) {
    node.edges
        .entry(target.into())
        .or_default()
        .insert(kind.into());
}
fn record_resolution(
    b: &mut BuiltGraph,
    file: &str,
    line: u32,
    spec: &str,
    target: &Option<String>,
    unsupported: bool,
    security: &ContentSecurity,
) {
    if target.is_some() {
        b.imports[0] += 1
    } else if unsupported {
        b.imports[3] += 1;
        b.diagnostics.push(Diagnostic {
            file: file.into(),
            line: Some(line),
            code: "unsupported-linking".into(),
            message: sanitize(
                security,
                &format!("Cannot link import {spec:?} (unsupported)."),
            ),
        })
    } else if spec.starts_with('.') || spec.starts_with('/') {
        b.imports[2] += 1;
        b.diagnostics.push(Diagnostic {
            file: file.into(),
            line: Some(line),
            code: "unresolved-internal".into(),
            message: sanitize(
                security,
                &format!("Cannot link import {spec:?} (unresolvedInternal)."),
            ),
        })
    } else {
        b.imports[1] += 1
    }
}

fn resolve(
    spec: &str,
    importer: &str,
    ext: &str,
    hint: Option<&str>,
    imported: &str,
    known: &BTreeSet<String>,
    cargo_crates: &BTreeMap<String, String>,
    workspace_packages: &BTreeMap<String, String>,
) -> Option<String> {
    if matches!(ext, "py" | "pyi") {
        return resolve_python(spec, importer, hint, imported, known);
    }
    if matches!(ext, "c" | "h" | "cc" | "cpp" | "cxx" | "hh" | "hpp" | "hxx") {
        if hint != Some("c-relative") || spec.starts_with('/') {
            return None;
        }
        let p = join(dirname(importer), spec);
        return known.contains(&p).then_some(p);
    }
    if ext == "rs" {
        return resolve_rust(spec, importer, known, cargo_crates);
    }
    if !spec.starts_with('.') && !spec.starts_with('/') {
        let (package, subpath) = spec
            .split_once('/')
            .filter(|(name, _)| name.starts_with('@'))
            .and_then(|(scope, rest)| {
                rest.split_once('/')
                    .map(|(pkg, sub)| (format!("{scope}/{pkg}"), sub))
            })
            .unwrap_or_else(|| {
                spec.split_once('/')
                    .map(|(pkg, sub)| (pkg.to_owned(), sub))
                    .unwrap_or((spec.to_owned(), ""))
            });
        if let Some(target) = workspace_packages.get(&package) {
            if subpath.is_empty() {
                return known.contains(target).then(|| target.clone());
            }
            let joined = join(dirname(target), subpath);
            return known.contains(&joined).then_some(joined);
        }
        return None;
    }
    if spec.starts_with('/') || !spec.starts_with('.') {
        return None;
    }
    let stem = join(dirname(importer), spec);
    let exts = [".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs"];
    let mut candidates = Vec::new();
    if exts.iter().any(|x| stem.ends_with(x)) {
        candidates.push(stem.clone());
        for (a, b) in [
            (".js", ".ts"),
            (".jsx", ".tsx"),
            (".mjs", ".mts"),
            (".cjs", ".cts"),
        ] {
            if stem.ends_with(a) {
                candidates.push(format!("{}{b}", &stem[..stem.len() - a.len()]));
                break;
            }
        }
    } else {
        candidates.push(stem.clone());
        for x in exts {
            candidates.push(format!("{stem}{x}"));
        }
        for x in exts {
            candidates.push(join(&stem, &format!("index{x}")));
        }
    }
    candidates.into_iter().find(|x| known.contains(x))
}
fn resolve_rust(
    spec: &str,
    importer: &str,
    known: &BTreeSet<String>,
    cargo_crates: &BTreeMap<String, String>,
) -> Option<String> {
    let trimmed = spec.trim_end_matches(';');
    let mut parts = trimmed.split("::");
    let first = parts.next().unwrap_or("");
    if !matches!(first, "crate" | "self" | "super" | "")
        && let Some(src) = cargo_crates.get(first)
    {
        let rest = parts.collect::<Vec<_>>().join("/");
        let base = dirname(src);
        let stem = if rest.is_empty() {
            src.clone()
        } else {
            join(base, &rest)
        };
        return [stem.clone(), format!("{stem}.rs"), join(&stem, "mod.rs")]
            .into_iter()
            .find(|path| known.contains(path));
    }
    let clean = spec
        .trim_start_matches("crate::")
        .trim_start_matches("self::")
        .replace("::", "/");
    let base = if spec.starts_with("self::") {
        dirname(importer).to_owned()
    } else {
        "src".into()
    };
    let stem = join(&base, &clean);
    [format!("{stem}.rs"), join(&stem, "mod.rs")]
        .into_iter()
        .find(|x| known.contains(x))
        .or_else(|| {
            let name = spec.trim_end_matches(';').split("::").last()?;
            let stem = join(dirname(importer), name);
            [format!("{stem}.rs"), join(&stem, "mod.rs")]
                .into_iter()
                .find(|x| known.contains(x))
        })
}
fn resolve_python(
    spec: &str,
    importer: &str,
    hint: Option<&str>,
    _imported: &str,
    known: &BTreeSet<String>,
) -> Option<String> {
    if !matches!(hint, Some("python-relative") | Some("python-absolute")) {
        return None;
    }
    let dots = spec.chars().take_while(|c| *c == '.').count();
    let base = if dots > 0 { dirname(importer) } else { "." };
    let module = spec[dots..].replace('.', "/");
    let stem = join(base, &module);
    [
        format!("{stem}.py"),
        format!("{stem}.pyi"),
        join(&stem, "__init__.py"),
        join(&stem, "__init__.pyi"),
    ]
    .into_iter()
    .find(|x| known.contains(x))
}
fn dirname(p: &str) -> &str {
    p.rsplit_once('/').map(|x| x.0).unwrap_or(".")
}
fn join(a: &str, b: &str) -> String {
    normalize(&format!("{a}/{b}"))
}
pub(crate) fn normalize(p: &str) -> String {
    let replaced = p.replace('\\', "/");
    let mut parts = Vec::new();
    for c in Path::new(&replaced).components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop();
            }
            Component::Normal(x) => parts.push(x.to_string_lossy().into_owned()),
            Component::RootDir => {}
            Component::Prefix(_) => {}
        }
    }
    parts.join("/")
}

fn load_cargo_crates(root: &Path) -> Result<BTreeMap<String, String>, String> {
    let mut child = std::process::Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--offline"])
        .current_dir(root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|error| error.to_string())?;
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) | Err(_) => {
                let _ = child.kill();
                return Err("cargo metadata exited unsuccessfully".into());
            }
            Ok(None) if started.elapsed() > std::time::Duration::from_secs(5) => {
                let _ = child.kill();
                return Err("cargo metadata timed out".into());
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(20)),
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|error| error.to_string())?;
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())?;
    let mut map = BTreeMap::new();
    for package in value
        .get("packages")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
    {
        let name = package
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        for target in package
            .get("targets")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
        {
            let kinds = target
                .get("kind")
                .and_then(serde_json::Value::as_array)
                .cloned()
                .unwrap_or_default();
            let lib = kinds.iter().any(|kind| {
                kind.as_str().is_some_and(|kind| {
                    matches!(
                        kind,
                        "lib" | "rlib" | "dylib" | "cdylib" | "staticlib" | "proc-macro"
                    )
                })
            });
            if !lib {
                continue;
            }
            if let Some(src) = target.get("src_path").and_then(serde_json::Value::as_str) {
                let relative = Path::new(src).strip_prefix(root).unwrap_or(Path::new(src));
                let relative = normalize(&relative.to_string_lossy());
                map.insert(name.to_owned(), relative.clone());
                map.insert(name.replace('-', "_"), relative.clone());
                if let Some(crate_name) = target.get("name").and_then(serde_json::Value::as_str) {
                    map.insert(crate_name.replace('-', "_"), relative);
                }
            }
        }
        for dependency in package
            .get("dependencies")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
        {
            let package_name = dependency
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let alias = dependency
                .get("rename")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(package_name);
            if let Some(src) = map
                .get(package_name)
                .cloned()
                .or_else(|| map.get(&package_name.replace('-', "_")).cloned())
            {
                map.insert(alias.replace('-', "_"), src);
            }
        }
    }
    Ok(map)
}

fn load_workspace_packages(root: &Path, known: &BTreeSet<String>) -> BTreeMap<String, String> {
    let mut packages = BTreeMap::new();
    let mut manifests = vec!["package.json".to_owned()];
    manifests.extend(
        known
            .iter()
            .filter(|path| path.ends_with("package.json"))
            .cloned(),
    );
    for relative in manifests {
        let path = root.join(&relative);
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let Some(name) = value.get("name").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let directory = Path::new(&relative)
            .parent()
            .map(|parent| parent.to_string_lossy().into_owned())
            .unwrap_or_default();
        let target = export_target(&value).or_else(|| {
            value
                .get("main")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        });
        let Some(target) = target else {
            continue;
        };
        let joined = if directory.is_empty() {
            target.trim_start_matches("./").to_owned()
        } else {
            join(&directory, target.trim_start_matches("./"))
        };
        packages.insert(name.to_owned(), normalize(&joined));
    }
    packages
}

fn export_target(package: &serde_json::Value) -> Option<String> {
    let exports = package.get("exports")?;
    match exports {
        serde_json::Value::String(value) => Some(value.clone()),
        serde_json::Value::Object(map) => map
            .get(".")
            .and_then(|dot| match dot {
                serde_json::Value::String(value) => Some(value.clone()),
                serde_json::Value::Object(nested) => nested
                    .get("import")
                    .or_else(|| nested.get("default"))
                    .or_else(|| nested.get("require"))
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                _ => None,
            })
            .or_else(|| {
                map.get("import")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            })
            .or_else(|| {
                map.iter().find_map(|(pattern, value)| {
                    let Some((prefix, suffix)) = pattern.split_once('*') else {
                        return None;
                    };
                    if prefix != "./" {
                        return None;
                    }
                    match value {
                        serde_json::Value::String(target) => {
                            Some(target.replace('*', suffix.trim_start_matches('/')))
                        }
                        serde_json::Value::Object(nested) => nested
                            .get("import")
                            .or_else(|| nested.get("default"))
                            .and_then(serde_json::Value::as_str)
                            .map(|target| target.replace('*', "index")),
                        _ => None,
                    }
                })
            }),
        _ => None,
    }
}
