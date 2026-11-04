//! What a Cargo package's sources say about loading one of its files:
//! the `include!`s rust-analyzer may not load and the module declarations of
//! the file, scanned once per state of the package.
use super::{CARGO_MANIFEST, cargo, cargo_package_dir, lexical, modules};
use crate::stamp::Proof;
use octocode_io::memo::Memo;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Source a Cargo package `include!`s that rust-analyzer may not load.
/// rust-analyzer loads a package's own directory (and, once build scripts
/// ran, their `OUT_DIR`); an `include!` it cannot load leaves every item the
/// included file defines unknown, so uses that reach a symbol through those
/// types (a method called through `Deref` to one, a field of one) do not
/// resolve.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackageIncludes {
    /// Package files that `include!` build-script output (`env!(…)`, such
    /// as `concat!(env!("OUT_DIR"), …)`): loaded only when the package has a
    /// build script and rust-analyzer runs it.
    pub generated: Vec<PathBuf>,
    /// Literal `include!` targets outside the package directory (lexically
    /// normalized): never loaded, whatever the build context.
    pub outside: Vec<PathBuf>,
    /// Package files that declare the scanned file as a module (`mod x;`,
    /// or `#[path = "…"] mod x;`). Two or more compile it into several
    /// crates (a bench and a test sharing a support file); rust-analyzer
    /// analyzes it as part of one, so uses in the others' copies are not
    /// references of the symbol it answers for.
    pub declared_by: Vec<PathBuf>,
    /// The request stopped the scan: more includes may exist.
    pub has_more: bool,
}

/// What a package's sources say, whatever the anchor.
#[derive(Default)]
struct PackageScan {
    /// Sources with a build-script `include!`.
    generated: Vec<PathBuf>,
    outside: Vec<PathBuf>,
    modules: modules::ModuleSources,
}

impl PackageScan {
    /// About how many bytes this scan holds.
    fn weight(&self) -> usize {
        self.generated
            .iter()
            .chain(&self.outside)
            .map(|path| path.as_os_str().len() + std::mem::size_of::<PathBuf>())
            .sum::<usize>()
            + self.modules.weight()
    }
}

/// A finished scan and the stamps that prove it: every source, directory and
/// ignore input of the walk.
struct KeptScan {
    inputs: Proof,
    scan: Arc<PackageScan>,
}

/// Scans kept this process, by package, against the one process budget.
static SCANS: Memo<PathBuf, KeptScan> = Memo::new(&crate::memory::MEMORY);

/// Walk `package` and read every one of its `.rs` files (nested packages and
/// ignored files are not its sources) until `stopped`. `None` when it stopped.
fn scan_package(package: &Path, stopped: &dyn Fn() -> bool) -> Option<(PackageScan, Proof)> {
    let root = package.to_path_buf();
    let walker = ignore::WalkBuilder::new(package)
        .filter_entry(move |entry| {
            entry.path() == root
                || !entry.file_type().is_some_and(|kind| kind.is_dir())
                || !entry.path().join(CARGO_MANIFEST).is_file()
        })
        .build();
    let mut scan = PackageScan::default();
    let mut inputs = Proof::default();
    for entry in walker.flatten() {
        let path = entry.path();
        if entry.file_type().is_some_and(|kind| kind.is_dir()) {
            inputs.follow(path.to_path_buf());
            for name in crate::search::walk::IGNORE_FILES {
                let ignore = path.join(name);
                if ignore.exists() {
                    inputs.follow(ignore);
                }
            }
            continue;
        }
        if !entry.file_type().is_some_and(|kind| kind.is_file())
            || path.extension().is_none_or(|ext| ext != "rs")
        {
            continue;
        }
        if stopped() {
            return None;
        }
        // Before the read: an edit after it moves the stamp.
        inputs.follow(path.to_path_buf());
        #[cfg(test)]
        PACKAGE_FILES_READ.with(|read| read.set(read.get() + 1));
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        for argument in include_arguments(&text) {
            match argument {
                IncludeArgument::Generated => {
                    if !scan.generated.iter().any(|seen| seen == path) {
                        scan.generated.push(path.to_path_buf());
                    }
                }
                IncludeArgument::Literal(target) => {
                    let target = lexical(&path.parent().unwrap_or(path).join(target));
                    if !target.starts_with(package) && !scan.outside.contains(&target) {
                        scan.outside.push(target);
                    }
                }
            }
        }
        scan.modules.push(path, &text);
    }
    for rule in crate::search::walk::external_ignore_inputs(package) {
        inputs.follow(rule);
    }
    for dir in package.ancestors() {
        for marker in crate::search::repository::MARKERS {
            inputs.presence(dir.join(marker));
        }
    }
    Some((scan, inputs))
}

/// The scan of `package`: a kept one while its inputs hold, else a new one, kept
/// when nothing it read was written within the settle window before it. `None`
/// when `stopped` ended a new one.
fn package_scan(package: &Path, stopped: &dyn Fn() -> bool) -> Option<Arc<PackageScan>> {
    if let Some(kept) = SCANS.get(package).filter(|kept| kept.value.inputs.holds()) {
        return Some(Arc::clone(&kept.value.scan));
    }
    let started = crate::stamp::settle_clock();
    let (scan, inputs) = scan_package(package, stopped)?;
    let scan = Arc::new(scan);
    if inputs.settled_before(started) {
        let weight = inputs.weight() + scan.weight();
        SCANS.insert(
            package.to_path_buf(),
            KeptScan {
                inputs,
                scan: Arc::clone(&scan),
            },
            weight,
            None,
        );
    }
    Some(scan)
}

/// [`PackageIncludes`] of the Cargo package `file` belongs to, from every one of
/// its `.rs` files (read once per state of the package, [`package_scan`]) unless
/// `stopped` (the request's cancellation or deadline) says to stop, which
/// `has_more` discloses: no file count bounds it. `generated` is empty when the
/// package has no build script: without one no build-script output exists to
/// load.
pub fn cargo_package_includes(file: &Path, stopped: &dyn Fn() -> bool) -> Option<PackageIncludes> {
    let package = cargo_package_dir(file.parent()?)?;
    let Some(scan) = package_scan(&package, stopped) else {
        return Some(PackageIncludes {
            has_more: true,
            ..PackageIncludes::default()
        });
    };
    let build_script = cargo_package_build_script(&package);
    let generated = if build_script.is_some() {
        scan.generated.clone()
    } else {
        Vec::new()
    };
    let targets = cargo::TargetRoots::new(&package, build_script);
    Some(PackageIncludes {
        generated,
        outside: scan.outside.clone(),
        declared_by: scan.modules.declared_by(&lexical(file), &targets),
        has_more: false,
    })
}

#[cfg(test)]
thread_local! {
    /// Package sources this thread's scans read.
    pub(super) static PACKAGE_FILES_READ: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// What an `include!` reads: a build-script value (`env!`, possibly in a
/// `concat!`) or a literal path relative to the including file.
#[derive(Debug, PartialEq, Eq)]
enum IncludeArgument {
    Generated,
    Literal(String),
}

/// The arguments of the `include!` invocations in `text`, outside line
/// comments. `include_str!`/`include_bytes!` read data, not source.
fn include_arguments(text: &str) -> Vec<IncludeArgument> {
    let mut found = Vec::new();
    for line in text.lines() {
        let code = line.split("//").next().unwrap_or(line);
        for (at, _) in code.match_indices("include!") {
            if code[..at]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_')
            {
                continue;
            }
            let rest = code[at + "include!".len()..].trim_start();
            let Some(argument) = rest
                .strip_prefix('(')
                .or_else(|| rest.strip_prefix('{'))
                .or_else(|| rest.strip_prefix('['))
                .map(str::trim_start)
            else {
                continue;
            };
            if let Some(literal) = argument.strip_prefix('"') {
                if let Some(end) = literal.find('"') {
                    found.push(IncludeArgument::Literal(literal[..end].to_owned()));
                }
            } else if argument.starts_with("env!") || argument.starts_with("concat!") {
                found.push(IncludeArgument::Generated);
            }
        }
    }
    found
}

/// The build script of the Cargo package at `package` (its manifest
/// directory), when it has one: `[package] build = "path"`, else `build.rs`
/// beside the manifest (Cargo's default), unless `build = false`.
pub(super) fn cargo_package_build_script(package: &Path) -> Option<PathBuf> {
    let manifest = cargo::manifest(package)?;
    let settings = manifest.get("package")?;
    if settings.get("build").and_then(toml::Value::as_bool) == Some(false) {
        return None;
    }
    let script = package.join(
        settings
            .get("build")
            .and_then(toml::Value::as_str)
            .unwrap_or("build.rs"),
    );
    script.is_file().then_some(script)
}
