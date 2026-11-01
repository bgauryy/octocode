//! Which text-scan files could reference the anchor's declaration at all.
//!
//! A file that spells the name but lies outside the declaration's reach is
//! no unverified site: it is out of reach, counted apart
//! (`coverage.outOfReachFiles`), and it neither makes a row partial nor
//! becomes a lead. Rust bounds reach by the item's written visibility and
//! its inherent container's (a method of a `pub(super)` type), and by crate:
//! the defining package, or one that depends on it ([`rust`]). TS/JS reach
//! is the importer pre-filter (`importers::reach`): a file whose imports
//! name only other modules is set aside as checked there. Other languages,
//! an anchor that does not start a declaration, trait items, macros and
//! every undecidable file keep every file in reach.
use std::path::{Path, PathBuf};

mod cargo;
mod rust;

/// The module files of `relocated` (declaring file, module file) that
/// belong to `territory`: declared from one of its files, or from a module
/// already placed in it.
fn placed_in(territory: &Path, relocated: Vec<(PathBuf, PathBuf)>) -> Vec<PathBuf> {
    let mut placed: Vec<PathBuf> = Vec::new();
    let mut pending = relocated;
    loop {
        let (inside, rest): (Vec<_>, Vec<_>) = pending.into_iter().partition(|(declarer, _)| {
            rust::in_territory(declarer, territory)
                || placed
                    .iter()
                    .any(|module| rust::in_module(declarer, module))
        });
        if inside.is_empty() {
            return placed;
        }
        placed.extend(inside.into_iter().map(|(_, module)| module));
        pending = rest;
    }
}

/// The request's anchor: canonical file and zero-based position.
#[derive(Clone, Debug)]
pub(super) struct Site {
    pub(super) path: String,
    pub(super) line: u32,
    pub(super) character: u32,
}

/// The reach of the declaration the anchor starts.
pub(super) struct Reach {
    bound: rust::Bound,
    /// The defining package (canonical manifest directory) and its name.
    package: PathBuf,
    name: String,
    /// What the written visibility reads, as `pub(super)`.
    written: String,
    /// The modules of the territory placed by `#[path]`, which may sit
    /// outside its directory; read once, on the first file outside it.
    relocated: std::sync::OnceLock<Vec<PathBuf>>,
    /// Whether a package (its manifest directory) depends on the defining
    /// one, directly or through local path dependencies.
    dependents: std::sync::Mutex<std::collections::HashMap<PathBuf, bool>>,
}

impl Reach {
    /// The reach of the declaration of `symbol` that the anchor `site`
    /// starts; `None` when no bound is known (see the module docs).
    pub(super) fn of(site: &Site, symbol: &str) -> Option<Self> {
        let path = Path::new(&site.path);
        if path.extension().is_none_or(|ext| ext != "rs") {
            return None;
        }
        let bytes =
            crate::tools::source::read_bounded(path, octocode_engine::signatures::MAX_PARSE_SIZE)
                .ok()?;
        let content = super::super::source::admitted_text(&bytes).ok()?;
        let document = crate::tools::declarations_cache::file_document(&content, &site.path)?;
        let declared = rust::declared(&document, &content, (site.line, site.character), symbol)?;
        let package = cargo::package_of(path)?;
        let name = cargo::package_name(&package)?;
        let roots = cargo::target_roots(&package);
        Some(Self {
            bound: declared.bound(path, &package, &roots),
            package,
            name,
            written: declared.written,
            relocated: std::sync::OnceLock::new(),
            dependents: std::sync::Mutex::new(std::collections::HashMap::new()),
        })
    }

    /// Whether `file` (canonical) could reference the declaration. A walk
    /// `stopped` before it decided keeps the file in reach.
    pub(super) fn reaches(&self, file: &str, stopped: &dyn Fn() -> bool) -> bool {
        let file = Path::new(file);
        let Some(package) = cargo::package_of(file) else {
            // No Cargo package compiles it: nothing bounds what it names.
            return true;
        };
        match &self.bound {
            rust::Bound::Territory(territory) => {
                package == self.package
                    && (rust::in_territory(file, territory)
                        || self.relocated(file, territory, stopped))
            }
            rust::Bound::Crate => package == self.package,
            rust::Bound::Public => package == self.package || self.depends(&package),
        }
    }

    /// Whether `file` is (or is a child module of) a module that a file of
    /// `territory`, or a module already placed in it, declares by `#[path]`:
    /// a module belongs to the module that declares it, wherever its file
    /// sits (a build script's `#[path]` module stays the build script's).
    fn relocated(&self, file: &Path, territory: &Path, stopped: &dyn Fn() -> bool) -> bool {
        let targets = match self.relocated.get() {
            Some(targets) => targets,
            None => match cargo::relocated_modules(&self.package, stopped) {
                Some(found) => self.relocated.get_or_init(|| placed_in(territory, found)),
                None => return true,
            },
        };
        targets.iter().any(|target| rust::in_module(file, target))
    }

    fn depends(&self, package: &Path) -> bool {
        let mut known = self
            .dependents
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *known
            .entry(package.to_path_buf())
            .or_insert_with(|| cargo::depends_on(package, &self.name, &self.package))
    }

    /// The bound as a reader checks it, paths relative to `root`.
    pub(super) fn describe(&self, root: &str) -> String {
        let shown = |path: &Path| {
            path.strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned()
        };
        match &self.bound {
            rust::Bound::Territory(territory) => format!(
                "{} reaches only module {} (its directory and file) of crate {}",
                self.written,
                shown(territory),
                self.name
            ),
            rust::Bound::Crate => format!("{} reaches only crate {}", self.written, self.name),
            rust::Bound::Public => format!(
                "{} reaches only crate {} and the crates that depend on it",
                self.written, self.name
            ),
        }
    }
}
