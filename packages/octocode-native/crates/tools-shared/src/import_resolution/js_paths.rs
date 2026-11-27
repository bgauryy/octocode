//! Build-output mapping and declaration suffixes shared by graph resolution and roots.
use super::paths::{join, join_within_root, normalize_specifier};
use std::collections::{BTreeSet, HashMap};

/// Package-relative build-output roots mapped back to sources.
pub(crate) const BUILD_DIRS: &[&str] = &[
    "dist",
    "build",
    "out",
    "lib",
    "esm",
    "cjs",
    "es",
    "types",
    "declarations",
];

/// Output flavor directories nested under a build root (`dist/esm/…`).
const FLAVOR_DIRS: &[&str] = &[
    "esm",
    "cjs",
    "es",
    "mjs",
    "lib",
    "types",
    "prod",
    "dev",
    "production",
    "development",
    "node",
    "browser",
    "module",
    "commonjs",
    "src",
];

/// Source suffixes (leading dot) in the order TypeScript probes an
/// extensionless module path. Not the engine's `JS_EXTENSIONS`, which holds
/// the dotless JavaScript-only set.
const MODULE_SUFFIXES: &[&str] = &[".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".mts", ".cts"];

pub const DECLARATION_EXTENSIONS: &[&str] = &[".d.ts", ".d.mts", ".d.cts"];

pub fn is_declaration(path: &str) -> bool {
    DECLARATION_EXTENSIONS.iter().any(|ext| path.ends_with(ext))
}

/// Strip a JavaScript/TypeScript or declaration extension.
fn strip_code_extension(path: &str) -> &str {
    DECLARATION_EXTENSIONS
        .iter()
        .chain(MODULE_SUFFIXES)
        .find_map(|ext| path.strip_suffix(ext))
        .unwrap_or(path)
}

/// Map a package-relative build-output target (`dist/esm/x.js`,
/// `lib/x.d.ts`) back to its source (`src/x.ts`, `x.ts`).
pub(crate) fn source_equivalent(
    dir: &str,
    target: &str,
    known: &BTreeSet<String>,
) -> Option<String> {
    let target = normalize_specifier(target);
    let segments = target.split('/').collect::<Vec<_>>();
    if !segments
        .first()
        .is_some_and(|first| BUILD_DIRS.contains(first))
    {
        return None;
    }
    let mut rests = vec![segments[1..].join("/")];
    let mut index = 1;
    while index + 1 < segments.len() && FLAVOR_DIRS.contains(&segments[index]) {
        index += 1;
        rests.push(segments[index..].join("/"));
    }
    rests.iter().find_map(|rest| {
        let stem = strip_code_extension(rest);
        ["src", "."].iter().find_map(|base| {
            let candidate = join_within_root(&join(dir, base), stem)?;
            probe_js(&candidate, known)
        })
    })
}

/// Whether `stem` names a module file on disk: itself, with a module suffix, or
/// a directory's `index`.
pub(crate) fn module_on_disk(stem: &std::path::Path) -> bool {
    stem.is_file()
        || MODULE_SUFFIXES.iter().any(|suffix| {
            let suffix = suffix.trim_start_matches('.');
            stem.with_added_extension(suffix).is_file()
                || stem.join("index").with_added_extension(suffix).is_file()
        })
}

/// Probe a module path the way relative imports are linked: exact file,
/// `.js`→`.ts` style swaps, extension probing, and `index` files.
pub fn probe_js(stem: &str, known: &BTreeSet<String>) -> Option<String> {
    if stem.is_empty() || stem == "." {
        return MODULE_SUFFIXES
            .iter()
            .map(|ext| format!("index{ext}"))
            .find(|candidate| known.contains(candidate));
    }
    if MODULE_SUFFIXES.iter().any(|ext| stem.ends_with(ext)) {
        if known.contains(stem) {
            return Some(stem.to_owned());
        }
        // TypeScript resolves `./x.js` to the source or declaration that
        // `x.js` is built from, in this order.
        let (base, extension) = stem.rsplit_once('.')?;
        return octocode_engine::signatures::languages::js_source_swaps(extension)
            .iter()
            .map(|swap| format!("{base}.{swap}"))
            .find(|candidate| known.contains(candidate));
    }
    // `./x` is `x.ts` … or `x.d.ts`, then the directory's `index`.
    let probed = |name: String| {
        MODULE_SUFFIXES
            .iter()
            .chain(&DECLARATION_EXTENSIONS[..1])
            .map(|ext| format!("{name}{ext}"))
            .collect::<Vec<_>>()
    };
    std::iter::once(stem.to_owned())
        .chain(probed(stem.to_owned()))
        .chain(probed(join(stem, "index")))
        .find(|candidate| known.contains(candidate))
}

/// Scanned JS/TS modules by the last segment of their module path (a
/// directory's `index` also under the directory), so a bare specifier finds
/// the files its path names without testing every file.
pub struct ModulesByName<'k>(HashMap<&'k str, Vec<&'k str>>);

impl<'k> ModulesByName<'k> {
    pub fn new(files: impl IntoIterator<Item = &'k str>) -> Self {
        let mut by_name = HashMap::<&str, Vec<&str>>::new();
        for file in files {
            let stem = strip_code_extension(file);
            for module in std::iter::once(stem).chain(stem.strip_suffix("/index")) {
                let entry = by_name.entry(last_segment(module)).or_default();
                if entry.last() != Some(&file) {
                    entry.push(file);
                }
            }
        }
        Self(by_name)
    }

    /// The files a bare, multi-segment specifier names by a trailing path:
    /// `vs/base/common/x` names `src/vs/base/common/x.ts`, and `src/dir`
    /// names `src/dir/index.ts`.
    pub fn named_by<'s>(&'s self, specifier: &'s str) -> impl Iterator<Item = &'k str> + 's {
        let path = (!specifier.starts_with(['.', '/']) && specifier.contains('/'))
            .then(|| strip_code_extension(specifier.trim_end_matches('/')));
        path.and_then(|path| self.0.get(last_segment(path)).map(|files| (path, files)))
            .into_iter()
            .flat_map(|(path, files)| {
                files.iter().copied().filter(move |file| {
                    let stem = strip_code_extension(file);
                    std::iter::once(stem)
                        .chain(stem.strip_suffix("/index"))
                        .any(|module| {
                            module
                                .strip_suffix(path)
                                .is_some_and(|rest| rest.is_empty() || rest.ends_with('/'))
                        })
                })
            })
    }
}

fn last_segment(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Type declarations from non-JavaScript languages.
pub const TYPE_STUB_EXTENSIONS: &[&str] = &[".pyi"];
/// Bundle classification stays narrower than source mapping: `lib` and
/// `esm` can contain hand-authored JavaScript whose symbols must stay visible.
pub const BUNDLED_OUTPUT_DIRS: &[&str] = &["dist", "build", "out", ".next", ".output", ".nuxt"];

#[cfg(test)]
mod tests;
