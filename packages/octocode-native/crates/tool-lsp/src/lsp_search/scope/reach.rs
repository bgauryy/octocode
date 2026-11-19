//! Which text-scan files could reference the anchor's declaration at all.
//!
//! A file that spells the name but lies outside the declaration's reach is
//! no unverified site: it is out of reach, counted apart
//! (`coverage.outOfReachFiles`), and it neither makes a row partial nor
//! becomes a lead. Rust bounds reach by the item's written visibility and
//! its inherent container's (a method of a `pub(super)` type), and by crate:
//! the defining package, or one that depends on it ([`rust`]); a file no
//! Cargo package compiles (a scratch copy) joins the defining crate only
//! where it places that file by `#[path]` or `include!`. A Rust reach
//! bounded to a module, a crate or a binding also bounds the text check
//! itself ([`Reach::searched`]): it reads only where the declaration
//! reaches, with what the crate places outside its directory, and every
//! other file of the scope counts as out of reach unread. TS/JS reach
//! is the importer pre-filter (`importers::reach`): a file whose imports
//! name only other modules is set aside as checked there. Other languages,
//! an anchor that does not start a declaration, trait items, macros and
//! every undecidable file keep every file in reach.
use octocode_engine::num::saturating_u32;
use std::path::{Path, PathBuf};

mod cargo;
mod rust;
pub(in super::super) use cargo::{depends_on, local_packages, package_name, package_of};

/// The module files of `relocated` (declaring file, module file) that
/// belong to `territory`: declared from one of its files, or from a module
/// already placed in it.
fn placed_in(territory: &Path, relocated: &[cargo::Relocated]) -> Vec<PathBuf> {
    let mut placed: Vec<PathBuf> = Vec::new();
    let mut pending = relocated.to_vec();
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

/// The reach of the declaration the anchor names.
pub(super) enum Reach {
    /// A local binding: only `lines` (zero-based, inclusive) of `file`
    /// (canonical) can name it.
    Local {
        file: String,
        lines: std::ops::RangeInclusive<u32>,
    },
    /// An item, bounded by its written visibility.
    Item(ItemReach),
}

impl Reach {
    /// The reach of what the anchor `site` names by `symbol`: a local
    /// binding's scope, else the declaration it starts; `None` when no
    /// bound is known (see the module docs).
    pub(super) fn of(site: &Site, symbol: &str) -> Option<Self> {
        let path = Path::new(&site.path);
        let bytes = octocode_tools_shared::source::read_bounded(
            path,
            octocode_runtime_base::security::SourceLimits::REAL.whole_source,
        )
        .ok()?;
        let content = super::super::source::admitted_text(&bytes).ok()?;
        if let Some((first, last)) = octocode_engine::lsp::resolver::local_binding_lines(
            &site.path,
            &content,
            symbol,
            site.line,
            site.character,
        ) {
            let line = |line| saturating_u32(line);
            return Some(Self::Local {
                file: site.path.clone(),
                lines: line(first)..=line(last),
            });
        }
        ItemReach::of(site, symbol, &content).map(Self::Item)
    }

    /// Whether `file` (canonical) could reference it. A walk `stopped`
    /// before it decided keeps the file in reach.
    pub(super) fn reaches(&self, file: &str, stopped: &dyn Fn() -> bool) -> bool {
        match self {
            Self::Local { file: own, .. } => own == file,
            Self::Item(item) => item.reaches(file, stopped),
        }
    }

    /// Whether zero-based `line` of `file` (canonical) could reference it.
    pub(super) fn reaches_line(&self, file: &str, line: u32) -> bool {
        match self {
            Self::Local { file: own, lines } => own == file && lines.contains(&line),
            Self::Item(item) => item.reaches(file, &|| false),
        }
    }

    /// The one path every file in reach sits under: a local binding's file,
    /// a module's directory, the defining crate's directory. `None` when the
    /// reach spans crates and no path bounds it.
    pub(super) fn directory(&self) -> Option<PathBuf> {
        match self {
            Self::Local { file, .. } => Some(PathBuf::from(file)),
            Self::Item(item) => item.directory(),
        }
    }

    /// Where a text check of the name needs to look ([`Searched`]): every
    /// file this reach admits lies there, so a file of the scope outside it
    /// is out of reach unread. `None` when the reach spans crates, the anchor
    /// is no Rust file, or what a crate places outside its directory could
    /// not be read (`stopped`): then the whole scope is searched.
    pub(super) fn searched(&self, stopped: &dyn Fn() -> bool) -> Option<Searched> {
        match self {
            Self::Local { file, .. } => Path::new(file)
                .extension()
                .is_some_and(|ext| ext == "rs")
                .then(|| Searched {
                    under: Vec::new(),
                    files: vec![PathBuf::from(file)],
                    modules: Vec::new(),
                }),
            Self::Item(item) => item.searched(stopped),
        }
    }

    /// The bound as a reader checks it, paths relative to `root`.
    pub(super) fn describe(&self, root: &str) -> String {
        match self {
            Self::Local { file, lines } => format!(
                "a local binding reaches only lines {}-{} of {}",
                lines.start() + 1,
                lines.end() + 1,
                Path::new(file)
                    .strip_prefix(root)
                    .unwrap_or(Path::new(file))
                    .to_string_lossy()
            ),
            Self::Item(item) => item.describe(root),
        }
    }
}

/// Where every file a bounded reach admits lies ([`Reach::searched`]): under
/// one of `under`, one of `files`, or one of `modules` (a module file placed
/// by `#[path]`) or a child module of it. It holds whatever [`Reach::reaches`]
/// admits, and more (a nested package under the directory, say), so the
/// files a text check skips by it are out of reach, never a missed site.
#[derive(Debug)]
pub(super) struct Searched {
    under: Vec<PathBuf>,
    files: Vec<PathBuf>,
    modules: Vec<PathBuf>,
}

impl Searched {
    /// Whether a text check reads `file` (canonical).
    pub(super) fn holds(&self, file: &Path) -> bool {
        self.under.iter().any(|dir| file.starts_with(dir))
            || self.files.iter().any(|own| own == file)
            || self
                .modules
                .iter()
                .any(|module| rust::in_module(file, module))
    }
}

/// The reach of the item declaration the anchor starts.
pub(super) struct ItemReach {
    bound: rust::Bound,
    /// The defining package (canonical manifest directory) and its name.
    package: PathBuf,
    name: String,
    /// What the written visibility reads, as `pub(super)`.
    written: String,
    /// The modules of the territory placed by `#[path]`, which may sit
    /// outside its directory; read once, on the first file outside it.
    relocated: std::sync::OnceLock<Vec<PathBuf>>,
    /// The files outside its directory the package `include!`s (canonical);
    /// read once, on the first file no Cargo package compiles.
    included: std::sync::OnceLock<Vec<PathBuf>>,
    /// Whether a package (its manifest directory) depends on the defining
    /// one, directly or through local path dependencies.
    dependents: std::sync::Mutex<std::collections::HashMap<PathBuf, bool>>,
}

impl ItemReach {
    /// The reach of the declaration of `symbol` that the anchor `site`
    /// starts, `content` its file's text; `None` when no bound is known
    /// (see the module docs).
    fn of(site: &Site, symbol: &str, content: &str) -> Option<Self> {
        let path = Path::new(&site.path);
        if path.extension().is_none_or(|ext| ext != "rs") {
            return None;
        }
        let document = octocode_tools_shared::declarations::file_document(content, &site.path)?;
        let declared = rust::declared(&document, content, (site.line, site.character), symbol)?;
        let package = cargo::package_of(path)?;
        let name = cargo::package_name(&package)?;
        let roots = cargo::target_roots(&package);
        Some(Self {
            bound: declared.bound(path, &package, &roots),
            package,
            name,
            written: declared.written,
            relocated: std::sync::OnceLock::new(),
            included: std::sync::OnceLock::new(),
            dependents: std::sync::Mutex::new(std::collections::HashMap::new()),
        })
    }

    /// Whether `file` (canonical) could reference the declaration. A walk
    /// `stopped` before it decided keeps the file in reach.
    fn reaches(&self, file: &str, stopped: &dyn Fn() -> bool) -> bool {
        let file = Path::new(file);
        let Some(package) = cargo::package_of(file) else {
            // No Cargo package compiles it on its own: it joins the defining
            // crate only where that crate places it (`#[path]`, `include!`),
            // and a crate-bounded item reaches no other. A public item may be
            // named by whatever compiles it, which nothing here bounds.
            return match &self.bound {
                rust::Bound::Territory(territory) => self.placed(file, territory, stopped),
                rust::Bound::Crate => self.placed(file, &self.package, stopped),
                rust::Bound::Public => true,
            };
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
        self.relocated_targets(territory, stopped)
            .is_none_or(|targets| targets.iter().any(|target| rust::in_module(file, target)))
    }

    /// The module files a file of `territory`, or a module already placed in
    /// it, declares by `#[path]`; `None` when `stopped` ended the read.
    fn relocated_targets(
        &self,
        territory: &Path,
        stopped: &dyn Fn() -> bool,
    ) -> Option<&[PathBuf]> {
        if let Some(targets) = self.relocated.get() {
            return Some(targets);
        }
        let found = cargo::relocated_modules(&self.package, stopped)?;
        Some(self.relocated.get_or_init(|| placed_in(territory, &found)))
    }

    /// The files outside its directory the package `include!`s (canonical);
    /// `None` when the read stopped before it saw every source.
    fn included_targets(&self, stopped: &dyn Fn() -> bool) -> Option<&[PathBuf]> {
        if let Some(included) = self.included.get() {
            return Some(included);
        }
        let includes = octocode_engine::lsp::workspace::cargo_package_includes(
            &self.package.join("Cargo.toml"),
            stopped,
        )
        .filter(|includes| !includes.has_more)?;
        Some(self.included.get_or_init(|| {
            includes
                .outside
                .iter()
                .map(|target| std::fs::canonicalize(target).unwrap_or(target.clone()))
                .collect()
        }))
    }

    /// See [`Reach::searched`]: the territory's directory and file, or the
    /// crate's directory, with what the crate places outside it (its
    /// `#[path]` modules and the files it `include!`s); `None` for a public
    /// item or a read `stopped` cut.
    fn searched(&self, stopped: &dyn Fn() -> bool) -> Option<Searched> {
        let (under, mut files, territory) = match &self.bound {
            rust::Bound::Territory(territory) => (
                territory.clone(),
                vec![territory.with_extension("rs")],
                territory,
            ),
            rust::Bound::Crate => (self.package.clone(), Vec::new(), &self.package),
            rust::Bound::Public => return None,
        };
        let modules = self.relocated_targets(territory, stopped)?.to_vec();
        files.extend(self.included_targets(stopped)?.iter().cloned());
        Some(Searched {
            under: vec![under],
            files,
            modules,
        })
    }

    /// Whether `file`, outside every Cargo package, is source the defining
    /// package compiles into `territory`: a module it places there by
    /// `#[path]` ([`Self::relocated`]), or a file it `include!`s from outside
    /// its directory (placed wherever the include sits, so counted for any
    /// territory). A scan `stopped` before it decided keeps the file in reach.
    fn placed(&self, file: &Path, territory: &Path, stopped: &dyn Fn() -> bool) -> bool {
        if self.relocated(file, territory, stopped) {
            return true;
        }
        self.included_targets(stopped)
            .is_none_or(|included| included.iter().any(|target| target == file))
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

    /// See [`Reach::directory`].
    fn directory(&self) -> Option<PathBuf> {
        match &self.bound {
            rust::Bound::Territory(territory) => Some(territory.clone()),
            rust::Bound::Crate => Some(self.package.clone()),
            rust::Bound::Public => None,
        }
    }

    /// The bound as a reader checks it, paths relative to `root`.
    fn describe(&self, root: &str) -> String {
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
