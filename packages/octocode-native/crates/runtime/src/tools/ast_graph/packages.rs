//! Package-level import linking for Go and Java.
//!
//! Neither language imports a file. A Go import names a package directory
//! through the module path its `go.mod` declares
//! (`github.com/prometheus/prometheus/tsdb` → `tsdb/*.go`); a Java import
//! names a class by its package path (`com.google.common.collect.ImmutableList`
//! → `…/com/google/common/collect/ImmutableList.java` under any source root).
//! An import therefore links to every non-test source file of the package
//! (Go, Java `.*`) or to the class's file (Java). An import that names no scanned
//! module stays external unless the repository shows it is local: a module the
//! enclosing `go.mod`, a local `replace` or `go.work` names, a Java package the
//! scan holds or a class file above the scan root. Java classes of
//! one package use each other without an import; a call, constructor, or
//! heritage clause naming a same-package class links to its file. Go
//! same-package references produce no edge.

use super::aliases::BareTarget;
use super::linking::Linker;
use super::paths::{dirname, dirs_above_within_repository};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

mod go_modules;
mod java;

#[derive(Default)]
pub(super) struct PackageIndex {
    /// `(module path, module dir)`, longest module path first.
    go_modules: Vec<(String, String)>,
    files_by_dir: BTreeMap<String, Vec<String>>,
    java_by_name: HashMap<String, Vec<String>>,
    /// Go files were scanned but no `go.mod` names their module, at or above
    /// the scan root: every module import is unlinkable, not external.
    go_module_missing: bool,
    /// Each Go file's package clause: the name a plain import of it binds.
    go_package_names: HashMap<String, String>,
    /// Module paths local to the repository but not scanned: the module
    /// enclosing the scan root, a local `replace`, a `go.work` member; each
    /// with whether its directory lies outside the scan root.
    go_local_modules: Vec<(String, bool)>,
    /// Every trailing run of a scanned Java directory's components: the package
    /// paths the scan may hold (`com/acme` for `src/main/java/com/acme`).
    java_packages: HashSet<String>,
    /// Scan root's components, nearest first, as a Java package path may spell them.
    java_root_names: Vec<String>,
    /// The directories above the scan root, up to the repository boundary.
    above_root: Vec<PathBuf>,
    /// Java package or class path → whether a file above the scan root spells it.
    java_outer_hits: std::sync::Mutex<HashMap<String, bool>>,
}

/// How a package import resolved.
pub(super) enum PackageLink {
    Files(Vec<String>),
    /// Links no file: a third-party package, or project code the scan should
    /// hold (a scanned module/package with no linkable file) or that lies
    /// beyond the scan root.
    Unlinked(BareTarget),
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

/// The module enclosing `root` from an ancestor directory, `root`'s path below
/// that module's directory, and the modules its `go.mod` replaces locally
/// with whether each lies outside `root`.
fn enclosing_go_module(
    root: &Path,
    read: &dyn Fn(&Path) -> Option<String>,
) -> Option<(String, String, Vec<(String, bool)>)> {
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
            let text = read(&manifest)?;
            let module = go_module_path(&text)?;
            below.reverse();
            return Some((
                module,
                below.join("/"),
                go_modules::local_replacements(&text, dir, root),
            ));
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
            if linker == Linker::Java && dir != "." {
                let mut package = dir.as_str();
                loop {
                    if !index.java_packages.insert(package.to_owned()) {
                        break;
                    }
                    let Some((_, rest)) = package.split_once('/') else {
                        break;
                    };
                    package = rest;
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
            if let Some(text) = read(&manifest) {
                let directory = manifest.parent().unwrap_or(root);
                index
                    .go_local_modules
                    .extend(go_modules::local_replacements(&text, directory, root));
                if let Some(module) = go_module_path(&text) {
                    index.go_modules.push((module, dir));
                }
            }
        }
        if has_go && !index.go_modules.iter().any(|(_, dir)| dir == ".") {
            // A scan rooted below its module (`go/tsdb` under `go/go.mod`)
            // imports its own packages by the enclosing module path; map
            // the scan root to `<module>/<root relative to the module>`.
            if let Some((module, below, replaced)) = enclosing_go_module(root, read) {
                index
                    .go_modules
                    .push((format!("{module}/{below}"), ".".to_owned()));
                // Its other packages lie above the scan root.
                index.go_local_modules.push((module, true));
                index.go_local_modules.extend(replaced);
            }
        }
        index.above_root = dirs_above_within_repository(root);
        if has_go {
            index.go_local_modules.extend(go_modules::workspace_members(
                root,
                &index.above_root,
                read,
            ));
        }
        index.go_module_missing = has_go && index.go_modules.is_empty();
        index.java_root_names = root
            .components()
            .rev()
            .take(MAX_GO_MODULE_ANCESTORS)
            .filter_map(|part| part.as_os_str().to_str().map(str::to_owned))
            .collect();
        if !index.java_by_name.is_empty() {
            // The scan root may itself be a package (`.../com/acme`).
            let mut below = String::new();
            for name in &index.java_root_names {
                below = if below.is_empty() {
                    name.clone()
                } else {
                    format!("{name}/{below}")
                };
                index.java_packages.insert(below.clone());
            }
        }
        index
            .go_modules
            .sort_by_key(|(module, _)| std::cmp::Reverse(module.len()));
        index
    }

    pub(super) fn go_module_missing(&self) -> bool {
        self.go_module_missing
    }

    /// What each scanned Go file's own facts say of it: the name its package
    /// clause binds, and whether a build constraint keeps it out of its
    /// package, so no import reaches it.
    pub(super) fn with_go_files<'a>(
        mut self,
        files: impl Iterator<Item = (&'a str, Option<&'a str>, bool)>,
    ) -> Self {
        for (file, package, excluded) in files {
            if let Some(package) = package {
                self.go_package_names
                    .insert(file.to_owned(), package.to_owned());
            }
            if excluded && let Some(files) = self.files_by_dir.get_mut(dirname(file)) {
                files.retain(|known| known != file);
            }
        }
        self
    }

    /// The name a plain Go import of `file`'s package binds.
    pub(super) fn go_package_name(&self, file: &str) -> Option<&str> {
        self.go_package_names.get(file).map(String::as_str)
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
            _ => PackageLink::Unlinked(BareTarget::External),
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
            // With no module to compare it to, a module path could be anyone's;
            // a path with no dot in its first segment is the standard library's.
            let unknowable = self.go_module_missing
                && spec
                    .split('/')
                    .next()
                    .is_some_and(|host| host.contains('.'));
            if unknowable {
                return PackageLink::Unlinked(BareTarget::Internal);
            }
            // The longest local module the path names decides where it lies.
            return PackageLink::Unlinked(
                self.go_local_modules
                    .iter()
                    .filter(|(module, _)| spec == module || spec.starts_with(&format!("{module}/")))
                    .max_by_key(|(module, _)| module.len())
                    .map_or(BareTarget::External, |(_, outside)| {
                        if *outside {
                            BareTarget::OutsideRoot
                        } else {
                            BareTarget::Internal
                        }
                    }),
            );
        };
        let rest = spec[module.len()..].trim_start_matches('/');
        let files = self.package_files(&join(dir, rest), ".go");
        if files.is_empty() {
            PackageLink::Unlinked(BareTarget::Internal)
        } else {
            PackageLink::Files(files)
        }
    }
}

#[cfg(test)]
mod tests;
