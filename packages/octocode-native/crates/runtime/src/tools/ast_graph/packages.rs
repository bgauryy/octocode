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

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

/// Files one package import may link to; larger packages are cut, not
/// dropped, so the graph stays bounded.
const MAX_PACKAGE_TARGETS: usize = 200;

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

fn dir_of(path: &str) -> &str {
    path.rsplit_once('/').map_or(".", |(dir, _)| dir)
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
    if root.join(".git").exists() {
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
        if dir.join(".git").exists() {
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
            let dir = dir_of(file).to_owned();
            if file.ends_with(".go") || file.ends_with(".java") {
                index
                    .files_by_dir
                    .entry(dir.clone())
                    .or_default()
                    .push(file.clone());
            }
            if file.ends_with(".go") {
                let mut current = dir.as_str();
                loop {
                    if !go_dirs.insert(current.to_owned()) {
                        break;
                    }
                    if current == "." {
                        break;
                    }
                    current = dir_of(current);
                }
            }
            if let Some(name) = file
                .rsplit('/')
                .next()
                .filter(|name| name.ends_with(".java"))
            {
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

    pub(super) fn resolve(&self, ext: &str, spec: &str, importer: &str) -> PackageLink {
        match ext {
            "go" => self.resolve_go(spec),
            "java" => self.resolve_java(spec, importer),
            _ => PackageLink::External,
        }
    }

    /// The Java class file `name` in the importer's own package: Java code
    /// uses same-package classes without an import.
    pub(super) fn same_package_class(&self, importer: &str, name: &str) -> Option<&str> {
        if !importer.ends_with(".java") || !name.starts_with(|c: char| c.is_ascii_uppercase()) {
            return None;
        }
        let dir = dir_of(importer);
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
            .take(MAX_PACKAGE_TARGETS)
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
mod tests {
    use super::*;

    fn index(root: &Path, files: &[&str]) -> PackageIndex {
        let known = files.iter().map(|file| (*file).to_owned()).collect();
        PackageIndex::build(root, &known, &|path| std::fs::read_to_string(path).ok())
    }

    fn files(link: PackageLink) -> Vec<String> {
        match link {
            PackageLink::Files(files) => files,
            PackageLink::UnresolvedInternal => vec!["<internal>".into()],
            PackageLink::External => vec![],
        }
    }

    #[test]
    fn go_module_imports_link_package_files_without_tests() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("go.mod"),
            "module example.com/app\n\ngo 1.22\n",
        )
        .expect("go.mod");
        let index = index(
            dir.path(),
            &[
                "main.go",
                "tsdb/head.go",
                "tsdb/db.go",
                "tsdb/db_test.go",
                "empty/x_test.go",
            ],
        );
        assert_eq!(
            files(index.resolve("go", "example.com/app/tsdb", "main.go")),
            vec!["tsdb/db.go", "tsdb/head.go"]
        );
        assert_eq!(
            files(index.resolve("go", "fmt", "main.go")),
            Vec::<String>::new()
        );
        assert_eq!(
            files(index.resolve("go", "example.com/app/empty", "main.go")),
            vec!["<internal>"]
        );
        assert_eq!(
            files(index.resolve("go", "example.com/application", "main.go")),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_scan_below_its_module_maps_through_the_enclosing_go_mod() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("go.mod"), "module example.com/app\n").expect("go.mod");
        let sub = dir.path().join("tsdb");
        std::fs::create_dir_all(&sub).expect("sub");
        let scanned = index(&sub, &["db.go", "chunkenc/chunk.go"]);
        assert!(!scanned.go_module_missing());
        assert_eq!(
            files(scanned.resolve("go", "example.com/app/tsdb/chunkenc", "db.go")),
            vec!["chunkenc/chunk.go"]
        );
        // A module import outside the scanned subtree stays external.
        assert_eq!(
            files(scanned.resolve("go", "example.com/app/model", "db.go")),
            Vec::<String>::new()
        );
        // No go.mod anywhere: module imports are unlinkable, not external.
        let bare = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(bare.path().join(".git")).expect("repository boundary");
        assert!(index(bare.path(), &["main.go"]).go_module_missing());
    }

    #[test]
    fn nested_go_modules_resolve_by_longest_module_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("go.mod"), "module example.com/app\n").expect("root");
        std::fs::create_dir_all(dir.path().join("sub")).expect("sub");
        std::fs::write(dir.path().join("sub/go.mod"), "module example.com/sub\n").expect("sub mod");
        let index = index(dir.path(), &["main.go", "sub/pkg/a.go"]);
        assert_eq!(
            files(index.resolve("go", "example.com/sub/pkg", "main.go")),
            vec!["sub/pkg/a.go"]
        );
    }

    #[test]
    fn java_imports_link_classes_nested_static_and_wildcards() {
        let dir = tempfile::tempdir().expect("tempdir");
        let index = index(
            dir.path(),
            &[
                "guava/src/com/g/collect/ImmutableList.java",
                "guava/src/com/g/collect/Lists.java",
                "android/guava/src/com/g/collect/ImmutableList.java",
                "guava/src/com/g/base/Preconditions.java",
                "guava/src/com/g/app/Main.java",
            ],
        );
        let importer = "guava/src/com/g/app/Main.java";
        assert_eq!(
            files(index.resolve("java", "com.g.collect.ImmutableList", importer)),
            vec!["guava/src/com/g/collect/ImmutableList.java"]
        );
        assert_eq!(
            files(index.resolve("java", "com.g.collect.ImmutableList.Builder", importer)),
            vec!["guava/src/com/g/collect/ImmutableList.java"]
        );
        assert_eq!(
            files(index.resolve(
                "java",
                "static com.g.base.Preconditions.checkNotNull",
                importer
            )),
            vec!["guava/src/com/g/base/Preconditions.java"]
        );
        assert_eq!(
            files(index.resolve("java", "com.g.collect.*", importer)).len(),
            2
        );
        assert_eq!(
            files(index.resolve("java", "java.util.List", importer)),
            Vec::<String>::new()
        );
    }
}
