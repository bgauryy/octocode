//! Which Rust file a `mod` mounts, as the compiler maps it, and which files may
//! declare the `mod` that mounts a given file. A parent is found going up from
//! the file: a module root of each directory up to its crate may reach it by
//! name through inline modules, and any Rust file in those directories by
//! `#[path]`. A `#[path]` reaching sideways through `..`, from a directory that
//! does not hold the file, is not found from the file's side: a file no module
//! root names asks the files of its crate that spell a `#[path]`, which only such
//! an orphan pays for, once per crate. One reaching in from another crate or from
//! a directory below a test directory stays unfound.
use super::rust_source::{
    candidate_dir, first_gated_byte, may_declare_module, module_name, spells_path_attribute,
    word_at,
};
use super::{TestCodeIndex, in_spans, lock};
use octocode_engine::graph::GraphFactRustModule;
use octocode_engine::text::lexical;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

/// A Rust file its modules' mounts are asked of.
pub(super) struct Parent {
    /// Its text, when it could be read.
    content: Option<String>,
    /// The byte from which it could name a module it mounts only under test.
    gate: Option<usize>,
    /// Its text spells a `#[path]`, so a file's own name can be what it mounts
    /// and a module can lie anywhere below it.
    renames: bool,
    /// The files it mounts, each with whether only under test, once a mount
    /// asked.
    mounts: OnceLock<Vec<(PathBuf, bool)>>,
}

impl Parent {
    fn of(content: Option<String>) -> Self {
        let text = content.as_deref().unwrap_or_default();
        Self {
            gate: first_gated_byte(text),
            renames: spells_path_attribute(text.as_bytes()),
            content,
            mounts: OnceLock::new(),
        }
    }

    /// Whether its text at or after `from` leaves the parse anything to find
    /// about the file `path`, whose module is `name`: a `mod` under a gate names
    /// it no earlier than the gate, while a `#[path]` can precede the gate on
    /// the same item.
    fn may_mount(&self, path: &Path, name: &str, from: usize) -> bool {
        self.content.as_deref().is_some_and(|content| {
            may_declare_module(content, name, from)
                || (self.renames
                    && path
                        .file_name()
                        .and_then(std::ffi::OsStr::to_str)
                        .is_some_and(|file| word_at(content, file, 0).is_some()))
        })
    }

    /// Whether a module named in its text can lie in `dir`: by name, the
    /// directories between it and `dir` are inline modules it opens, the first
    /// of them possibly its own stem. Only a `#[path]` reaches further.
    fn may_reach(&self, parent: &Path, dir: &Path) -> bool {
        let content = self.content.as_deref().unwrap_or_default();
        self.renames
            || parent
                .parent()
                .and_then(|top| dir.strip_prefix(top).ok())
                .is_none_or(|below| {
                    below.components().all(|part| {
                        let part = part.as_os_str();
                        parent.file_stem() == Some(part)
                            || part
                                .to_str()
                                .is_none_or(|name| may_declare_module(content, name, 0))
                    })
                })
    }
}

/// One verdict's way up its parents.
#[derive(Default)]
struct MountWalk {
    /// Only module roots are asked, not every file that spells a `#[path]`.
    by_name: bool,
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
        let base = relative.map_or(dir.clone(), |stem| dir.join(stem));
        let name = bare_name(&module.name);
        mount(base.join(format!("{name}.rs")), false);
        mount(base.join(name).join("mod.rs"), false);
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
        let top = manifest.is_file() || self.paths.workspace_relative(dir).as_deref() == Some(".");
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
                .filter(|candidate| candidate.is_file())
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

    /// Whether the Rust file `path` is a module a parent mounts only under
    /// test: the parent's `mod` lies in a `#[cfg(test)]` item, or the parent is
    /// itself test code. A `#[path]` can make files mount each other, so a walk
    /// keeps what it is inside of and leaves the verdict of what it cut short
    /// unsettled.
    pub(super) fn mounted_under_test(&self, path: &Path) -> bool {
        Self::may_hold_test_code(path) && self.mount_verdict(path, &mut MountWalk::default())
    }

    /// [`Self::mounted_under_test`] asking only module roots, so without
    /// reading the other files of the directories above: a `true` holds there
    /// too, a `false` may not.
    pub(super) fn mounted_by_name(&self, path: &Path) -> bool {
        let mut walk = MountWalk {
            by_name: true,
            ..MountWalk::default()
        };
        Self::may_hold_test_code(path) && self.mount_verdict(path, &mut walk)
    }

    fn mount_verdict(&self, path: &Path, walk: &mut MountWalk) -> bool {
        let verdicts = if walk.by_name {
            &self.named
        } else {
            &self.mounted
        };
        if let Some(known) = verdicts.known(path) {
            return known;
        }
        if let Some(under_test) = self.crate_target(path) {
            return under_test;
        }
        if walk.inside.iter().any(|inside| inside == path) {
            walk.cut = true;
            return false;
        }
        let (Some(name), Some(dir), Some(own)) =
            (module_name(path), candidate_dir(path), path.parent())
        else {
            return false;
        };
        walk.inside.push(path.to_path_buf());
        let cut_before = std::mem::take(&mut walk.cut);
        let declarers = if walk.by_name {
            Arc::default()
        } else {
            self.path_declarers(own)
        };
        // Whether a near parent's text could be what mounts it: then it is no
        // orphan, and no other file of its crate need be read about it.
        let mut spelled = false;
        let verdict = [self.module_roots(dir), declarers]
            .iter()
            .flat_map(|parents| parents.iter())
            .filter(|parent| parent.as_path() != path)
            .any(|parent| {
                let known = self.parent(parent);
                if !known.may_reach(parent, dir) || !known.may_mount(path, &name, 0) {
                    return false;
                }
                spelled = true;
                self.mounts_under_test(parent, &known, (path, &name), walk)
            })
            || (!walk.by_name && !spelled && self.only_test_reaches((path, dir), &name, own, walk));
        walk.inside.pop();
        if verdict || !walk.cut || walk.inside.is_empty() {
            verdicts.settle(path, verdict);
        }
        walk.cut |= cut_before;
        verdict
    }

    /// Whether a `#[path]` anywhere in the crate mounts the file `path`, and each
    /// that does is test code: one a plain build compiles makes it production.
    fn only_test_reaches(
        &self,
        (path, dir): (&Path, &Path),
        name: &str,
        own: &Path,
        walk: &mut MountWalk,
    ) -> bool {
        let target = lexical(path);
        let mut under_test = false;
        for parent in self.path_declarers_of_crate(own).iter() {
            if parent.as_path() == path {
                continue;
            }
            let known = self.parent(parent);
            if !known.may_reach(parent, dir) || !known.may_mount(path, name, 0) {
                continue;
            }
            if self.mounts_under_test(parent, &known, (path, name), walk) {
                under_test = true;
            } else if self
                .parsed_mounts(parent, &known)
                .iter()
                .any(|(mounted, _)| *mounted == target)
            {
                return false;
            }
        }
        under_test
    }

    fn parent(&self, path: &Path) -> Arc<Parent> {
        self.parents
            .of(path, || Arc::new(Parent::of(self.source(path))))
    }

    /// Whether the Rust file `parent`, whose text could mount the file `path` with
    /// the module `name`, mounts it only under test: the parse decides, once per
    /// parent, not once per child.
    fn mounts_under_test(
        &self,
        parent: &Path,
        known: &Parent,
        (path, name): (&Path, &str),
        walk: &mut MountWalk,
    ) -> bool {
        let target = lexical(path);
        let names = |only_gated: bool| {
            self.parsed_mounts(parent, known)
                .iter()
                .any(|(mounted, gated)| *mounted == target && (*gated || !only_gated))
        };
        known
            .gate
            .is_some_and(|from| known.may_mount(path, name, from))
            && names(true)
            || ((self.by_spelling(parent) || self.mount_verdict(parent, walk)) && names(false))
    }

    /// The files `path` mounts by its parse, each with whether a `#[cfg(test)]`
    /// item holds it.
    fn parsed_mounts<'p>(&self, path: &Path, parent: &'p Parent) -> &'p [(PathBuf, bool)] {
        parent.mounts.get_or_init(|| {
            #[cfg(test)]
            self.parents_parsed
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let parsed = self.parsed(path, parent.content.as_deref().unwrap_or_default());
            parsed
                .modules
                .iter()
                .map(|mount| {
                    (
                        mount.path.clone(),
                        mount.under_test || in_spans(&parsed.test_lines, mount.line),
                    )
                })
                .collect()
        })
    }
}
