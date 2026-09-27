use super::packages::{PackageIndex, PackageLink};
use super::types::*;
use crate::{
    policy::path::PathPolicy, security::ContentSecurity, tools::local_fetch::CancellationCheck,
};
use octocode_engine::types::{GraphFactsScanOptions, GraphLanguageGlob};
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
    q: &AstTopologyQuery,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
) -> Result<BuiltGraph, AstGraphError> {
    cancel
        .check()
        .map_err(|e| AstGraphError::new("ast.cancelled", e))?;
    let requested_root=q.path().map(PathBuf::from).or_else(|| infer_root(q)).ok_or_else(||AstGraphError::new("invalidGraphQuery","path is required — or provide an absolute file path to infer its nearest Cargo.toml (Rust) or package.json root"))?;
    let validated = paths
        .validate(&requested_root)
        .map_err(|e| AstGraphError::new("ast.path.invalid", e.message))?;
    let mut exclude = EXCLUDES.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    if let Some(extra) = q.exclude_dir() {
        for e in extra {
            if !exclude.contains(e) {
                exclude.push(e.clone())
            }
        }
    }
    if q.max_files().is_none() {
        admit_scope(q, &validated.canonical, &exclude, paths, cancel)?;
    }
    let max_files = q.max_files().unwrap_or(20_000).clamp(1, 50_000);
    let scan = octocode_engine::portable::scan_typed_graph_facts_filtered(
        GraphFactsScanOptions {
            path: validated.canonical.to_string_lossy().into_owned(),
            exclude_dir: Some(exclude),
            max_files: Some(max_files),
            max_file_bytes: Some(1_000_000),
            language_globs: q.language_globs().as_ref().map(|map| {
                map.iter()
                    .flat_map(|(language, globs)| {
                        globs.iter().map(|glob| GraphLanguageGlob {
                            language: language.clone(),
                            glob: glob.clone(),
                        })
                    })
                    .collect()
            }),
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
        let code = if message.starts_with("[ast.language.unsupported]") {
            "ast.language.unsupported"
        } else if message.starts_with("[ast.language.invalidGlob]") {
            "ast.language.invalidGlob"
        } else if message.starts_with("[ast.execution.cancelled]") {
            "ast.cancelled"
        } else {
            "ast.graph.scanFailed"
        };
        AstGraphError::new(code, message)
    })?;
    cancel
        .check()
        .map_err(|e| AstGraphError::new("ast.cancelled", e))?;
    // A discovered path is not linkable until its facts were actually parsed.
    // Otherwise an unread or oversized target becomes a false resolved edge.
    let known: BTreeSet<String> = scan
        .entries
        .iter()
        .map(|entry| normalize(&entry.relative_path))
        .collect();
    let mut graph_builder = octocode_engine::graph::CodeGraphBuilder::new(
        validated.canonical.to_string_lossy(),
        scan.schema_version,
    );
    let mut built = BuiltGraph {
        root: validated.canonical,
        display_path: paths.redact(&requested_root),
        files_skipped: scan.files_skipped,
        truncated: scan.truncated,
        ..Default::default()
    };
    if scan.truncated || scan.files_skipped > 0 {
        graph_builder.mark_incomplete("scan-incomplete", scan.files_skipped);
    }
    let rust_cargo_unavailable = q.rust_workspace() == Some(AstTopologyQueryRustWorkspace::Cargo)
        && known.iter().any(|file| file.ends_with(".rs"))
        && !has_cargo_manifest(&built.root, &known);
    if rust_cargo_unavailable {
        built.diagnostics.push(Diagnostic {
            file: ".".into(),
            line: None,
            code: "unsupported-linking".into(),
            message: "Cargo metadata cargo-manifest-missing: No Cargo.toml was found at the scan root or known Rust-file ancestors. Point astTopology at the crate root that contains Cargo.toml (not a nested src/ directory); crate:: imports cannot be resolved otherwise.".into(),
        });
    }
    let cargo_crates = if q.rust_workspace() == Some(AstTopologyQueryRustWorkspace::Cargo)
        && !rust_cargo_unavailable
    {
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
    let workspace_packages = load_workspace_packages(&built.root, &known, paths, security);
    let packages = PackageIndex::build(&built.root, &known);
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
        let parsed: RawFacts = entry.facts;
        if parsed.schema_version != 1 {
            built.files_skipped += 1;
            built.diagnostics.push(Diagnostic {
                file,
                line: None,
                code: "facts-schema-unsupported".into(),
                message: format!(
                    "unsupported graph-fact schema version: {}",
                    parsed.schema_version
                ),
            });
            continue;
        }
        graph_builder
            .ingest_facts(&file, entry.content_digest, &parsed)
            .map_err(|error| AstGraphError::new("ast.graph.modelFailed", error))?;
        link_file(
            &mut built,
            &known,
            file,
            parsed,
            entry
                .reference_counts
                .into_iter()
                .map(|x| (x.declaration_id, x.count))
                .collect(),
            security,
            rust_cargo_unavailable,
            &cargo_crates,
            &workspace_packages,
            &packages,
            &mut graph_builder,
        )?;
    }
    // astTopology identifies results by their own digest (analysis
    // `resultId`); the whole-graph digest is never read here.
    built.code_graph = graph_builder.finish_without_digest();
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

fn infer_root(q: &AstTopologyQuery) -> Option<PathBuf> {
    let candidate = q
        .file()
        .into_iter()
        .chain(q.target())
        .chain(q.entrypoints().into_iter().flatten().map(String::as_str))
        .map(PathBuf::from)
        .find(|p| p.is_absolute())?;
    let rust = q.rust_workspace() == Some(AstTopologyQueryRustWorkspace::Cargo)
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
fn is_c_family_extension(ext: &str) -> bool {
    matches!(
        ext,
        "c" | "h" | "cc" | "cpp" | "cxx" | "hh" | "hpp" | "hxx" | "cu" | "cuh"
    )
}
fn is_javascript_extension(ext: &str) -> bool {
    matches!(
        ext,
        "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" | "mts" | "cts"
    )
}
fn linking(ext: &str) -> &'static str {
    if is_javascript_extension(ext) {
        "javascript-relative"
    } else if ext == "go" {
        "go-packages"
    } else if ext == "java" {
        "java-imports"
    } else if ext == "rs" {
        "rust-modules"
    } else if matches!(ext, "py" | "pyi") {
        "python-modules"
    } else if is_c_family_extension(ext) {
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
    } else if ext == "go" {
        "go-import"
    } else if ext == "java" {
        "java-import"
    } else if matches!(ext, "py" | "pyi") {
        "python-import"
    } else if is_c_family_extension(ext) {
        "c-include"
    } else {
        "static-import"
    }
}

/// Diagnostic prefix the tree-sitter fact producer stamps on every file.
const TREE_SITTER_SYNTAX_ONLY: &str = "tree-sitter graph facts are syntax-only;";

#[allow(clippy::too_many_arguments)]
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
    packages: &PackageIndex,
    graph_builder: &mut octocode_engine::graph::CodeGraphBuilder,
) -> Result<(), AstGraphError> {
    let ext = extension(&file).to_owned();
    let language = if p.language.is_empty() {
        ext.clone()
    } else {
        p.language.clone()
    };
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
        if message.starts_with(TREE_SITTER_SYNTAX_ONLY) {
            // Every response already declares coverage.basis = syntactic.
            // Repeating this notice for each file hides actionable gaps.
            continue;
        }
        let code = if message.starts_with("unsupported ") {
            "unsupported-linking"
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
    let syntax_only = p
        .diagnostics
        .iter()
        .any(|message| message.starts_with(TREE_SITTER_SYNTAX_ONLY));
    let mut facts = FileFacts {
        reference_counts: counts,
        reference_basis: if syntax_only {
            "syntax-references"
        } else {
            "semantic-references"
        },
        ..Default::default()
    };
    let mut node = Node::default();
    for d in p.declarations {
        facts.declarations.push(Declaration {
            id: d.id,
            name: d.name,
            kind: d.kind,
            line: d.line,
            exported: d.exported,
            exported_as: d.exported_as,
            parent: d.parent,
        });
    }
    for i in p.imports {
        if matches!(ext.as_str(), "go" | "java") {
            let targets = match packages.resolve(&ext, &i.specifier, &file) {
                PackageLink::Files(files) => files,
                PackageLink::UnresolvedInternal => {
                    b.imports[2] += 1;
                    b.diagnostics.push(Diagnostic {
                        file: file.clone(),
                        line: Some(i.line),
                        code: "unresolved-internal".into(),
                        message: sanitize(
                            security,
                            &format!("Cannot link import {:?} (unresolvedInternal).", i.specifier),
                        ),
                    });
                    Vec::new()
                }
                PackageLink::External => {
                    b.imports[1] += 1;
                    Vec::new()
                }
            };
            if !targets.is_empty() {
                b.imports[0] += 1;
            }
            for target in &targets {
                if target != &file {
                    add_edge(
                        b,
                        graph_builder,
                        &file,
                        &mut node,
                        target,
                        edge_kind(&ext, "value"),
                        i.line,
                    )?;
                }
            }
            if targets.len() > 1 {
                b.namespace_targets.extend(targets.iter().cloned());
            }
            // One import fact per linked file, so every package edge keeps
            // its import line (topology reports it as `importLine`).
            let imported_name = i.imported_name.unwrap_or_default();
            if targets.is_empty() {
                facts.imports.push(Import {
                    imported_name,
                    line: i.line,
                    target: None,
                });
            } else {
                for target in targets {
                    facts.imports.push(Import {
                        imported_name: imported_name.clone(),
                        line: i.line,
                        target: Some(target),
                    });
                }
            }
            continue;
        }
        let target = if ext == "rs" && rust_cargo_unavailable {
            None
        } else {
            resolve(
                &i.specifier,
                &file,
                &ext,
                i.resolution_hint.as_deref(),
                i.imported_name.as_deref().unwrap_or_default(),
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
            &ext,
            &target,
            link == "unsupported" || ext == "rs" && rust_cargo_unavailable,
            security,
        );
        if let Some(t) = &target {
            add_edge(
                b,
                graph_builder,
                &file,
                &mut node,
                t,
                edge_kind(
                    &ext,
                    if i.import_kind.is_empty() {
                        "value"
                    } else {
                        &i.import_kind
                    },
                ),
                i.line,
            )?;
            if i.imported_name.as_deref() == Some("*") || is_c_family_extension(&ext) {
                b.namespace_targets.insert(t.clone());
            }
        }
        facts.imports.push(Import {
            imported_name: i.imported_name.unwrap_or_default(),
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
                &ext,
                &target,
                link == "unsupported",
                security,
            );
            if x.name == "*" {
                if let Some(t) = target {
                    let kind = if ext == "rs" {
                        "rust-use"
                    } else if x.export_kind == "type" {
                        "type-star-reexport"
                    } else {
                        "star-reexport"
                    };
                    add_edge(b, graph_builder, &file, &mut node, &t, kind, x.line)?;
                    b.star_reexporters.entry(t).or_default().push(file.clone());
                }
            } else {
                if let Some(t) = &target {
                    add_edge(
                        b,
                        graph_builder,
                        &file,
                        &mut node,
                        t,
                        if x.export_kind == "type" {
                            "type-named-reexport"
                        } else {
                            "named-reexport"
                        },
                        x.line,
                    )?;
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
        if c.kind == "dynamic-import" {
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
            record_resolution(b, &file, c.line, &c.callee, &ext, &target, false, security);
            if let Some(t) = target {
                add_edge(
                    b,
                    graph_builder,
                    &file,
                    &mut node,
                    &t,
                    "dynamic-import",
                    c.line,
                )?;
                node.dynamic_only.insert(t.clone());
                b.namespace_targets.insert(t);
            }
        } else {
            facts.calls.push(Call {
                caller_id: c.caller_id,
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
                record_resolution(b, &file, c.line, &spec, &ext, &target, false, security);
                if let Some(t) = target {
                    add_edge(
                        b,
                        graph_builder,
                        &file,
                        &mut node,
                        &t,
                        if c.binding == "create-require" {
                            "create-require"
                        } else {
                            "commonjs-require"
                        },
                        c.line,
                    )?;
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
    Ok(())
}

/// Upper bound on file-graph edges. File count is already capped by the scan
/// (`max_files` ≤ 50k), but per-file import counts are attacker-sized; without
/// an edge cap a pathological tree can exhaust memory in the edge maps and the
/// evidence graph. Hitting the cap degrades to a truncated (diagnosed) graph.
const MAX_GRAPH_EDGES: u32 = 2_000_000;

fn add_edge(
    b: &mut BuiltGraph,
    graph_builder: &mut octocode_engine::graph::CodeGraphBuilder,
    source: &str,
    node: &mut Node,
    target: &str,
    kind: &str,
    line: u32,
) -> Result<(), AstGraphError> {
    if b.edge_count >= MAX_GRAPH_EDGES {
        if !b.edges_capped {
            b.edges_capped = true;
            b.truncated = true;
            b.diagnostics.push(Diagnostic {
                file: ".".into(),
                line: None,
                code: "graph-edge-cap".into(),
                message: format!(
                    "Edge collection stopped at the {MAX_GRAPH_EDGES}-edge cap; topology results are partial. Narrow the scan root or excludeDir."
                ),
            });
        }
        return Ok(());
    }
    b.edge_count += 1;
    node.edges
        .entry(target.into())
        .or_default()
        .insert(kind.into());
    graph_builder
        .add_file_relation(source, target, kind, line)
        .map_err(|error| AstGraphError::new("ast.graph.modelFailed", error))
}
/// A Rust `use` path that unambiguously targets the current crate, so failing to
/// resolve it means the intra-crate edge graph is incomplete — never a benign
/// external dependency. `crate::`/`self::`/`super::` and leading `::` qualify.
fn rust_internal_specifier(spec: &str) -> bool {
    if spec.starts_with("::") {
        return true;
    }
    let first = spec
        .trim_start_matches("::")
        .split("::")
        .next()
        .unwrap_or("");
    matches!(first, "crate" | "self" | "super")
}

/// A path specifier naming data, style, or asset content (`./package.json`,
/// `./app.css`, `./logo.svg?url`). Such imports are bundler/loader concerns,
/// never code-graph edges, so failing to link them is not a coverage gap.
/// Python is excluded: its dotted relative modules (`.utils.json`) are code.
fn is_non_code_specifier(spec: &str) -> bool {
    const NON_CODE: &[&str] = &[
        "json", "jsonc", "json5", "css", "scss", "sass", "less", "styl", "pcss", "svg", "png",
        "jpg", "jpeg", "gif", "webp", "avif", "ico", "bmp", "tif", "tiff", "woff", "woff2", "ttf",
        "otf", "eot", "mp3", "mp4", "webm", "wav", "ogg", "txt", "md", "html", "htm", "wasm",
        "node", "yaml", "yml", "toml", "graphql", "gql", "csv", "xml", "pdf",
    ];
    let path = spec.split(['?', '#']).next().unwrap_or(spec);
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.').is_some_and(|(stem, ext)| {
        !stem.is_empty() && NON_CODE.contains(&ext.to_ascii_lowercase().as_str())
    })
}

#[allow(clippy::too_many_arguments)]
fn record_resolution(
    b: &mut BuiltGraph,
    file: &str,
    line: u32,
    spec: &str,
    ext: &str,
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
    } else if !matches!(ext, "py" | "pyi")
        && (spec.starts_with('.') || spec.starts_with('/'))
        && is_non_code_specifier(spec)
    {
        b.imports[4] += 1
    } else if spec.starts_with('.')
        || spec.starts_with('/')
        || (ext == "rs" && rust_internal_specifier(spec))
    {
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

#[allow(clippy::too_many_arguments)]
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
    if is_c_family_extension(ext) {
        if hint != Some("c-relative") || spec.starts_with('/') {
            return None;
        }
        let p = join_within_root(dirname(importer), spec)?;
        return known.contains(&p).then_some(p);
    }
    if ext == "rs" {
        return resolve_rust(spec, importer, known, cargo_crates);
    }
    if !is_javascript_extension(ext) {
        return None;
    }
    if !spec.starts_with('.') && !spec.starts_with('/') {
        let (package, subpath) = if spec.starts_with('@') {
            let mut parts = spec.splitn(3, '/');
            let scope = parts.next()?;
            let name = parts.next()?;
            (format!("{scope}/{name}"), parts.next().unwrap_or(""))
        } else {
            spec.split_once('/')
                .map(|(pkg, sub)| (pkg.to_owned(), sub))
                .unwrap_or((spec.to_owned(), ""))
        };
        if let Some(target) = workspace_packages.get(&package) {
            if subpath.is_empty() {
                return known.contains(target).then(|| target.clone());
            }
            let joined = join_within_root(dirname(target), subpath)?;
            return known.contains(&joined).then_some(joined);
        }
        return None;
    }
    if spec.starts_with('/') || !spec.starts_with('.') {
        return None;
    }
    let stem = join_within_root(dirname(importer), spec)?;
    let exts = [".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".mts", ".cts"];
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
/// The directory holding the importer's child modules: `foo/` for
/// `foo/mod.rs` (and crate roots), `foo/bar/` for `foo/bar.rs`.
fn rust_module_child_dir(importer: &str) -> String {
    let name = importer.rsplit_once('/').map_or(importer, |x| x.1);
    if matches!(name, "mod.rs" | "lib.rs" | "main.rs") {
        dirname(importer).to_owned()
    } else {
        importer.strip_suffix(".rs").unwrap_or(importer).to_owned()
    }
}

/// The importer's crate-root directory: the nearest ancestor holding a
/// `lib.rs`/`main.rs` in the scanned set, else the nearest ancestor named
/// `src` (a multi-crate workspace has many `src/` roots — anchoring to the
/// importer's own is what makes `crate::` resolvable outside a single-crate
/// scan), else the scan-relative `src`.
fn rust_crate_root(importer: &str, known: &BTreeSet<String>) -> String {
    let mut dir = dirname(importer);
    loop {
        if known.contains(&join(dir, "lib.rs")) || known.contains(&join(dir, "main.rs")) {
            return dir.to_owned();
        }
        let parent = dirname(dir);
        if parent == dir || dir == "." {
            break;
        }
        dir = parent;
    }
    let mut dir = dirname(importer);
    loop {
        if dir == "src" || dir.ends_with("/src") {
            return dir.to_owned();
        }
        let parent = dirname(dir);
        if parent == dir || dir == "." {
            break;
        }
        dir = parent;
    }
    "src".into()
}

/// Map module-path segments under `base` to a file: trailing segments may be
/// items (types, functions) or globs rather than modules, so take the
/// longest prefix that names a real file; with no resolvable segment the
/// path denotes the base module itself (`use super::*;`).
fn resolve_rust_module_prefix(
    base: &str,
    segments: &[&str],
    known: &BTreeSet<String>,
) -> Option<String> {
    let module_len = segments
        .iter()
        .take_while(|segment| !segment.is_empty() && !segment.contains(['{', '*']))
        .count();
    (1..=module_len)
        .rev()
        .find_map(|len| {
            let stem = join(base, &segments[..len].join("/"));
            [format!("{stem}.rs"), join(&stem, "mod.rs")]
                .into_iter()
                .find(|path| known.contains(path))
        })
        .or_else(|| {
            [
                format!("{base}.rs"),
                join(base, "mod.rs"),
                join(base, "lib.rs"),
                join(base, "main.rs"),
            ]
            .into_iter()
            .find(|path| known.contains(path))
        })
}

fn resolve_rust(
    spec: &str,
    importer: &str,
    known: &BTreeSet<String>,
    cargo_crates: &BTreeMap<String, String>,
) -> Option<String> {
    let trimmed = spec.trim_end_matches(';');
    let segments = trimmed.split("::").collect::<Vec<_>>();
    let first = *segments.first().unwrap_or(&"");
    if !matches!(first, "crate" | "self" | "super" | "")
        && let Some(src) = cargo_crates.get(first)
    {
        let rest = &segments[1..];
        let base = dirname(src);
        if rest.is_empty() {
            return known.contains(src).then(|| src.clone());
        }
        return resolve_rust_module_prefix(base, rest, known)
            .or_else(|| known.contains(src).then(|| src.clone()));
    }
    match first {
        "crate" => {
            let base = rust_crate_root(importer, known);
            resolve_rust_module_prefix(&base, &segments[1..], known)
        }
        "self" => {
            let base = rust_module_child_dir(importer);
            resolve_rust_module_prefix(&base, &segments[1..], known)
        }
        "super" => {
            // Each `super` climbs one module level from the importer's own
            // module directory.
            let mut base = rust_module_child_dir(importer);
            let mut index = 0;
            while segments.get(index) == Some(&"super") {
                base = dirname(&base).to_owned();
                index += 1;
            }
            resolve_rust_module_prefix(&base, &segments[index..], known)
        }
        // Leading `::` names an external crate absolutely (2015-style);
        // nothing to link inside this scan unless cargo metadata knows it.
        "" => segments
            .get(1)
            .and_then(|name| cargo_crates.get(*name))
            .filter(|src| known.contains(*src))
            .cloned(),
        _ => {
            // Uniform-path fallback: a bare `use foo::…` can name a module the
            // importer itself declares (`mod foo;`), whose file lives in the
            // importer's child-module directory. Only look there — matching
            // the last path segment anywhere in the tree fabricates edges
            // between unrelated same-named modules.
            let child_dir = rust_module_child_dir(importer);
            (1..=segments.len()).rev().find_map(|len| {
                let stem = join(&child_dir, &segments[..len].join("/"));
                [format!("{stem}.rs"), join(&stem, "mod.rs")]
                    .into_iter()
                    .find(|path| known.contains(path))
            })
        }
    }
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
    let mut base = if dots > 0 { dirname(importer) } else { "." };
    for _ in 1..dots {
        if base == "." {
            return None;
        }
        base = dirname(base);
    }
    let module = spec[dots..].replace('.', "/");
    let stem = join_within_root(base, &module)?;
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
fn join_within_root(a: &str, b: &str) -> Option<String> {
    let mut parts = Vec::new();
    for component in Path::new(&format!("{a}/{b}")).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop()?;
            }
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(parts.join("/"))
}
/// Implicit-default scans larger than this are refused before parsing: a
/// monorepo root otherwise parses thousands of files and can exhaust the call
/// deadline before any analysis. An explicit maxFiles opts in to a full scan.
pub(crate) const SCOPE_ADMISSION_FILES: u32 = 5_000;
const SCOPE_COUNT_FILES: u32 = 100_000;

/// Cheap discovery-only preflight (no parsing) for an implicit-default scan.
fn admit_scope(
    q: &AstTopologyQuery,
    root: &Path,
    exclude: &[String],
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<(), AstGraphError> {
    let found = octocode_engine::portable::query_file_system_filtered(
        octocode_engine::types::FileSystemQueryOptions {
            path: root.to_string_lossy().into_owned(),
            recursive: Some(true),
            show_hidden: Some(false),
            entry_type: Some("f".to_owned()),
            extensions: Some(octocode_engine::signatures::graph_facts::graph_fact_extensions()),
            exclude_dir: Some(exclude.to_vec()),
            // Discovery is cheap; count well past the admission bound so the
            // per-directory suggestions reflect the whole root, not the
            // first directories walked.
            stop_at_limit: Some(true),
            limit: Some(SCOPE_COUNT_FILES),
            ..Default::default()
        },
        &|path| {
            cancel.check()?;
            Ok(paths.permits_discovery(path))
        },
    )
    .map_err(|e| AstGraphError::new("ast.graph.scanFailed", e.to_string()))?;
    if found.entries.len() as u32 <= SCOPE_ADMISSION_FILES {
        return Ok(());
    }
    let mut by_dir: BTreeMap<String, u32> = BTreeMap::new();
    for entry in &found.entries {
        let relative = normalize(&entry.relative_path);
        let mut parts = relative.split('/');
        let (Some(first), Some(second)) = (parts.next(), parts.next()) else {
            continue;
        };
        // Group by package-level directory (packages/x, crates/y) when the
        // first segment is a workspace container, else by the first segment.
        let key = if parts.next().is_some() && WORKSPACE_CONTAINERS.contains(&first) {
            format!("{first}/{second}")
        } else {
            first.to_owned()
        };
        *by_dir.entry(key).or_default() += 1;
    }
    let mut ranked: Vec<(String, u32)> = by_dir.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let narrower: Vec<&(String, u32)> = ranked
        .iter()
        .filter(|(_, count)| *count <= SCOPE_ADMISSION_FILES)
        .take(5)
        .collect();
    let root_display = root.to_string_lossy().into_owned();
    let listed = narrower
        .iter()
        .map(|(dir, count)| format!("{dir} ({count})"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut error = AstGraphError::new(
        "ast.graph.scopeTooBroad",
        format!(
            "More than {SCOPE_ADMISSION_FILES} parseable files under this root; a full parse can exhaust the call deadline before analysis. Admissible roots by file count: {}.",
            if listed.is_empty() {
                "none below the bound"
            } else {
                listed.as_str()
            }
        ),
    );
    // Response shaping keeps one concise hint per row.
    error.hints = vec!["Run next.narrowScope, pick another listed root, or set maxFiles (next.expandScan) to opt in.".into()];
    let continuation = |path: String, max_files: Option<u32>| {
        let mut query = serde_json::to_value(q).unwrap_or_default();
        if let Some(object) = query.as_object_mut() {
            object.retain(|_, value| !value.is_null());
            object.insert("path".into(), serde_json::json!(path));
            if let Some(max_files) = max_files {
                object.insert("maxFiles".into(), serde_json::json!(max_files));
            }
            object.insert(
                "reasoning".into(),
                serde_json::json!("Rerun topology within an admitted scan scope."),
            );
        }
        query
    };
    let mut next = serde_json::Map::new();
    if let Some((dir, _)) = narrower.first() {
        next.insert(
            "narrowScope".into(),
            serde_json::json!({
                "tool": "astTopology",
                "confidence": "medium",
                "query": continuation(format!("{root_display}/{dir}"), None),
            }),
        );
    }
    next.insert(
        "expandScan".into(),
        serde_json::json!({
            "tool": "astTopology",
            "confidence": "low",
            "query": continuation(root_display, Some(20_000)),
        }),
    );
    error.next = Some(Box::new(serde_json::Value::Object(next)));
    Err(error)
}

const WORKSPACE_CONTAINERS: &[&str] =
    &["packages", "crates", "apps", "libs", "services", "modules"];

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

/// `cargo metadata` output per workspace root, reused while the root
/// manifest and lockfile are unchanged (size + mtime). The TTL bounds staleness
/// from glob-added members, which do not touch the root manifest.
fn load_cargo_crates(root: &Path) -> Result<BTreeMap<String, String>, String> {
    use std::sync::{LazyLock, Mutex};
    use std::time::{Duration, Instant, SystemTime};
    type Stamp = [(u64, Option<SystemTime>); 2];
    type Entry = (Stamp, Instant, BTreeMap<String, String>);
    static CACHE: LazyLock<Mutex<BTreeMap<std::path::PathBuf, Entry>>> =
        LazyLock::new(|| Mutex::new(BTreeMap::new()));
    const TTL: Duration = Duration::from_secs(300);
    let stamp = |name: &str| {
        std::fs::metadata(root.join(name))
            .map(|m| (m.len(), m.modified().ok()))
            .unwrap_or((0, None))
    };
    let current: Stamp = [stamp("Cargo.toml"), stamp("Cargo.lock")];
    if let Ok(cache) = CACHE.lock()
        && let Some((cached, at, crates)) = cache.get(root)
        && *cached == current
        && at.elapsed() < TTL
    {
        return Ok(crates.clone());
    }
    let crates = run_cargo_metadata(root)?;
    if let Ok(mut cache) = CACHE.lock() {
        cache.insert(
            root.to_path_buf(),
            (current, Instant::now(), crates.clone()),
        );
    }
    Ok(crates)
}

fn run_cargo_metadata(root: &Path) -> Result<BTreeMap<String, String>, String> {
    const MAX_METADATA_BYTES: usize = 32 * 1024 * 1024;
    // Resolve cargo from an explicit env-provided path when available rather than
    // trusting the ambient PATH against an untrusted working directory. `--no-deps`
    // keeps metadata to the workspace's own crates, cutting work and attack surface.
    let cargo = std::env::var_os("OCTOCODE_CARGO")
        .or_else(|| std::env::var_os("CARGO"))
        .unwrap_or_else(|| std::ffi::OsString::from("cargo"));
    let mut child = std::process::Command::new(&cargo)
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--offline",
        ])
        .current_dir(root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|error| error.to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "cargo metadata stdout was unavailable".to_owned())?;
    let reader = std::thread::spawn(move || {
        use std::io::Read;
        let mut stdout = stdout;
        let mut bytes = Vec::new();
        let mut chunk = [0_u8; 16 * 1024];
        let mut exceeded = false;
        loop {
            let read = stdout.read(&mut chunk).map_err(|error| error.to_string())?;
            if read == 0 {
                break;
            }
            if bytes.len().saturating_add(read) <= MAX_METADATA_BYTES {
                bytes.extend_from_slice(&chunk[..read]);
            } else {
                exceeded = true;
            }
        }
        if exceeded {
            Err("cargo metadata exceeded the 32 MiB output limit".to_owned())
        } else {
            Ok(bytes)
        }
    });
    let started = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err(error.to_string());
            }
            Ok(None) if started.elapsed() > std::time::Duration::from_secs(5) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err("cargo metadata timed out".into());
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(20)),
        }
    };
    let bytes = reader
        .join()
        .map_err(|_| "cargo metadata output reader failed".to_owned())??;
    if !status.success() {
        return Err("cargo metadata exited unsuccessfully".into());
    }
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
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

fn load_workspace_packages(
    root: &Path,
    known: &BTreeSet<String>,
    paths: &PathPolicy,
    security: &ContentSecurity,
) -> BTreeMap<String, String> {
    let mut packages = BTreeMap::new();
    // The graph scan includes code extensions, never JSON. Discover package
    // manifests along scanned files' ancestors instead of looking for them in
    // `known`, which cannot contain package.json.
    let mut manifests = BTreeSet::from(["package.json".to_owned()]);
    for file in known {
        let mut parent = Path::new(file).parent();
        while let Some(directory) = parent {
            if directory.as_os_str().is_empty() {
                break;
            }
            manifests.insert(
                directory
                    .join("package.json")
                    .to_string_lossy()
                    .into_owned(),
            );
            parent = directory.parent();
        }
    }
    for relative in manifests {
        let path = root.join(&relative);
        let Ok(validated) = paths.validate_read(&path) else {
            continue;
        };
        let Ok(bytes) = std::fs::read(&validated.canonical) else {
            continue;
        };
        let Ok(safe) = security.validate_text_bytes(&bytes, Some(&validated.canonical), 1_000_000)
        else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&safe.content) else {
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
        let Some(joined) = join_within_root(
            if directory.is_empty() {
                "."
            } else {
                &directory
            },
            &target,
        ) else {
            continue;
        };
        packages.insert(name.to_owned(), joined);
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
                    let (prefix, suffix) = pattern.split_once('*')?;
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

#[cfg(test)]
mod tests {
    #[test]
    fn python_parent_imports_climb_the_declared_number_of_packages() {
        let known = ["pkg/shared.py", "pkg/sub/shared.py", "shared.py"]
            .iter()
            .map(|path| (*path).to_owned())
            .collect();
        assert_eq!(
            super::resolve_python(
                "..shared",
                "pkg/sub/worker.py",
                Some("python-relative"),
                "run",
                &known,
            ),
            Some("pkg/shared.py".to_owned())
        );
        assert_eq!(
            super::resolve_python(
                "....shared",
                "pkg/sub/worker.py",
                Some("python-relative"),
                "run",
                &known,
            ),
            None,
            "relative imports cannot climb above the scan root"
        );
    }

    #[test]
    fn relative_javascript_import_cannot_reenter_after_escaping_scan_root() {
        let known = ["target.ts"]
            .iter()
            .map(|path| (*path).to_owned())
            .collect();
        assert_eq!(
            super::resolve(
                "../target",
                "entry.ts",
                "ts",
                None,
                "*",
                &known,
                &Default::default(),
                &Default::default(),
            ),
            None,
        );
    }

    #[test]
    fn extensionless_javascript_import_links_supported_module_extensions() {
        let known = ["src/module.mts", "src/legacy.cts"]
            .iter()
            .map(|path| (*path).to_owned())
            .collect();
        for (specifier, expected) in [
            ("./module", "src/module.mts"),
            ("./legacy", "src/legacy.cts"),
        ] {
            assert_eq!(
                super::resolve(
                    specifier,
                    "src/entry.ts",
                    "ts",
                    None,
                    "*",
                    &known,
                    &Default::default(),
                    &Default::default(),
                ),
                Some(expected.to_owned()),
            );
        }
    }

    #[test]
    fn unsupported_languages_do_not_use_javascript_package_resolution() {
        let known = ["src/index.ts"]
            .iter()
            .map(|path| (*path).to_owned())
            .collect();
        let packages = [("example".to_owned(), "src/index.ts".to_owned())]
            .into_iter()
            .collect();
        assert_eq!(
            super::resolve(
                "example",
                "main.go",
                "go",
                None,
                "*",
                &known,
                &Default::default(),
                &packages,
            ),
            None,
        );
    }

    #[test]
    fn rust_internal_paths_resolve_across_workspace_crates() {
        let known = [
            "crates/runtime/src/lib.rs",
            "crates/runtime/src/config/mod.rs",
            "crates/runtime/src/config/types.rs",
            "crates/runtime/src/tools/mod.rs",
            "crates/runtime/src/tools/local_fetch.rs",
            "crates/engine/src/lib.rs",
            "crates/engine/src/lsp.rs",
        ]
        .iter()
        .map(|path| (*path).to_owned())
        .collect::<std::collections::BTreeSet<_>>();
        let crates = std::collections::BTreeMap::new();
        // `crate::` anchors to the importer's own crate root, not a global
        // `src/` — the multi-crate workspace case.
        assert_eq!(
            super::resolve_rust(
                "crate::config::types::ResolvedConfig;",
                "crates/runtime/src/tools/local_fetch.rs",
                &known,
                &crates
            ),
            Some("crates/runtime/src/config/types.rs".to_owned())
        );
        // Trailing item segments fall back to the longest module prefix.
        assert_eq!(
            super::resolve_rust(
                "crate::config::PROTECTED_KEYS;",
                "crates/runtime/src/tools/local_fetch.rs",
                &known,
                &crates
            ),
            Some("crates/runtime/src/config/mod.rs".to_owned())
        );
        // `super::` climbs one module level per segment; an item-only tail
        // resolves to the parent module file.
        assert_eq!(
            super::resolve_rust(
                "super::PROTECTED_KEYS;",
                "crates/runtime/src/config/types.rs",
                &known,
                &crates
            ),
            Some("crates/runtime/src/config/mod.rs".to_owned())
        );
        assert_eq!(
            super::resolve_rust(
                "super::super::tools::local_fetch::Row;",
                "crates/runtime/src/config/types.rs",
                &known,
                &crates
            ),
            Some("crates/runtime/src/tools/local_fetch.rs".to_owned())
        );
        // `use super::*;` denotes the parent module file itself.
        assert_eq!(
            super::resolve_rust(
                "super::*;",
                "crates/runtime/src/config/types.rs",
                &known,
                &crates
            ),
            Some("crates/runtime/src/config/mod.rs".to_owned())
        );
        // Grouped imports resolve to the deepest real module prefix.
        assert_eq!(
            super::resolve_rust(
                "crate::config::{types, validation};",
                "crates/runtime/src/tools/local_fetch.rs",
                &known,
                &crates
            ),
            Some("crates/runtime/src/config/mod.rs".to_owned())
        );
        // Importers in a different crate anchor to their own root.
        assert_eq!(
            super::resolve_rust(
                "crate::lsp::Pool;",
                "crates/engine/src/lsp.rs",
                &known,
                &crates
            ),
            Some("crates/engine/src/lsp.rs".to_owned())
        );
    }

    #[test]
    fn rust_uniform_path_fallback_only_links_the_importers_child_modules() {
        let known = ["src/a.rs", "src/thing.rs", "src/b.rs", "src/b/child.rs"]
            .iter()
            .map(|path| (*path).to_owned())
            .collect::<std::collections::BTreeSet<_>>();
        let crates = std::collections::BTreeMap::new();
        // A same-named file elsewhere in the tree must not become an edge for a
        // path that is not one of the importer's own child modules.
        assert_eq!(
            super::resolve_rust("unrelated::thing;", "src/a.rs", &known, &crates),
            None
        );
        // A module the importer declares (`mod child;`) lives in its child
        // directory and resolves.
        assert_eq!(
            super::resolve_rust("child::item;", "src/b.rs", &known, &crates),
            Some("src/b/child.rs".to_owned())
        );
        // Root-style importers (lib.rs/main.rs/mod.rs) keep sibling resolution.
        assert_eq!(
            super::resolve_rust("thing::item;", "src/lib.rs", &known, &crates),
            Some("src/thing.rs".to_owned())
        );
    }

    #[test]
    fn cargo_metadata_stdout_is_drained_while_the_child_runs() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let crates = super::load_cargo_crates(root).expect("cargo metadata");
        assert_eq!(
            crates.get("octocode_native").map(String::as_str),
            Some("src/lib.rs")
        );
    }
}
