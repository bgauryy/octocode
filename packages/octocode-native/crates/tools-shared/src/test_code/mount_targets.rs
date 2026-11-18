//! Which Rust file a `mod` mounts, as the compiler maps it, and which files may
//! declare the `mod` that mounts a given file. A parent is found going up from
//! the file: a module root of each directory up to its crate may reach it by
//! name through inline modules, and any Rust file in those directories by
//! `#[path]`. A `#[path]` reaching sideways through `..`, from a directory that
//! does not hold the file, is not found from the file's side: a file no module
//! root names asks the files of its crate that spell a `#[path]`, which only such
//! an orphan pays for, once per crate. One reaching in from another crate or from
//! a directory below a test directory stays unfound.
use super::module_heads::ModuleHeads;
use super::rust_source::{candidate_dir, first_gated_byte, module_name, spells_path_attribute};
use super::{TestCodeIndex, in_spans, lock};
use octocode_engine::graph::GraphFactRustModule;
use octocode_engine::text::lexical;
use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

/// The files a parent mounts, by whether a `#[cfg(test)]` item holds the mount:
/// a child asks by its path, so each ask is a lookup, not a pass over the mounts.
#[derive(Default)]
struct Mounts {
    gated: HashSet<PathBuf>,
    plain: HashSet<PathBuf>,
}

impl Mounts {
    /// Whether `target` is mounted, by a mount held under test (`Some(true)`),
    /// not held under test (`Some(false)`), or either (`None`).
    fn holds(&self, target: &Path, under_test: Option<bool>) -> bool {
        let gated = || self.gated.contains(target);
        let plain = || self.plain.contains(target);
        match under_test {
            Some(true) => gated(),
            Some(false) => plain(),
            None => gated() || plain(),
        }
    }
}

/// A Rust file its modules' mounts are asked of.
pub(super) struct Parent {
    /// Its text, when it could be read.
    content: Option<Arc<str>>,
    /// The byte from which it could name a module it mounts only under test.
    gate: Option<usize>,
    /// Its text spells a `#[path]`, so a file's own name can be what it mounts
    /// and a module can lie anywhere below it.
    renames: bool,
    /// The files it mounts, each with whether only under test, once a mount
    /// asked.
    mounts: OnceLock<Mounts>,
    /// The modules and files its text spells, once a child asked: the text is
    /// scanned for them once, not once per child.
    heads: OnceLock<ModuleHeads>,
    /// Texts scanned for [`Self::heads`], shared with the index that made it.
    #[cfg(test)]
    scans: Arc<std::sync::atomic::AtomicUsize>,
}

impl Parent {
    fn of(
        content: Option<Arc<str>>,
        #[cfg(test)] scans: Arc<std::sync::atomic::AtomicUsize>,
    ) -> Self {
        let text = content.as_deref().unwrap_or_default();
        Self {
            gate: first_gated_byte(text),
            renames: spells_path_attribute(text.as_bytes()),
            content,
            mounts: OnceLock::new(),
            heads: OnceLock::new(),
            #[cfg(test)]
            scans,
        }
    }

    fn heads(&self) -> &ModuleHeads {
        self.heads.get_or_init(|| {
            #[cfg(test)]
            self.scans
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            ModuleHeads::of(self.content.as_deref().unwrap_or_default())
        })
    }

    /// Whether its text at or after `from` could spell the head of the module
    /// `name` ([`super::rust_source::may_declare_module`]).
    fn may_declare(&self, name: &str, from: usize) -> bool {
        self.content
            .as_deref()
            .is_some_and(|content| self.heads().declares_module(content, name, from))
    }

    /// Whether its text at or after `from` leaves the parse anything to find
    /// about the file `path`, whose module is `name`: a `mod` under a gate names
    /// it no earlier than the gate, while a `#[path]` can precede the gate on
    /// the same item.
    fn may_mount(&self, path: &Path, name: &str, from: usize) -> bool {
        self.may_declare(name, from)
            || (self.renames
                && self.content.as_deref().is_some_and(|content| {
                    path.file_name()
                        .and_then(std::ffi::OsStr::to_str)
                        .is_some_and(|file| self.heads().names_file(content, file))
                }))
    }

    /// Whether a module named in its text can lie in `dir`: by name, the
    /// directories between it and `dir` are inline modules it opens, the first
    /// of them possibly its own stem. Only a `#[path]` reaches further.
    fn may_reach(&self, parent: &Path, dir: &Path) -> bool {
        self.renames
            || parent
                .parent()
                .and_then(|top| dir.strip_prefix(top).ok())
                .is_none_or(|below| {
                    below.components().all(|part| {
                        let part = part.as_os_str();
                        parent.file_stem() == Some(part)
                            || part.to_str().is_none_or(|name| self.may_declare(name, 0))
                    })
                })
    }
}

/// Where the compiler places a Rust file: a module root or a mounted module
/// is test or production code by what mounts it, whatever it is called; a file
/// nothing is found to mount is unplaced, and only its spelling can say. A file
/// not spelled as test code is production unless placed in test code, so only
/// a file spelled as test code is asked for `Production`: the mounts that
/// place the rest in production are never parsed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Placement {
    Test,
    Production,
    Unplaced,
}

impl Placement {
    fn of(under_test: bool) -> Self {
        if under_test {
            Self::Test
        } else {
            Self::Production
        }
    }
}

/// One verdict's way up its parents.
#[derive(Default)]
struct MountWalk {
    /// Only module roots are asked, not every file that spells a `#[path]`.
    by_name: bool,
    /// Every file asked about is also looked for in production mounts.
    exact: bool,
    inside: Vec<PathBuf>,
    /// A parent already inside was left out of a verdict below it.
    cut: bool,
}

/// A file a `mod` may mount.
#[derive(Clone)]
pub(super) struct Mount {
    pub(super) path: PathBuf,
    pub(super) line: u32,
    /// Only a `cfg_attr` under test names it, whatever its item's `cfg` says.
    pub(super) under_test: bool,
}

/// The file modules `file` declares, each with the files it may mount and its
/// line. `crate_root`: its modules lie beside it whatever it is called.
pub(super) fn file_modules(
    file: &Path,
    modules: &[GraphFactRustModule],
    crate_root: bool,
) -> Vec<Mount> {
    let mut mounted = Vec::new();
    for module in modules.iter().filter(|module| !module.inline) {
        let (dir, relative) = inline_dir(file, module, modules, crate_root);
        let mut mount = |path: PathBuf, under_test: bool| {
            mounted.push(Mount {
                path: lexical(&path),
                line: module.line,
                under_test,
            });
        };
        if let Some(path) = &module.test_path {
            mount(dir.join(path), true);
        }
        if let Some(path) = &module.path {
            mount(dir.join(path), false);
            continue;
        }
        // A `cfg_attr` giving the path only outside test leaves the module its
        // own name under test.
        if let Some(path) = &module.plain_path {
            mount(dir.join(path), false);
        }
        let own_name_under_test = module.plain_path.is_some();
        let base = relative.map_or(dir.clone(), |stem| dir.join(stem));
        let name = bare_name(&module.name);
        mount(base.join(format!("{name}.rs")), own_name_under_test);
        mount(base.join(name).join("mod.rs"), own_name_under_test);
    }
    mounted
}

fn bare_name(name: &str) -> &str {
    name.strip_prefix("r#").unwrap_or(name)
}

/// The directory paths in `module`'s inline scope are relative to, and the
/// `file` stem a name below it first goes under: a named file's modules lie in
/// a directory of its name, and a `#[path]` on an inline module replaces both.
fn inline_dir(
    file: &Path,
    module: &GraphFactRustModule,
    modules: &[GraphFactRustModule],
    crate_root: bool,
) -> (PathBuf, Option<OsString>) {
    let mut dir = file.parent().map_or_else(PathBuf::new, Path::to_path_buf);
    let is_root = crate_root
        || file
            .file_name()
            .is_some_and(|name| name == "mod.rs" || name == "lib.rs" || name == "main.rs");
    let mut relative = if is_root {
        None
    } else {
        file.file_stem().map(std::ffi::OsStr::to_os_string)
    };
    for (depth, name) in module.scope.iter().enumerate() {
        let holder = modules.iter().rev().find(|held| {
            held.inline
                && held.line <= module.line
                && held.name == *name
                && held.scope == module.scope[..depth]
        });
        if let Some(path) = holder.and_then(|holder| holder.path.as_ref()) {
            dir.push(path);
            relative = None;
        } else {
            if let Some(stem) = relative.take() {
                dir.push(stem);
            }
            dir.push(bare_name(name));
        }
    }
    (dir, relative)
}

impl TestCodeIndex {
    /// The directory above `dir` a parent may still lie in; `None` at its
    /// crate's top or the workspace root.
    pub(super) fn level_above<'p>(&self, dir: &'p Path) -> Option<&'p Path> {
        let manifest = dir.join("Cargo.toml");
        let top =
            self.is_file(&manifest) || self.paths.workspace_relative(dir).as_deref() == Some(".");
        lock(&self.read).insert(manifest);
        if top { None } else { dir.parent() }
    }

    /// The module roots that may declare a file below `dir` by name: at each
    /// level, its `mod.rs`, `lib.rs`, `main.rs` and sibling `<dir>.rs`.
    pub(super) fn module_roots(&self, dir: &Path) -> Arc<[PathBuf]> {
        self.candidates.of(dir, || {
            let named = super::rust_source::candidates_in(dir);
            lock(&self.read).extend(named.iter().cloned());
            let mut roots = named
                .into_iter()
                .filter(|candidate| self.is_file(candidate))
                .collect::<Vec<_>>();
            for root in self.crate_roots_in(dir) {
                if !roots.contains(&root) {
                    roots.push(root);
                }
            }
            if let Some(up) = self.level_above(dir) {
                roots.extend(self.module_roots(up).iter().cloned());
            }
            roots.into()
        })
    }

    /// Where the Rust file `path` is placed: test code when a parent's `mod`
    /// lies in a `#[cfg(test)]` item or the parent is itself test code,
    /// production when a parent mounts it otherwise. A `#[path]` can make
    /// files mount each other, so a walk keeps what it is inside of and leaves
    /// the verdict of what it cut short unsettled.
    pub(super) fn placement(&self, path: &Path) -> Placement {
        if !Self::may_hold_test_code(path) {
            return Placement::Unplaced;
        }
        self.mount_verdict(path, &mut MountWalk::default())
    }

    /// [`Self::placement`] asking only module roots, so without reading the
    /// other files of the directories above: a placement found holds there
    /// too, an unplaced file may not be.
    pub(super) fn placement_by_name(&self, path: &Path) -> Placement {
        if !Self::may_hold_test_code(path) {
            return Placement::Unplaced;
        }
        let mut walk = MountWalk {
            by_name: true,
            ..MountWalk::default()
        };
        self.mount_verdict(path, &mut walk)
    }

    fn mount_verdict(&self, path: &Path, walk: &mut MountWalk) -> Placement {
        // A file spelled as test code is asked for production mounts too, and
        // so are the parents that place it: each cache holds verdicts of one
        // kind only.
        let exact = walk.exact || self.by_spelling(path);
        let verdicts = match (walk.by_name, exact) {
            (true, false) => &self.named,
            (false, false) => &self.mounted,
            (true, true) => &self.placed_named,
            (false, true) => &self.placed,
        };
        if let Some(known) = verdicts.known(path) {
            return known;
        }
        if let Some(under_test) = self.crate_target(path) {
            return Placement::of(under_test);
        }
        if walk.inside.iter().any(|inside| inside == path) {
            walk.cut = true;
            return Placement::Unplaced;
        }
        let (Some(name), Some(dir), Some(own)) =
            (module_name(path), candidate_dir(path), path.parent())
        else {
            return Placement::Unplaced;
        };
        walk.inside.push(path.to_path_buf());
        let cut_before = std::mem::take(&mut walk.cut);
        let declarers = if walk.by_name {
            Arc::default()
        } else {
            self.path_declarers(own)
        };
        let parents = [self.module_roots(dir), declarers];
        let found = parents
            .iter()
            .flat_map(|parents| parents.iter())
            .filter(|parent| parent.as_path() != path)
            .filter(|parent| {
                let known = self.parent(parent);
                known.may_reach(parent, dir) && known.may_mount(path, &name, 0)
            })
            .collect::<Vec<_>>();
        // Whether a near parent's text could be what mounts it: then it is no
        // orphan, and no other file of its crate need be read about it.
        let spelled = !found.is_empty();
        let outer = std::mem::replace(&mut walk.exact, exact);
        let mut verdict = self.placed_by(&found, (path, &name), exact, walk);
        walk.exact = outer;
        if verdict == Placement::Test && !exact {
            // A test mount found: a mount in production elsewhere still wins.
            let outer = std::mem::replace(&mut walk.exact, true);
            verdict = self.placed_by(&found, (path, &name), true, walk);
            walk.exact = outer;
        }
        if verdict == Placement::Unplaced && !walk.by_name && !spelled {
            verdict = self.only_test_reaches((path, dir), &name, own, walk);
        }
        walk.inside.pop();
        // A production mount stands whatever a cut parent might add; a test or
        // no mount may still change once the parents being decided are.
        if verdict == Placement::Production || !walk.cut || walk.inside.is_empty() {
            verdicts.settle(path, verdict);
        }
        walk.cut |= cut_before;
        verdict
    }

    /// Where a `#[path]` anywhere in the crate places the file `path`: test
    /// code when each that mounts it is, production when one a plain build
    /// compiles does, unplaced when none does.
    fn only_test_reaches(
        &self,
        (path, dir): (&Path, &Path),
        name: &str,
        own: &Path,
        walk: &mut MountWalk,
    ) -> Placement {
        let target = lexical(path);
        let mut placement = Placement::Unplaced;
        for parent in self.path_declarers_of_crate(own).iter() {
            if parent.as_path() == path {
                continue;
            }
            let known = self.parent(parent);
            if !known.may_reach(parent, dir) || !known.may_mount(path, name, 0) {
                continue;
            }
            match self.mounts_under_test(parent, &known, (path, name), (false, walk)) {
                Placement::Test => placement = Placement::Test,
                Placement::Production => return Placement::Production,
                Placement::Unplaced => {
                    if self.parsed_mounts(parent, &known).holds(&target, None) {
                        return Placement::Production;
                    }
                }
            }
        }
        placement
    }

    fn parent(&self, path: &Path) -> Arc<Parent> {
        self.parents.of(path, || {
            Arc::new(Parent::of(
                self.source(path),
                #[cfg(test)]
                Arc::clone(&self.heads_scanned),
            ))
        })
    }

    /// Where the `parents` that may mount the file `path` place it together:
    /// production when any does, test code when every mount found is, else
    /// unplaced. Parents are only those already found: no directory is read.
    fn placed_by(
        &self,
        parents: &[&PathBuf],
        (path, name): (&Path, &str),
        exact: bool,
        walk: &mut MountWalk,
    ) -> Placement {
        let mut verdict = Placement::Unplaced;
        for parent in parents {
            let known = self.parent(parent);
            match self.mounts_under_test(parent, &known, (path, name), (exact, &mut *walk)) {
                Placement::Production => return Placement::Production,
                Placement::Test => verdict = Placement::Test,
                Placement::Unplaced => {}
            }
        }
        verdict
    }

    /// How the Rust file `parent`, whose text could mount the file `path`
    /// with the module `name`, places it: test code when each mount of it is
    /// under test or in test code, production when one is not, unplaced when
    /// its parse mounts it not at all or it is still being decided. The parse
    /// decides, once per parent, not once per child. Without `exact` a
    /// mount that is not test code is left `Unplaced` and not looked for.
    fn mounts_under_test(
        &self,
        parent: &Path,
        known: &Parent,
        (path, name): (&Path, &str),
        (exact, walk): (bool, &mut MountWalk),
    ) -> Placement {
        let target = lexical(path);
        let mounts =
            |under_test: Option<bool>| self.parsed_mounts(parent, known).holds(&target, under_test);
        let gated = known
            .gate
            .is_some_and(|from| known.may_mount(path, name, from))
            && mounts(Some(true));
        if !exact {
            return if gated || (self.is_test_parent(parent, walk) == Some(true) && mounts(None)) {
                Placement::Test
            } else {
                Placement::Unplaced
            };
        }
        let plain = mounts(Some(false));
        let by_plain = if plain {
            match self.is_test_parent(parent, walk) {
                Some(false) => return Placement::Production,
                parent_test => parent_test == Some(true),
            }
        } else {
            false
        };
        if gated || by_plain {
            Placement::Test
        } else {
            Placement::Unplaced
        }
    }

    /// Whether the Rust file `parent` is test code: by where it is placed,
    /// else by its spelling. `None` when it is no evidence: its own placement
    /// is still being decided up the walk (a cut parent), or nothing mounts
    /// it, so a mount of it is not compiled.
    fn is_test_parent(&self, parent: &Path, walk: &mut MountWalk) -> Option<bool> {
        let before = std::mem::take(&mut walk.cut);
        let verdict = self.mount_verdict(parent, walk);
        let cut = walk.cut;
        walk.cut |= before;
        match verdict {
            Placement::Test => Some(true),
            Placement::Production => Some(false),
            Placement::Unplaced if cut => None,
            // A crate root nothing mounts is still compiled; any other file
            // nothing mounts is not, so its own mounts are no evidence of
            // production (only its spelling can still make them test).
            Placement::Unplaced if module_name(parent).is_none() => Some(self.by_spelling(parent)),
            Placement::Unplaced => self.by_spelling(parent).then_some(true),
        }
    }

    /// The files `path` mounts by its parse, each with whether a `#[cfg(test)]`
    /// item holds it.
    fn parsed_mounts<'p>(&self, path: &Path, parent: &'p Parent) -> &'p Mounts {
        parent.mounts.get_or_init(|| {
            #[cfg(test)]
            self.parents_parsed
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let parsed = self.mount_facts(path, parent.content.as_deref().unwrap_or_default());
            let mut mounts = Mounts::default();
            for mount in &parsed.modules {
                let under_test = mount.under_test || in_spans(&parsed.test_lines, mount.line);
                let kept = if under_test {
                    &mut mounts.gated
                } else {
                    &mut mounts.plain
                };
                kept.insert(mount.path.clone());
            }
            mounts
        })
    }
}
