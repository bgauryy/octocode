//! Rust module-path resolution: crate roots, `mod` children, inline paths
//! and call targets.
use super::*;

/// The directory holding the importer's child modules: `foo/` for
/// `foo/mod.rs` (and crate roots), `foo/bar/` for `foo/bar.rs`.
pub(crate) fn rust_module_child_dir(importer: &str) -> String {
    let name = importer.rsplit_once('/').map_or(importer, |x| x.1);
    if matches!(name, "mod.rs" | "lib.rs" | "main.rs") {
        dirname(importer).to_owned()
    } else {
        importer.strip_suffix(".rs").unwrap_or(importer).to_owned()
    }
}

/// The importer's crate-root directory: the nearest ancestor holding a
/// `lib.rs`/`main.rs` in the scanned set; `None` when the scan holds none.
pub(super) fn rust_crate_root(importer: &str, known: &BTreeSet<String>) -> Option<String> {
    let mut dir = dirname(importer);
    loop {
        if known.contains(&join(dir, "lib.rs")) || known.contains(&join(dir, "main.rs")) {
            return Some(dir.to_owned());
        }
        let parent = dirname(dir);
        if parent == dir || dir == "." {
            return None;
        }
        dir = parent;
    }
}

/// Without a scanned crate root: the importer's nearest ancestor named
/// `src` (a multi-crate workspace has many `src/` roots — anchoring to the
/// importer's own is what makes `crate::` resolvable outside a single-crate
/// scan), else the scan-relative `src`.
pub(super) fn rust_src_dir(importer: &str) -> String {
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
pub(super) fn rust_local_target(
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

/// A Rust path written inside `depth` inline modules: a `self::` path or
/// one whose `super`s stay within those modules names this file (`None`);
/// otherwise the path with the inline climbs removed, resolved from the
/// file's own module.
pub(super) fn rust_inline_path(path: &str, depth: u32) -> Option<&str> {
    if depth == 0 {
        return Some(path);
    }
    if path.starts_with("self::") {
        return None;
    }
    let climbs = path.split("::").take_while(|s| *s == "super").count();
    if climbs == 0 {
        return Some(path);
    }
    if climbs <= depth as usize {
        return None;
    }
    let mut rest = path;
    for _ in 0..depth {
        rest = rest.strip_prefix("super::")?;
    }
    Some(rest)
}

/// A Rust `super::` path written inside inline modules climbs those first:
/// at inline depth `d`, the first `d` `super`s stay in the file, so only the
/// rest climb from the file's own module.
pub(super) fn rust_scoped_specifier<'a>(
    import: &'a octocode_engine::graph::GraphFactImport,
    lang: Linker,
) -> std::borrow::Cow<'a, str> {
    let depth = import.module_scope.as_ref().map_or(0, Vec::len);
    if lang != Linker::Rust || depth == 0 {
        return std::borrow::Cow::Borrowed(&import.specifier);
    }
    let mut rest = import.specifier.as_str();
    for _ in 0..depth {
        match rest.strip_prefix("super::") {
            Some(tail) if tail == "super" || tail.starts_with("super::") => rest = tail,
            _ => return std::borrow::Cow::Borrowed(&import.specifier),
        }
    }
    std::borrow::Cow::Owned(rest.to_owned())
}

pub(super) fn resolve_rust_module_prefix(
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
pub(super) fn rust_call_target(
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

pub(super) fn resolve_rust(
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
        "crate" => match rust_crate_root(importer, known) {
            Some(base) => resolve_rust_module_prefix(&base, &segments[1..], known),
            // The crate's root is above the scan: a path into the scan's
            // own modules names a file from the scan root.
            None => match cargo_crates.inside_scan(&segments[1..]) {
                Some(rest) => resolve_rust_module_prefix(".", rest, known),
                None => resolve_rust_module_prefix(&rust_src_dir(importer), &segments[1..], known),
            },
        },
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
