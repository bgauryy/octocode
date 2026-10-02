use super::aliases::{ResolveContext, probe_js};
use super::cargo::{CargoCrates, load_cargo_crates};
use super::packages::{PackageIndex, PackageLink};
use super::types::*;
use crate::tools::id::ToolId;
use crate::{
    policy::{gitignore::GitignoreFilter, path::PathPolicy},
    security::ContentSecurity,
    tools::cancel::CancellationCheck,
};
use octocode_engine::types::{GraphFactsScanOptions, GraphLanguageGlob};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
};

/// Scan options beyond the public astTopology query (used by persisted
/// graph ingest). The default keeps astTopology's behavior.
#[derive(Clone, Debug, Default)]
pub(crate) struct BuildExtras {
    /// Skip paths ignored by `.gitignore` files (scan root, nested, and
    /// ancestors up to the repository root) and `.git/info/exclude`.
    pub respect_gitignore: bool,
    /// Additional directory names excluded from the scan.
    pub extra_excludes: Vec<String>,
}

pub(crate) fn build_graph(
    q: &AstTopologyQuery,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
) -> Result<BuiltGraph, AstGraphError> {
    build_graph_with(q, paths, security, cancel, &BuildExtras::default())
}

pub(crate) fn build_graph_with(
    q: &AstTopologyQuery,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
    extras: &BuildExtras,
) -> Result<BuiltGraph, AstGraphError> {
    cancel
        .check()
        .map_err(|e| AstGraphError::new("ast.cancelled", e))?;
    let requested_root = q
        .path()
        .map(PathBuf::from)
        .or_else(|| infer_root(q))
        .ok_or_else(|| missing_root_error(q))?;
    let validated = paths
        .validate(&requested_root)
        .map_err(|e| AstGraphError::new("ast.path.invalid", e.message))?;
    let requested = q
        .exclude_dir()
        .unwrap_or_default()
        .iter()
        .chain(&extras.extra_excludes)
        .cloned()
        .collect::<Vec<_>>();
    let exclude = crate::policy::prune::PruneMode::SyntaxVisible
        .directories(&requested, q.default_excludes());
    let gitignore = extras
        .respect_gitignore
        .then(|| GitignoreFilter::new(&validated.canonical));
    if q.max_files().is_none() {
        admit_scope(
            q,
            &validated.canonical,
            &exclude,
            paths,
            gitignore.as_ref(),
            cancel,
        )?;
    }
    let max_files = q
        .max_files()
        .unwrap_or(20_000)
        .clamp(1, super::topology_max("maxFiles"));
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
            if gitignore
                .as_ref()
                .is_some_and(|filter| filter.is_ignored(path))
            {
                return Ok(false);
            }
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
    // Only drift diffs the evidence graph; every other analysis runs on
    // `BuiltGraph`, so the (costly) evidence graph is built for drift only.
    let mut graph_builder = (q.analysis() == super::GraphAnalysis::Drift).then(|| {
        octocode_engine::graph::CodeGraphBuilder::new(
            validated.canonical.to_string_lossy(),
            scan.schema_version,
        )
    });
    let mut built = BuiltGraph {
        root: validated.canonical,
        display_path: paths.redact(&requested_root),
        files_skipped: scan.files_skipped,
        truncated: scan.truncated,
        ..Default::default()
    };
    if (scan.truncated || scan.files_skipped > 0)
        && let Some(builder) = graph_builder.as_mut()
    {
        builder.mark_incomplete("scan-incomplete", scan.files_skipped);
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
                CargoCrates::default()
            }
        }
    } else {
        CargoCrates::default()
    };
    let resolve_context = ResolveContext::load(&built.root, &known, paths, security);
    let packages = PackageIndex::build(&built.root, &known, &|path| {
        paths.validate_read(path).is_ok()
    });
    if packages.go_module_missing() {
        built.diagnostics.push(Diagnostic {
            file: ".".into(),
            line: None,
            code: "unsupported-linking".into(),
            message: "No readable go.mod at or above the scan root, so Go module imports cannot be linked and dependents are unproven. Point astTopology at the module root (the directory with go.mod).".into(),
        });
    }
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
        if let Some(builder) = graph_builder.as_mut() {
            builder
                .ingest_facts(&file, entry.content_digest.clone(), &parsed)
                .map_err(|error| AstGraphError::new("ast.graph.modelFailed", error))?;
        }
        let digest = entry.content_digest;
        link_file(
            &mut built,
            &known,
            file.clone(),
            parsed,
            entry
                .reference_counts
                .into_iter()
                .map(|x| (x.declaration_id, x.count))
                .collect(),
            security,
            rust_cargo_unavailable,
            &cargo_crates,
            &resolve_context,
            &packages,
            graph_builder.as_mut(),
        )?;
        if let Some(facts) = built.facts.get_mut(&file) {
            facts.digest = digest;
        }
    }
    // astTopology identifies results by their own digest (analysis
    // `resultId`); the whole-graph digest is never read here.
    if let Some(builder) = graph_builder {
        built.code_graph = builder.finish_without_digest();
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

/// No `path` and no absolute file to infer a root from: the caller must name
/// the scan root, so this is an input error, not an execution failure.
fn missing_root_error(q: &AstTopologyQuery) -> AstGraphError {
    let message = match q.file().or(q.target()) {
        Some(relative) => format!(
            "path is required: file {relative:?} is relative and no scan root was given. Pass path:<absolute project root> (file then resolves against it) or an absolute file (its nearest Cargo.toml or package.json becomes the root)."
        ),
        None => "path is required: pass path:<absolute project root>, or an absolute file whose nearest Cargo.toml or package.json becomes the root.".to_owned(),
    };
    let mut error = AstGraphError::new("ast.input.invalid", message);
    error.hints =
        vec!["Add path with the absolute project root the relative file lives under.".into()];
    error
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
    cargo_crates: &CargoCrates,
    ctx: &ResolveContext,
    packages: &PackageIndex,
    mut graph_builder: Option<&mut octocode_engine::graph::CodeGraphBuilder>,
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
        language: language.clone(),
        ..Default::default()
    };
    let mut node = Node::default();
    // A bare JS/TS specifier naming project code (alias, workspace package,
    // `#`/`@/`/`~/` convention): failing to link it is an internal gap.
    let internal_bare = |spec: &str| {
        is_javascript_extension(&ext)
            && !spec.starts_with('.')
            && !spec.starts_with('/')
            && ctx.is_internal_bare_js(spec, &file)
    };
    for edge in &p.edges {
        if matches!(edge.relation.as_str(), "extends" | "implements") {
            facts.heritage.push(Heritage {
                decl_id: edge.from.clone(),
                relation: edge.relation.clone(),
                target: edge.to.clone(),
                line: edge.line,
            });
        }
    }
    for d in p.declarations {
        facts.declarations.push(Declaration {
            id: d.id,
            name: d.name,
            kind: d.kind,
            line: d.line,
            end_line: d.line + d.range.end.line.saturating_sub(d.range.start.line),
            exported: d.exported,
            exported_as: d.exported_as,
            parent: d.parent,
        });
    }
    for i in p.imports {
        if matches!(ext.as_str(), "go" | "java") {
            let mut external = false;
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
                    external = true;
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
                        graph_builder.as_deref_mut(),
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
                    local_name: i.local_name,
                    specifier: i.specifier,
                    line: i.line,
                    target: None,
                    external,
                    used_in: None,
                });
            } else {
                for target in targets {
                    facts.imports.push(Import {
                        imported_name: imported_name.clone(),
                        local_name: i.local_name.clone(),
                        specifier: i.specifier.clone(),
                        line: i.line,
                        target: Some(target),
                        external: false,
                        used_in: None,
                    });
                }
            }
            continue;
        }
        let target = if ext == "rs"
            && let Some(local) = rust_local_target(&i, &file, &p.modules, known)
        {
            Some(local)
        } else if ext == "rs" && rust_cargo_unavailable {
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
                ctx,
            )
        };
        let external = record_resolution(
            b,
            &file,
            i.line,
            &i.specifier,
            &ext,
            &target,
            link == "unsupported" || ext == "rs" && rust_cargo_unavailable,
            target.is_none() && internal_bare(&i.specifier),
            security,
        );
        if let Some(t) = target.as_ref().filter(|t| **t != file) {
            add_edge(
                b,
                graph_builder.as_deref_mut(),
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
            local_name: i.local_name,
            specifier: i.specifier,
            line: i.line,
            target,
            external,
            used_in: i.used_in,
        });
    }
    if ext == "java" {
        // Same-package classes need no import: link the classes a call
        // receiver, constructor, or heritage clause names.
        let mut named = BTreeMap::<&str, u32>::new();
        let uses = p
            .calls
            .iter()
            .flat_map(|call| {
                let head = call
                    .callee
                    .split(['.', '(', '<'])
                    .next()
                    .unwrap_or_default();
                [Some(head), call.receiver_type.as_deref()]
                    .into_iter()
                    .flatten()
                    .map(|name| (name, call.line))
            })
            .chain(
                p.edges
                    .iter()
                    .filter(|edge| matches!(edge.relation.as_str(), "extends" | "implements"))
                    .map(|edge| {
                        (
                            edge.to.split(['.', '<']).next().unwrap_or_default(),
                            edge.line,
                        )
                    }),
            );
        for (name, line) in uses {
            let first = named.entry(name).or_insert(line);
            *first = (*first).min(line);
        }
        for (name, line) in named {
            if let Some(target) = packages.same_package_class(&file, name) {
                let target = target.to_owned();
                add_edge(
                    b,
                    graph_builder.as_deref_mut(),
                    &file,
                    &mut node,
                    &target,
                    // Not an import: no import statement exists, so the edge
                    // carries no `importLine` and names its own kind.
                    "java-same-package",
                    line,
                )?;
            }
        }
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
                ctx,
            );
            record_resolution(
                b,
                &file,
                x.line,
                &spec,
                &ext,
                &target,
                link == "unsupported",
                target.is_none() && internal_bare(&spec),
                security,
            );
            if let Some(t) = &target {
                facts.reexport_lines.push((t.clone(), x.line));
            }
            if x.name == "*" {
                if let Some(t) = target {
                    let kind = if ext == "rs" {
                        "rust-use"
                    } else if x.export_kind == "type" {
                        "type-star-reexport"
                    } else {
                        "star-reexport"
                    };
                    add_edge(
                        b,
                        graph_builder.as_deref_mut(),
                        &file,
                        &mut node,
                        &t,
                        kind,
                        x.line,
                    )?;
                    b.star_reexporters.entry(t).or_default().push(file.clone());
                }
            } else {
                if let Some(t) = &target {
                    add_edge(
                        b,
                        graph_builder.as_deref_mut(),
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
            let target = resolve(&c.callee, &file, &ext, None, "*", known, cargo_crates, ctx);
            let internal = target.is_none() && internal_bare(&c.callee);
            record_resolution(
                b, &file, c.line, &c.callee, &ext, &target, false, internal, security,
            );
            if let Some(t) = target {
                add_edge(
                    b,
                    graph_builder.as_deref_mut(),
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
            let target = (ext == "rs" && !rust_cargo_unavailable)
                .then(|| rust_call_target(&c.callee, &file, &facts.imports, known, cargo_crates))
                .flatten();
            facts.calls.push(Call {
                caller_id: c.caller_id,
                callee: c.callee,
                line: c.line,
                kind: c.kind,
                receiver_type: c.receiver_type,
                target,
            });
        }
    }
    for c in p.common_js {
        match c.specifier {
            Some(spec) => {
                let target = resolve(&spec, &file, &ext, None, "*", known, cargo_crates, ctx);
                let internal = target.is_none() && internal_bare(&spec);
                record_resolution(
                    b, &file, c.line, &spec, &ext, &target, false, internal, security,
                );
                if let Some(t) = target {
                    add_edge(
                        b,
                        graph_builder.as_deref_mut(),
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
    graph_builder: Option<&mut octocode_engine::graph::CodeGraphBuilder>,
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
    if let Some(builder) = graph_builder {
        builder
            .add_file_relation(source, target, kind, line)
            .map_err(|error| AstGraphError::new("ast.graph.modelFailed", error))?;
    }
    Ok(())
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

/// Tallies one import resolution; returns whether an unlinked specifier names
/// an external package (as opposed to an unresolved or unsupported one).
/// `internal_bare` marks a bare specifier that names project code (alias or
/// workspace package), so an unlinked one is an internal gap.
#[allow(clippy::too_many_arguments)]
fn record_resolution(
    b: &mut BuiltGraph,
    file: &str,
    line: u32,
    spec: &str,
    ext: &str,
    target: &Option<String>,
    unsupported: bool,
    internal_bare: bool,
    security: &ContentSecurity,
) -> bool {
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
        && (spec.starts_with('.') || spec.starts_with('/') || internal_bare)
        && is_non_code_specifier(spec)
    {
        b.imports[4] += 1
    } else if spec.starts_with('.')
        || spec.starts_with('/')
        || (ext == "rs" && rust_internal_specifier(spec))
        || internal_bare
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
        b.imports[1] += 1;
        return true;
    }
    false
}

#[allow(clippy::too_many_arguments)]
fn resolve(
    spec: &str,
    importer: &str,
    ext: &str,
    hint: Option<&str>,
    imported: &str,
    known: &BTreeSet<String>,
    cargo_crates: &CargoCrates,
    ctx: &ResolveContext,
) -> Option<String> {
    if matches!(ext, "py" | "pyi") {
        return resolve_python(
            spec,
            importer,
            hint,
            imported,
            known,
            &ctx.python_roots_for(importer),
        )
        .or_else(|| {
            // An absolute import naming the scan root's own package.
            let local = ctx
                .python_package_local(spec)
                .filter(|_| hint == Some("python-absolute"))?;
            if local.is_empty() {
                return ["__init__.py", "__init__.pyi"]
                    .into_iter()
                    .flat_map(|init| {
                        [format!("{imported}.py"), format!("{imported}/{init}")]
                            .into_iter()
                            .chain([init.to_owned()])
                    })
                    .find(|file| known.contains(file));
            }
            resolve_python(local, importer, hint, imported, known, &["."])
        });
    }
    if is_c_family_extension(ext) {
        let quoted = match hint {
            Some("c-relative") => true,
            Some("c-system") => false,
            _ => return None,
        };
        if spec.starts_with('/') {
            return None;
        }
        if quoted
            && let Some(p) = join_within_root(dirname(importer), spec)
            && known.contains(&p)
        {
            return Some(p);
        }
        // Then the project's include search path (bounded list), then a
        // unique path-suffix match for build-system include roots.
        return ctx
            .include_dirs(quoted)
            .iter()
            .find_map(|dir| join_within_root(dir, spec).filter(|p| known.contains(p)))
            .or_else(|| ctx.header_by_suffix(spec, importer));
    }
    if ext == "rs" {
        return resolve_rust(spec, importer, known, cargo_crates);
    }
    if !is_javascript_extension(ext) {
        return None;
    }
    if spec.starts_with('/') {
        return None;
    }
    if !spec.starts_with('.') {
        return ctx.resolve_bare_js(spec, importer, known);
    }
    probe_js(&join_within_root(dirname(importer), spec)?, known)
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
/// Rust paths the linker can settle from the importing file alone:
/// - `#[path = "x.rs"] mod name;` resolves relative to the declaring file.
/// - `super::`/`self::` inside an inline module (`mod tests { use super::*; }`)
///   that does not climb past it refers to the same file.
fn rust_local_target(
    import: &octocode_engine::graph::GraphFactImport,
    file: &str,
    modules: &[octocode_engine::graph::GraphFactRustModule],
    known: &BTreeSet<String>,
) -> Option<String> {
    if import.import_kind == "module" {
        let name = import.imported_name.as_deref()?;
        let path = modules
            .iter()
            .find(|m| m.name == name && m.line == import.line && !m.inline)?
            .path
            .as_deref()?;
        let target = join_within_root(dirname(file), path)?;
        return known.contains(&target).then_some(target);
    }
    let depth = import.module_scope.as_ref().map_or(0, Vec::len);
    if depth == 0 {
        return None;
    }
    let segments = import.specifier.split("::").collect::<Vec<_>>();
    let climbs = match segments.first() {
        Some(&"self") => 0,
        Some(&"super") => segments.iter().take_while(|s| **s == "super").count(),
        _ => return None,
    };
    (climbs <= depth).then(|| file.to_owned())
}

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

/// The file named by the module prefix of a Rust qualified callee. A first
/// segment bound by a `use`/`mod` in the calling file resolves through that
/// binding (`use crate::helper as h; h::run()` → `helper.rs`); any other
/// prefix resolves as a path from the calling file (`crate::`, `super::`,
/// a workspace crate, or a child module).
fn rust_call_target(
    callee: &str,
    file: &str,
    imports: &[Import],
    known: &BTreeSet<String>,
    cargo_crates: &CargoCrates,
) -> Option<String> {
    let (prefix, _) = callee.rsplit_once("::")?;
    let first = prefix.split("::").next()?;
    if prefix.is_empty() || first == "Self" {
        return None;
    }
    let bound = imports.iter().find(|import| {
        import.target.is_some()
            && import
                .local_name
                .as_deref()
                .unwrap_or(&import.imported_name)
                == first
    });
    match bound {
        Some(import) => match &prefix[first.len()..] {
            "" => import.target.clone(),
            rest => resolve_rust(
                &format!("{}{rest}", import.specifier),
                file,
                known,
                cargo_crates,
            ),
        },
        None => resolve_rust(prefix, file, known, cargo_crates),
    }
}

fn resolve_rust(
    spec: &str,
    importer: &str,
    known: &BTreeSet<String>,
    cargo_crates: &CargoCrates,
) -> Option<String> {
    let trimmed = spec.trim_end_matches(';');
    let segments = trimmed.split("::").collect::<Vec<_>>();
    let first = *segments.first().unwrap_or(&"");
    if !matches!(first, "crate" | "self" | "super" | "")
        && let Some(src) = cargo_crates.resolve(importer, first)
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
            .and_then(|name| cargo_crates.resolve(importer, name))
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
/// Python imports: relative ones climb from the importer's package;
/// absolute ones try each import root (`roots`, nearest first). With
/// `from pkg import name`, a submodule `pkg/name.py` wins over `pkg` itself.
fn resolve_python(
    spec: &str,
    importer: &str,
    hint: Option<&str>,
    imported: &str,
    known: &BTreeSet<String>,
    roots: &[&str],
) -> Option<String> {
    if !matches!(hint, Some("python-relative") | Some("python-absolute")) {
        return None;
    }
    let dots = spec.chars().take_while(|c| *c == '.').count();
    let module = spec[dots..].replace('.', "/");
    let bases = if dots > 0 {
        let mut base = dirname(importer);
        for _ in 1..dots {
            if base == "." {
                return None;
            }
            base = dirname(base);
        }
        vec![base]
    } else {
        roots.to_vec()
    };
    let submodule = (!imported.is_empty()
        && imported != "*"
        && imported.chars().all(|c| c == '_' || c.is_alphanumeric()))
    .then_some(imported);
    let module_files = |stem: &str| {
        [
            format!("{stem}.py"),
            format!("{stem}.pyi"),
            join(stem, "__init__.py"),
            join(stem, "__init__.pyi"),
        ]
        .into_iter()
        .find(|x| known.contains(x))
    };
    bases.into_iter().find_map(|base| {
        let stem = join_within_root(base, &module)?;
        submodule
            .and_then(|name| module_files(&join(&stem, name)))
            .or_else(|| module_files(&stem))
    })
}
pub(super) fn dirname(p: &str) -> &str {
    p.rsplit_once('/').map(|x| x.0).unwrap_or(".")
}
pub(super) fn join(a: &str, b: &str) -> String {
    normalize(&format!("{a}/{b}"))
}
pub(super) fn join_within_root(a: &str, b: &str) -> Option<String> {
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
    gitignore: Option<&GitignoreFilter>,
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
            if gitignore.is_some_and(|filter| filter.is_ignored(path)) {
                return Ok(false);
            }
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
                "tool": ToolId::AstTopology.as_str(),
                "confidence": "medium",
                "query": continuation(format!("{root_display}/{dir}"), None),
            }),
        );
    }
    next.insert(
        "expandScan".into(),
        serde_json::json!({
            "tool": ToolId::AstTopology.as_str(),
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

#[cfg(test)]
mod tests {
    /// Real-repo sanity probe: `OCTOCODE_GRAPH_SANITY_ROOTS=a:b cargo test ...
    /// graph_sanity_tallies -- --ignored --nocapture` prints import tallies.
    #[test]
    #[ignore = "manual real-repo probe"]
    fn graph_sanity_tallies() {
        struct Active;
        impl crate::tools::cancel::CancellationCheck for Active {
            fn check(&self) -> Result<(), String> {
                Ok(())
            }
        }
        let Ok(roots) = std::env::var("OCTOCODE_GRAPH_SANITY_ROOTS") else {
            return;
        };
        for root in roots.split(':') {
            let paths =
                crate::policy::path::PathPolicy::new(crate::policy::path::PathPolicyConfig {
                    workspace_root: Some(std::path::PathBuf::from(root)),
                    ..Default::default()
                })
                .unwrap();
            let security = crate::security::ContentSecurity::new();
            let query: super::AstTopologyQuery = serde_json::from_value(serde_json::json!({
                "goal": "test", "reasoning": "sanity", "analysis": "cycles", "path": root, "maxFiles": 20000
            }))
            .unwrap();
            let started = std::time::Instant::now();
            let built = super::build_graph(&query, &paths, &security, &Active).unwrap();
            let unresolved = built
                .diagnostics
                .iter()
                .filter(|d| d.code == "unresolved-internal")
                .take(8)
                .map(|d| format!("{}:{:?} {}", d.file, d.line, d.message))
                .collect::<Vec<_>>();
            println!(
                "SANITY {root} files={} imports[resolved,external,unresolvedInternal,unsupported,nonCode]={:?} ms={}\n  sample unresolved: {unresolved:#?}",
                built.facts.len(),
                built.imports,
                started.elapsed().as_millis()
            );
        }
    }

    fn cargo_fixture() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"a\", \"b\", \"z_one\", \"z_two\"]\nresolver = \"2\"\n",
        )
        .unwrap();
        for (name, dependency) in [
            ("a", "shared = { package = \"z_one\", path = \"../z_one\" }"),
            ("b", "shared = { package = \"z_two\", path = \"../z_two\" }"),
            ("z_one", ""),
            ("z_two", ""),
        ] {
            let directory = root.path().join(name);
            std::fs::create_dir_all(directory.join("src")).unwrap();
            std::fs::write(directory.join("src/lib.rs"), "pub fn item() {}\n").unwrap();
            std::fs::write(directory.join("Cargo.toml"), format!(
                "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[dependencies]\n{dependency}\n"
            )).unwrap();
        }
        root
    }

    #[test]
    fn cargo_aliases_are_resolved_in_the_importing_package() {
        let fixture = cargo_fixture();
        let crates = super::load_cargo_crates(&fixture.path().canonicalize().unwrap()).unwrap();
        let known = [
            "a/src/lib.rs",
            "b/src/lib.rs",
            "z_one/src/lib.rs",
            "z_two/src/lib.rs",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        for (importer, expected) in [
            ("a/src/lib.rs", "z_one/src/lib.rs"),
            ("b/src/lib.rs", "z_two/src/lib.rs"),
        ] {
            assert_eq!(
                super::resolve_rust("shared::item", importer, &known, &crates),
                Some(expected.to_owned())
            );
        }
        assert_eq!(
            super::resolve_rust("shared::item", "z_one/src/lib.rs", &known, &crates),
            None
        );
        assert_eq!(
            super::resolve_rust("z_two::item", "a/src/lib.rs", &known, &crates),
            None
        );
    }

    #[test]
    fn cargo_member_manifest_changes_refresh_dependency_aliases() {
        let fixture = cargo_fixture();
        let manifest = fixture.path().join("z_two/Cargo.toml");
        let text = std::fs::read_to_string(&manifest).unwrap();
        std::fs::write(
            &manifest,
            format!("{text}shared = {{ package = \"z_one\", path = \"../z_one\" }}\n"),
        )
        .unwrap();
        // Warm up after any first-run lockfile creation, so only the member
        // manifest changes between the two observations below.
        super::load_cargo_crates(&fixture.path().canonicalize().unwrap()).unwrap();
        let before = super::load_cargo_crates(&fixture.path().canonicalize().unwrap()).unwrap();
        let text = std::fs::read_to_string(&manifest).unwrap();
        std::fs::write(&manifest, text.replace("shared =", "renamed =")).unwrap();
        let after = super::load_cargo_crates(&fixture.path().canonicalize().unwrap()).unwrap();
        let known = ["z_two/src/lib.rs", "z_one/src/lib.rs"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        assert_eq!(
            super::resolve_rust("shared::item", "z_two/src/lib.rs", &known, &before),
            Some("z_one/src/lib.rs".to_owned())
        );
        assert_eq!(
            super::resolve_rust("renamed::item", "z_two/src/lib.rs", &known, &before),
            None
        );
        assert_eq!(
            super::resolve_rust("renamed::item", "z_two/src/lib.rs", &known, &after),
            Some("z_one/src/lib.rs".to_owned())
        );
    }

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
                &["."],
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
                &["."],
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
        let packages =
            super::ResolveContext::default().with_package("example", ".", "src/index.ts");
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
        let crates = super::CargoCrates::default();
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
        let crates = super::CargoCrates::default();
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

    fn known_set(files: &[&str]) -> std::collections::BTreeSet<String> {
        files.iter().map(|file| (*file).to_owned()).collect()
    }

    fn write_file(root: &std::path::Path, file: &str, text: &str) {
        let path = root.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    struct Active;
    impl crate::tools::cancel::CancellationCheck for Active {
        fn check(&self) -> Result<(), String> {
            Ok(())
        }
    }

    fn fixture_graph(root: &std::path::Path, extras: &super::BuildExtras) -> super::BuiltGraph {
        let paths = crate::policy::path::PathPolicy::new(crate::policy::path::PathPolicyConfig {
            workspace_root: Some(root.to_path_buf()),
            ..Default::default()
        })
        .unwrap();
        let query: super::AstTopologyQuery = serde_json::from_value(serde_json::json!({
            "goal": "test", "reasoning": "test", "analysis": "cycles", "path": root
        }))
        .unwrap();
        super::build_graph_with(
            &query,
            &paths,
            &crate::security::ContentSecurity::new(),
            &Active,
            extras,
        )
        .unwrap()
    }

    #[test]
    fn alias_looking_bare_specifiers_are_internal_gaps_not_external() {
        let mut b = super::BuiltGraph::default();
        let security = crate::security::ContentSecurity::new();
        let record = |b: &mut super::BuiltGraph, spec: &str, internal: bool| {
            super::record_resolution(
                b, "src/a.ts", 1, spec, "ts", &None, false, internal, &security,
            )
        };
        assert!(record(&mut b, "react", false), "third-party stays external");
        assert!(!record(&mut b, "@/missing", true));
        assert!(!record(&mut b, "@acme/core/package.json", true));
        assert_eq!(b.imports, [0, 1, 1, 0, 1]);
        assert!(
            b.diagnostics
                .iter()
                .any(|d| d.code == "unresolved-internal" && d.message.contains("@/missing"))
        );
    }

    #[test]
    fn tsconfig_aliases_and_workspace_packages_link_in_a_built_graph() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        write_file(
            &root,
            "tsconfig.json",
            "{ /* jsonc */ \"compilerOptions\": { \"paths\": { \"@app/*\": [\"./app/src/*\"], }, }, }",
        );
        write_file(
            &root,
            "lib/package.json",
            r#"{ "name": "@acme/lib", "main": "./dist/index.js", "exports": { ".": { "import": "./dist/index.js" } } }"#,
        );
        write_file(&root, "lib/src/index.ts", "export const lib = 1;\n");
        write_file(&root, "app/src/util.ts", "export const util = 1;\n");
        write_file(
            &root,
            "app/src/main.ts",
            "import { util } from '@app/util';\nimport { lib } from '@acme/lib';\nimport { gone } from '@app/gone';\nimport React from 'react';\nexport const all = [util, lib, gone, React];\n",
        );
        let built = fixture_graph(&root, &super::BuildExtras::default());
        let imports = &built.facts["app/src/main.ts"].imports;
        let target = |spec: &str| {
            imports
                .iter()
                .find(|import| import.specifier == spec)
                .map(|import| (import.target.clone(), import.external))
        };
        assert_eq!(
            target("@app/util"),
            Some((Some("app/src/util.ts".to_owned()), false))
        );
        assert_eq!(
            target("@acme/lib"),
            Some((Some("lib/src/index.ts".to_owned()), false))
        );
        assert_eq!(target("@app/gone"), Some((None, false)));
        assert_eq!(target("react"), Some((None, true)));
        assert_eq!(built.imports[..3], [2, 1, 1]);
    }

    #[test]
    fn python_absolute_imports_use_src_layout_roots_and_submodules() {
        let known = known_set(&[
            "proj/src/pkg/__init__.py",
            "proj/src/pkg/sub.py",
            "proj/src/pkg/nested/__init__.py",
            "proj/app.py",
        ]);
        let roots = ["proj/src", "proj", "."];
        let resolve = |spec: &str, imported: &str| {
            super::resolve_python(
                spec,
                "proj/app.py",
                Some(if spec.starts_with('.') {
                    "python-relative"
                } else {
                    "python-absolute"
                }),
                imported,
                &known,
                &roots,
            )
        };
        assert_eq!(
            resolve("pkg", "*").as_deref(),
            Some("proj/src/pkg/__init__.py")
        );
        assert_eq!(
            resolve("pkg", "sub").as_deref(),
            Some("proj/src/pkg/sub.py")
        );
        assert_eq!(
            resolve("pkg", "nested").as_deref(),
            Some("proj/src/pkg/nested/__init__.py")
        );
        // A non-module name falls back to the package itself.
        assert_eq!(
            resolve("pkg", "function_name").as_deref(),
            Some("proj/src/pkg/__init__.py")
        );
        assert_eq!(resolve("missing", "*"), None);
    }

    #[test]
    fn c_includes_search_project_include_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        write_file(&root, "include/proj/api.h", "int api(void);\n");
        write_file(&root, "third/include/dep.h", "int dep(void);\n");
        write_file(
            &root,
            "compile_commands.json",
            &serde_json::json!([{"directory": root, "command": "cc -Ithird/include -c src/main.c", "file": "src/main.c"}]).to_string(),
        );
        write_file(
            &root,
            "src/main.c",
            "#include \"proj/api.h\"\n#include <dep.h>\n#include <stdio.h>\nint main(void) { return api() + dep(); }\n",
        );
        let built = fixture_graph(&root, &super::BuildExtras::default());
        let imports = &built.facts["src/main.c"].imports;
        let target = |spec: &str| {
            imports
                .iter()
                .find(|import| import.specifier == spec)
                .and_then(|import| import.target.clone())
        };
        assert_eq!(target("proj/api.h").as_deref(), Some("include/proj/api.h"));
        assert_eq!(target("dep.h").as_deref(), Some("third/include/dep.h"));
        assert_eq!(target("stdio.h"), None);
    }

    #[test]
    fn gitignore_is_respected_only_when_requested() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join(".git/info")).unwrap();
        write_file(&root, ".git/info/exclude", "local-only.ts\n");
        write_file(&root, ".gitignore", "generated/\n*.gen.ts\n");
        write_file(&root, "pkg/.gitignore", "scratch/\n!keep.gen.ts\n");
        for file in [
            "src/main.ts",
            "generated/api.ts",
            "src/model.gen.ts",
            "pkg/keep.gen.ts",
            "pkg/scratch/tmp.ts",
            "local-only.ts",
        ] {
            write_file(&root, file, "export const x = 1;\n");
        }
        let all = fixture_graph(&root, &super::BuildExtras::default());
        assert_eq!(all.facts.len(), 6, "{:?}", all.facts.keys());
        let filtered = fixture_graph(
            &root,
            &super::BuildExtras {
                respect_gitignore: true,
                extra_excludes: vec!["pkg".into()],
            },
        );
        assert_eq!(filtered.facts.keys().collect::<Vec<_>>(), ["src/main.ts"],);
        let nested = fixture_graph(
            &root,
            &super::BuildExtras {
                respect_gitignore: true,
                extra_excludes: Vec::new(),
            },
        );
        assert_eq!(
            nested.facts.keys().collect::<Vec<_>>(),
            ["pkg/keep.gen.ts", "src/main.ts"],
            "nested negation re-includes; nested dir rule prunes"
        );
    }

    #[test]
    fn link_file_keeps_heritage_and_call_kinds() {
        use octocode_engine::graph::{GraphFactCall, GraphFactEdge, GraphFactsDocument};
        let document = GraphFactsDocument {
            schema_version: 1,
            language: "typescript".into(),
            edges: vec![
                GraphFactEdge {
                    from: "decl:A".into(),
                    to: "ns.Base".into(),
                    relation: "extends".into(),
                    line: 3,
                    ..Default::default()
                },
                GraphFactEdge {
                    from: "decl:A".into(),
                    to: "Shape".into(),
                    relation: "implements".into(),
                    line: 3,
                    ..Default::default()
                },
                GraphFactEdge {
                    from: "decl:A".into(),
                    to: "x".into(),
                    relation: "contains".into(),
                    line: 4,
                    ..Default::default()
                },
            ],
            calls: vec![
                GraphFactCall {
                    callee: "Widget".into(),
                    kind: "renders".into(),
                    line: 5,
                    ..Default::default()
                },
                GraphFactCall {
                    callee: "Injectable".into(),
                    kind: "decorates".into(),
                    caller_id: Some("decl:A".into()),
                    line: 2,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let mut built = super::BuiltGraph::default();
        super::link_file(
            &mut built,
            &known_set(&["a.ts"]),
            "a.ts".into(),
            document,
            Default::default(),
            &crate::security::ContentSecurity::new(),
            false,
            &super::CargoCrates::default(),
            &super::ResolveContext::default(),
            &Default::default(),
            None,
        )
        .unwrap();
        let facts = &built.facts["a.ts"];
        assert_eq!(
            facts
                .heritage
                .iter()
                .map(|h| (
                    h.decl_id.as_str(),
                    h.relation.as_str(),
                    h.target.as_str(),
                    h.line
                ))
                .collect::<Vec<_>>(),
            [
                ("decl:A", "extends", "ns.Base", 3),
                ("decl:A", "implements", "Shape", 3)
            ]
        );
        assert_eq!(
            facts
                .calls
                .iter()
                .map(|c| (c.callee.as_str(), c.kind.as_str(), c.caller_id.is_some()))
                .collect::<Vec<_>>(),
            [
                ("Widget", "renders", false),
                ("Injectable", "decorates", true)
            ]
        );
    }

    #[test]
    fn cargo_metadata_stdout_is_drained_while_the_child_runs() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let crates = super::load_cargo_crates(root).expect("cargo metadata");
        assert_eq!(
            crates
                .resolve("src/lib.rs", "octocode_native")
                .map(String::as_str),
            Some("src/lib.rs")
        );
    }
}
