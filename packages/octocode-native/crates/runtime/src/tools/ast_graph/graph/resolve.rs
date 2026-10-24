//! Recording edges and resolving import specifiers (generic and Python).
use super::*;
use octocode_engine::graph::FileGraphNode;

/// Records one file edge. A repeat of `(target, kind)` keeps the file's
/// first production evidence line, else its first test-only one
/// (`test_scoped`), so an edge is test code only when all of its sites are.
pub(super) fn add_edge(
    b: &mut BuiltGraph,
    node: &mut FileGraphNode,
    target: &str,
    kind: &str,
    line: u32,
    test_scoped: &dyn Fn(u32) -> bool,
) {
    // The cap counts distinct (target, kind) edges; a repeat of one only
    // moves its evidence line.
    let rank = |line: u32| (test_scoped(line), line);
    let repeat = node
        .edges
        .get_mut(target)
        .and_then(|kinds| kinds.get_mut(kind))
        .map(|first| {
            if rank(line) < rank(*first) {
                *first = line;
            }
        })
        .is_some();
    if !repeat {
        if b.edge_count >= edge_cap() {
            if !b.edges_capped {
                b.edges_capped = true;
                b.diagnostics.push(Diagnostic::new(
                    ".",
                    None,
                    DiagnosticCode::GraphEdgeCap,
                    format!(
                        "Edge collection stopped at the {}-edge cap; graph answers are partial. Narrow the scan root or exclude.",
                        edge_cap()
                    ),
                ));
            }
            return;
        }
        b.edge_count += 1;
        node.edges
            .entry(target.into())
            .or_default()
            .insert(kind.into(), line);
    }
}

/// A Rust `use` path that unambiguously targets the current crate, so failing to
/// resolve it means the intra-crate edge graph is incomplete — never a benign
/// external dependency. `crate::`/`self::`/`super::` and leading `::` qualify.
pub(super) fn rust_internal_specifier(spec: &str) -> bool {
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
pub(super) fn is_non_code_specifier(spec: &str) -> bool {
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
pub(super) fn record_resolution(
    b: &mut BuiltGraph,
    file: &str,
    line: u32,
    spec: &str,
    lang: Linker,
    target: &Option<String>,
    unsupported: bool,
    internal_bare: bool,
    security: &ContentSecurity,
) -> bool {
    if target.is_some() {
        b.count_import(ImportOutcome::Resolved, file, line, spec);
    } else if unsupported {
        b.count_import(ImportOutcome::Unsupported, file, line, spec);
        b.diagnostics.push(Diagnostic::new(
            file,
            Some(line),
            DiagnosticCode::UnsupportedLinking,
            sanitize(
                security,
                &format!("Cannot link import {spec:?} (unsupported)."),
            ),
        ));
    } else if lang != Linker::Python
        && (spec.starts_with('.') || spec.starts_with('/') || internal_bare)
        && is_non_code_specifier(spec)
    {
        b.count_import(ImportOutcome::NonCode, file, line, spec);
    } else if spec.starts_with('.')
        || spec.starts_with('/')
        || (lang == Linker::Rust && rust_internal_specifier(spec))
        || internal_bare
    {
        b.count_import(ImportOutcome::UnresolvedInternal, file, line, spec);
        b.diagnostics.push(Diagnostic::new(
            file,
            Some(line),
            DiagnosticCode::UnresolvedInternal,
            sanitize(
                security,
                &format!("Cannot link import {spec:?} (unresolvedInternal)."),
            ),
        ));
    } else {
        b.count_import(ImportOutcome::External, file, line, spec);
        return true;
    }
    false
}

pub(super) fn resolve(
    spec: &str,
    importer: &str,
    lang: Linker,
    hint: Option<&str>,
    imported: &str,
    known: &BTreeSet<String>,
    cargo_crates: &CargoCrates,
    ctx: &ResolveContext,
) -> Option<String> {
    if lang == Linker::Python {
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
    if lang == Linker::CFamily {
        let quoted = match hint {
            Some("c-relative") => true,
            Some("c-system") => false,
            _ => return None,
        };
        if spec.starts_with('/') {
            return None;
        }
        if quoted
            && let Some(p) = ctx.join_relative(dirname(importer), spec)
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
    if lang == Linker::Rust {
        return resolve_rust(spec, importer, known, cargo_crates);
    }
    if lang != Linker::JsTs {
        return None;
    }
    if spec.starts_with('/') {
        return None;
    }
    if !spec.starts_with('.') {
        return ctx.resolve_bare_js(spec, importer, known);
    }
    probe_js(&ctx.join_relative(dirname(importer), spec)?, known)
}

/// Python imports: relative ones climb from the importer's package;
/// absolute ones try each import root (`roots`, nearest first). With
/// `from pkg import name`, a submodule `pkg/name.py` wins over `pkg` itself.
pub(super) fn resolve_python(
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
