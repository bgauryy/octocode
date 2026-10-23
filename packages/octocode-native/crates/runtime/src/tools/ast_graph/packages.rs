//! Package-level import linking for Go and Java.
//!
//! Neither language imports a file. A Go import names a package directory
//! through the module path its `go.mod` declares
//! (`github.com/prometheus/prometheus/tsdb` → `tsdb/*.go`); a Java import
//! names a class by its package path (`com.google.common.collect.ImmutableList`
//! → `…/com/google/common/collect/ImmutableList.java` under any source root).
//! An import therefore links to every non-test source file of the package
//! (Go, Java `.*`) or to the class's file (Java). Imports outside the scanned
//! modules (standard library, third-party) stay external. Java classes of
//! one package use each other without an import; a call, constructor, or
//! heritage clause naming a same-package class links to its file. Go
//! same-package references produce no edge.

use super::graph::dirname;
use super::linking::Linker;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

#[derive(Default)]
pub(super) struct PackageIndex {
    /// `(module path, module dir)`, longest module path first.
    go_modules: Vec<(String, String)>,
    files_by_dir: BTreeMap<String, Vec<String>>,
    java_by_name: HashMap<String, Vec<String>>,
    /// Go files were scanned but no `go.mod` names their module, at or above
    /// the scan root: every module import is unlinkable, not external.
    go_module_missing: bool,
}

/// How a package import resolved.
pub(super) enum PackageLink {
    Files(Vec<String>),
    /// Names a scanned module/package that holds no linkable file.
    UnresolvedInternal,
    External,
}

fn join(dir: &str, rest: &str) -> String {
    match (dir, rest) {
        (".", rest) => rest.to_owned(),
        (dir, "") => dir.to_owned(),
        (dir, rest) => format!("{dir}/{rest}"),
    }
}

fn go_module_path(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("module"))
        .map(|rest| rest.trim().trim_matches('"').to_owned())
        .filter(|path| !path.is_empty() && !path.contains(char::is_whitespace))
}

/// Enclosing `go.mod` directories searched above a scan root.
const MAX_GO_MODULE_ANCESTORS: usize = 16;

/// The module path of `root` itself when it sits inside a module whose
/// `go.mod` is in an ancestor directory: the module path joined with the
/// root's path below the module directory.
fn enclosing_go_module(root: &Path, read: &dyn Fn(&Path) -> Option<String>) -> Option<String> {
    if octocode_engine::search::repository::is_root(root) {
        return None;
    }
    let mut below = Vec::new();
    let mut dir = root;
    for _ in 0..MAX_GO_MODULE_ANCESTORS {
        below.push(dir.file_name()?.to_string_lossy().into_owned());
        dir = dir.parent()?;
        let manifest = dir.join("go.mod");
        if manifest.is_file() {
            let module = go_module_path(&read(&manifest)?)?;
            below.reverse();
            return Some(format!("{module}/{}", below.join("/")));
        }
        // A repository boundary ends the search: its go.mod would belong
        // to another project.
        if octocode_engine::search::repository::is_root(dir) {
            return None;
        }
    }
    None
}

/// Shared path components; the candidate nearest the importer wins.
fn closeness(a: &str, b: &str) -> usize {
    a.split('/')
        .zip(b.split('/'))
        .take_while(|(x, y)| x == y)
        .count()
}

impl PackageIndex {
    /// `read` reads each `go.mod` (also the enclosing one above the scan
    /// root) through the caller's path and content policy.
    pub(super) fn build(
        root: &Path,
        known: &BTreeSet<String>,
        read: &dyn Fn(&Path) -> Option<String>,
    ) -> Self {
        let mut index = Self::default();
        let mut go_dirs = BTreeSet::new();
        for file in known {
            let dir = dirname(file).to_owned();
            let linker = Linker::of(file);
            if matches!(linker, Linker::Go | Linker::Java) {
                index
                    .files_by_dir
                    .entry(dir.clone())
                    .or_default()
                    .push(file.clone());
            }
            if linker == Linker::Go {
                let mut current = dir.as_str();
                loop {
                    if !go_dirs.insert(current.to_owned()) {
                        break;
                    }
                    if current == "." {
                        break;
                    }
                    current = dirname(current);
                }
            }
            if let Some(name) = file.rsplit('/').next().filter(|_| linker == Linker::Java) {
                index
                    .java_by_name
                    .entry(name.to_owned())
                    .or_default()
                    .push(file.clone());
            }
        }
        let has_go = !go_dirs.is_empty();
        for dir in go_dirs {
            let manifest = if dir == "." {
                root.join("go.mod")
            } else {
                root.join(&dir).join("go.mod")
            };
            if let Some(module) = read(&manifest).as_deref().and_then(go_module_path) {
                index.go_modules.push((module, dir));
            }
        }
        if has_go && !index.go_modules.iter().any(|(_, dir)| dir == ".") {
            // A scan rooted below its module (`go/tsdb` under `go/go.mod`)
            // imports its own packages by the enclosing module path; map
            // the scan root to `<module>/<root relative to the module>`.
            if let Some(module) = enclosing_go_module(root, read) {
                index.go_modules.push((module, ".".to_owned()));
            }
        }
        index.go_module_missing = has_go && index.go_modules.is_empty();
        index
            .go_modules
            .sort_by_key(|(module, _)| std::cmp::Reverse(module.len()));
        index
    }

    pub(super) fn go_module_missing(&self) -> bool {
        self.go_module_missing
    }

    pub(super) fn resolve(
        &self,
        lang: super::linking::Linker,
        spec: &str,
        importer: &str,
    ) -> PackageLink {
        match lang {
            super::linking::Linker::Go => self.resolve_go(spec),
            super::linking::Linker::Java => self.resolve_java(spec, importer),
            _ => PackageLink::External,
        }
    }

    /// The Java class file `name` in the importer's own package: Java code
    /// uses same-package classes without an import.
    pub(super) fn same_package_class(&self, importer: &str, name: &str) -> Option<&str> {
        if Linker::of(importer) != Linker::Java
            || !name.starts_with(|c: char| c.is_ascii_uppercase())
        {
            return None;
        }
        let dir = dirname(importer);
        let target = join(dir, &format!("{name}.java"));
        self.files_by_dir
            .get(dir)?
            .iter()
            .find(|file| **file == target && file.as_str() != importer)
            .map(String::as_str)
    }

    fn package_files(&self, dir: &str, ext: &str) -> Vec<String> {
        self.files_by_dir
            .get(dir)
            .into_iter()
            .flatten()
            .filter(|file| file.ends_with(ext) && !file.ends_with("_test.go"))
            .cloned()
            .collect()
    }

    fn resolve_go(&self, spec: &str) -> PackageLink {
        let Some((module, dir)) = self
            .go_modules
            .iter()
            .find(|(module, _)| spec == module || spec.starts_with(&format!("{module}/")))
        else {
            return PackageLink::External;
        };
        let rest = spec[module.len()..].trim_start_matches('/');
        let files = self.package_files(&join(dir, rest), ".go");
        if files.is_empty() {
            PackageLink::UnresolvedInternal
        } else {
            PackageLink::Files(files)
        }
    }

    fn resolve_java(&self, spec: &str, importer: &str) -> PackageLink {
        let spec = spec.strip_prefix("static ").unwrap_or(spec).trim();
        if let Some(package) = spec.strip_suffix(".*") {
            let tail = package.replace('.', "/");
            let best = self
                .files_by_dir
                .keys()
                .filter(|dir| *dir == &tail || dir.ends_with(&format!("/{tail}")))
                .max_by_key(|dir| closeness(dir, importer));
            return match best {
                Some(dir) => {
                    let files = self.package_files(dir, ".java");
                    if files.is_empty() {
                        PackageLink::UnresolvedInternal
                    } else {
                        PackageLink::Files(files)
                    }
                }
                None => PackageLink::External,
            };
        }
        // `a.b.C`, `a.b.C.Inner`, and `static a.b.C.member` all name the
        // file of the longest prefix that is a scanned class.
        let segments = spec.split('.').collect::<Vec<_>>();
        for end in (2..=segments.len()).rev() {
            let tail = format!("{}.java", segments[..end].join("/"));
            let name = format!("{}.java", segments[end - 1]);
            let best = self
                .java_by_name
                .get(&name)
                .into_iter()
                .flatten()
                .filter(|file| **file == tail || file.ends_with(&format!("/{tail}")))
                .max_by_key(|file| closeness(file, importer));
            if let Some(file) = best {
                return PackageLink::Files(vec![file.clone()]);
            }
        }
        PackageLink::External
    }
}

#[cfg(test)]
mod tests;
