//! The language-family, directory and Rust-module scope that bounds
//! graph-wide name links.
use super::*;
use crate::tools::ast_graph::paths::{join, parent};

/// Per-graph lookups [`super::Linker`] needs for scope-aware choices.
pub(crate) struct LinkScope<'a> {
    /// Root-relative path by file node id (`""` for other nodes).
    pub(super) file_paths: &'a [&'a str],
    /// [`link_family`] by file node id (`""` for other nodes).
    pub(super) families: Vec<&'a str>,
    /// Rust module name → files defining it (`m.rs`, `m/mod.rs`).
    pub(super) modules: HashMap<&'a str, Vec<&'a str>>,
    /// Every scanned Rust file, for module paths resolved segment by segment.
    pub(super) rust_files: HashSet<&'a str>,
    /// Workspace crate import name (`octocode_engine`) → crate directory.
    pub(super) crate_dirs: HashMap<String, String>,
}

impl LinkScope<'_> {
    /// The root-relative path of file node `id`.
    pub(super) fn path(&self, id: u32) -> Option<&str> {
        self.file_paths
            .get(id as usize)
            .copied()
            .filter(|path| !path.is_empty())
    }

    /// Whether file nodes `a` and `b` share a language family.
    pub(super) fn same_family(&self, a: u32, b: u32) -> bool {
        let family = |id: u32| self.families.get(id as usize).copied().unwrap_or("");
        family(a) == family(b)
    }

    /// The directory of the Rust crate that holds `path`: the deepest
    /// workspace crate directory containing it, else the directory above its
    /// nearest `src/` ancestor, else the snapshot root (`""`).
    pub(super) fn rust_crate_dir<'p>(&'p self, path: &'p str) -> &'p str {
        let inside = |dir: &str| dir.is_empty() || path.starts_with(&format!("{dir}/"));
        if let Some(dir) = self
            .crate_dirs
            .values()
            .filter(|dir| inside(dir))
            .max_by_key(|dir| dir.len())
        {
            return dir;
        }
        let mut dir = parent(path);
        while !dir.is_empty() {
            if dir == "src" || dir.ends_with("/src") {
                return parent(dir);
            }
            dir = parent(dir);
        }
        ""
    }

    /// Whether file `path` lies in the crate directory `crate_dir`.
    pub(super) fn in_crate(path: &str, crate_dir: &str) -> bool {
        crate_dir.is_empty()
            || path
                .strip_prefix(crate_dir)
                .is_some_and(|rest| rest.starts_with('/'))
    }

    /// The file a Rust module path (`crate::a::b`, `super::b`,
    /// `other_crate::b`) names: its written segments when they reach a
    /// scanned file, else its last segment, scoped to the caller's crate (or
    /// the named workspace crate) and settled by the longest shared directory
    /// prefix when several files share the module name.
    pub(super) fn rust_module_file(&self, qualifier: &str, caller_file: &str) -> Option<&str> {
        let segments = qualifier
            .split("::")
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        let (first, module) = (*segments.first()?, *segments.last()?);
        if segments.len() < 2 || module.starts_with(|c: char| c.is_ascii_uppercase()) {
            return None;
        }
        if let Some(written) = self.written_module_file(&segments, caller_file) {
            return Some(written);
        }
        let scope_dir = match first {
            "crate" | "self" | "super" => None,
            name => Some(self.crate_dirs.get(name)?.as_str()),
        };
        let shared = |path: &str| {
            path.split('/')
                .zip(caller_file.split('/'))
                .take_while(|(a, b)| a == b)
                .count()
        };
        let candidates = self
            .modules
            .get(module)?
            .iter()
            .copied()
            .filter(|path| {
                scope_dir.is_none_or(|dir| dir.is_empty() || path.starts_with(&format!("{dir}/")))
            })
            .collect::<Vec<_>>();
        let best = candidates.iter().map(|p| shared(p)).max()?;
        let mut top = candidates.iter().filter(|p| shared(p) == best);
        let first_match = *top.next()?;
        top.next().is_none().then_some(first_match)
    }

    /// `crate::a::b`, `self::b`, `super::super::b`: the module file the
    /// segments name from the caller's crate root (the nearest directory
    /// holding `lib.rs` or `main.rs`) or its own module directory. `None`
    /// when no scanned file matches (inline module, `#[path]`, a binary or
    /// test crate), so the caller falls back to the module name.
    fn written_module_file(&self, segments: &[&str], caller_file: &str) -> Option<&'_ str> {
        let (stem, name) = match caller_file.strip_suffix(".rs")?.rsplit_once('/') {
            Some((dir, name)) => (dir, name),
            None => ("", caller_file.strip_suffix(".rs")?),
        };
        let mut dirs = std::iter::successors(Some(stem), |dir| {
            (!dir.is_empty()).then(|| dir.rsplit_once('/').map_or("", |(parent, _)| parent))
        });
        let root = dirs.find(|dir| {
            ["lib.rs", "main.rs"]
                .iter()
                .any(|file| self.rust_files.contains(join(dir, file).as_str()))
        })?;
        let own = if matches!(name, "mod" | "lib" | "main") {
            stem.to_owned()
        } else {
            join(stem, name)
        };
        let mut base = match segments[0] {
            "crate" => root.to_owned(),
            "self" | "super" => own,
            _ => return None,
        };
        let mut rest = &segments[1..];
        if segments[0] == "super" {
            base = parent(&base).to_owned();
            while let Some(("super", tail)) = rest.split_first().map(|(h, t)| (*h, t)) {
                base = parent(&base).to_owned();
                rest = tail;
            }
        }
        let inside = root.is_empty() || base == root || base.starts_with(&format!("{root}/"));
        if !inside
            || rest
                .iter()
                .any(|s| matches!(*s, "self" | "super" | "crate"))
        {
            return None;
        }
        let module = rest.iter().fold(base, |dir, segment| join(&dir, segment));
        [format!("{module}.rs"), join(&module, "mod.rs")]
            .iter()
            .find_map(|file| self.rust_files.get(file.as_str()).copied())
    }
}
