//! Rust module-path resolution: crate roots, `mod` children, inline paths
//! and call targets.
use super::*;
mod modules;
pub(super) use modules::{ModuleResolution, RustModules};

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

/// Whether a `super::` chain climbs above the snapshot root: each `super`
/// leaves one directory of the importer's module directory, and the root's
/// own module (`.`) has no parent inside the scan.
pub(super) fn rust_super_escapes_root(importer: &str, spec: &str) -> bool {
    let climbs = spec.split("::").take_while(|s| *s == "super").count();
    let depth = rust_module_child_dir(importer)
        .split('/')
        .filter(|s| !s.is_empty() && *s != ".")
        .count();
    climbs > depth
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
/// The module item a qualified Rust path names: its first type-cased segment
/// after the head (`Kind` of `m::Kind::A`, `T` of `m::T::new`), else its last
/// segment (`f` of `crate::m::f`). A path of one segment names no module item.
pub(super) fn rust_path_item(path: &str) -> Option<&str> {
    let mut segments = path.split("::").filter(|segment| !segment.is_empty());
    segments.next()?;
    let rest = segments.collect::<Vec<_>>();
    let item = rest
        .iter()
        .find(|segment| segment.starts_with(|c: char| c.is_uppercase()))
        .or(rest.last())?;
    (!item.contains(['<', '{', '*', '(']) && *item != "Self").then_some(*item)
}

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
    modules: &RustModules,
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
            rest => modules.resolve(
                &format!("{}{rest}", import.specifier),
                file,
                known,
                cargo_crates,
            ),
        },
        None => modules.resolve(prefix, file, known, cargo_crates),
    }
}

/// A Rust path without its generic arguments: `a::f::<T>` is `a::f` and
/// `Vec::<u8>::new` is `Vec::new`, so the name and the module prefix are the
/// path's own segments.
pub(super) fn rust_without_generic_arguments(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    let mut rest = path;
    while let Some((head, arguments)) = rest.split_once("::<") {
        out.push_str(head);
        // An unclosed argument list drops the rest of the path.
        let mut depth = 1usize;
        let mut after = "";
        for (offset, c) in arguments.char_indices() {
            match c {
                '<' => depth += 1,
                '>' => {
                    depth -= 1;
                    if depth == 0 {
                        after = arguments.get(offset + 1..).unwrap_or_default();
                        break;
                    }
                }
                _ => {}
            }
        }
        rest = after;
    }
    out.push_str(rest);
    out
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
            // module directory; past the snapshot root the target is outside
            // the scan, never a same-named root file.
            if rust_super_escapes_root(importer, trimmed) {
                return None;
            }
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

/// Whether a Rust glob import (`specifier` ends in `::*`) imports an
/// ancestor module, whose private `use` bindings it sees: `super::*`,
/// `super::super::*`, `crate::*`.
pub(super) fn ancestor_glob(specifier: &str) -> bool {
    specifier.strip_suffix("::*").is_some_and(|module| {
        module == "crate" || module.split("::").all(|segment| segment == "super")
    })
}

/// What a glob walk found for a name: the module that binds it and the
/// `use` that does. A name the module declares, or a name a glob-imported
/// child declares, has no `use` of its own; `declared`
/// marks the import made up for it, which names the declaring file.
struct GlobBinding<'b> {
    module: &'b str,
    import: Import,
    declared: bool,
}

/// The `use` binding `head` that a glob of `module` brings in (`private`:
/// a `use super::*`, which passes on the ancestor's private names), and the
/// module that writes it: `module`'s own `use`, else, unless an ancestor
/// `module` declares `head` itself, the one a glob of it brings in.
/// A glob of an ancestor module passes on that module's private names too;
/// any other glob of a repo module (`use child::*`) passes on its
/// declarations and the names its `use`s bind (a path that compiles is the
/// proof the binding is visible, as the export resolver reads a `use`: the
/// facts call only a bare `pub` exported, while `pub(super)` and
/// `pub(crate)` reach the parent too), and
/// so the names its own globs bring in. Nearest first; `from` and every module visited in a
/// mode are never revisited.
fn glob_binding<'b>(
    b: &'b BuiltGraph,
    from: &'b str,
    module: &'b str,
    head: &str,
    private: bool,
) -> Option<GlobBinding<'b>> {
    let mut seen =
        std::collections::BTreeSet::from([(from, true), (from, false), (module, private)]);
    let mut pending = std::collections::VecDeque::from([(module, private)]);
    while let Some((current, private)) = pending.pop_front() {
        let Some(facts) = b.facts.get(current) else {
            continue;
        };
        let uses = || facts.imports.iter().filter(|import| !import.inline);
        if let Some(bound) =
            uses().find(|import| import.imported_name != "*" && bound_name(import) == head)
        {
            return Some(GlobBinding {
                module: current,
                import: bound.clone(),
                declared: false,
            });
        }
        let declares = facts
            .declarations
            .iter()
            .any(|decl| decl.parent.is_none() && decl.name == head);
        if declares {
            if private {
                continue;
            }
            return Some(GlobBinding {
                module: current,
                import: Import {
                    id: String::new(),
                    imported_name: head.to_owned(),
                    local_name: None,
                    specifier: String::new(),
                    line: 0,
                    target: Some(current.to_owned()),
                    external: false,
                    scope_id: None,
                    inline: false,
                },
                declared: true,
            });
        }
        for glob in uses().filter(|import| import.imported_name == "*") {
            let Some(target) = glob.target.as_deref() else {
                continue;
            };
            let passes_private = private && ancestor_glob(&glob.specifier);
            if seen.insert((target, passes_private)) {
                pending.push_back((target, passes_private));
            }
        }
    }
    None
}

fn bound_name(import: &Import) -> &str {
    import
        .local_name
        .as_deref()
        .unwrap_or(&import.imported_name)
}

/// `use super::*` binds every name the parent module binds, its private
/// `use`s included. A module path whose first segment its file binds no
/// other way (`query::view()` beside the parent's `use crate::query;`)
/// binds through the parent's `use`: that binding becomes an import of the
/// file at the glob's line, so calls resolve through it, and a path through
/// it depends on its module as a `use` does. Repeats until no file gains a
/// binding, so a parent's own inherited bindings reach its children. A
/// parent that binds the segment only through its own `use super::*` passes
/// on its ancestor's `use` ([`glob_binding`]). A file's own `use child::*`
/// binds such a segment the same way, through the names the child passes on.
pub(super) fn inherit_parent_globs(b: &mut BuiltGraph, inputs: &LinkInputs) {
    loop {
        let mut gained = Vec::<(String, Vec<Import>, Vec<(String, u32)>, Vec<RustPathUse>)>::new();
        for (file, facts) in &b.facts {
            if facts.rust_unbound_paths.is_empty() {
                continue;
            }
            let mut imports = Vec::<Import>::new();
            let mut edges = Vec::new();
            let mut uses = Vec::new();
            let globs = facts
                .imports
                .iter()
                .filter(|import| !import.inline && import.imported_name == "*");
            for glob in globs {
                let private = ancestor_glob(&glob.specifier);
                let Some(parent) = glob.target.as_deref().filter(|parent| *parent != file) else {
                    continue;
                };
                for (path, item, line) in &facts.rust_unbound_paths {
                    let head = path.split("::").next().unwrap_or_default();
                    let held = |import: &Import| bound_name(import) == head;
                    if facts.imports.iter().any(held) {
                        continue;
                    }
                    let Some(GlobBinding {
                        module: parent,
                        import: bound,
                        declared,
                    }) = glob_binding(b, file, parent, head, private)
                    else {
                        continue;
                    };
                    let rest = path.strip_prefix(head).unwrap_or_default();
                    let target = match &bound.target {
                        Some(target) if rest.is_empty() || declared => Some(target.clone()),
                        Some(_) if !declared => inputs.rust_modules.resolve(
                            &format!("{}{rest}", bound.specifier),
                            parent,
                            inputs.known,
                            inputs.cargo_crates,
                        ),
                        _ => None,
                    };
                    if let Some(target) = target.filter(|target| target != file) {
                        // The item the path names in that module: a
                        // re-export of it makes this file its user.
                        if let Some(item) = item.as_ref().filter(|_| !declared) {
                            uses.push(RustPathUse {
                                module: target.clone(),
                                item: item.clone(),
                                line: *line,
                                reexport_only: true,
                            });
                        }
                        edges.push((target, *line));
                    }
                    if !imports.iter().any(held) {
                        imports.push(Import {
                            id: format!("{}:{head}", glob.id),
                            line: glob.line,
                            scope_id: None,
                            ..bound
                        });
                    }
                }
            }
            if !imports.is_empty() {
                gained.push((file.clone(), imports, edges, uses));
            }
        }
        if gained.is_empty() {
            return;
        }
        for (file, imports, edges, uses) in gained {
            let Some(mut node) = b.nodes.remove(&file) else {
                continue;
            };
            for (target, line) in edges {
                let test = b
                    .facts
                    .get(&file)
                    .is_some_and(|facts| facts.test_scoped(line));
                add_edge(
                    b,
                    &mut node,
                    &target,
                    super::super::edges::RUST_PATH,
                    line,
                    &|_| test,
                );
                if node
                    .edges
                    .get(&target)
                    .is_some_and(|kinds| kinds.contains_key(super::super::edges::RUST_PATH))
                    && let Some(facts) = b.facts.get_mut(&file)
                {
                    facts.file_edges.push((
                        target,
                        super::super::edges::RUST_PATH.to_owned(),
                        line,
                    ));
                }
            }
            b.nodes.insert(file.clone(), node);
            if let Some(facts) = b.facts.get_mut(&file) {
                facts.imports.extend(imports);
                facts.rust_path_uses.extend(uses);
            }
        }
    }
}
