//! Slash-path arithmetic shared by the graph builder, its resolvers and the
//! store: a leaf, so every module of the code graph reaches it directly.
use octocode_engine::text::{lexical, slash_path};
use std::path::{Component, Path};

/// The directory of slash path `p`; `.` for a root-level file.
pub fn dirname(p: &str) -> &str {
    p.rsplit_once('/').map_or(".", |(dir, _)| dir)
}

/// The directory of slash path `p`; the empty root for a root-level file
/// (the form [`join_within_root`] and workspace keys use).
pub fn parent(p: &str) -> &str {
    p.rsplit_once('/').map_or("", |(dir, _)| dir)
}

pub fn join(a: &str, b: &str) -> String {
    normalize(&format!("{a}/{b}"))
}

pub fn join_within_root(a: &str, b: &str) -> Option<String> {
    climb(Vec::new(), &format!("{a}/{b}")).map(|parts| parts.join("/"))
}

/// The relative `path` walked from `parts`; `None` when a `..` climbs above
/// them or `path` is absolute.
pub(crate) fn climb(mut parts: Vec<String>, path: &str) -> Option<Vec<String>> {
    for component in Path::new(path).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop()?;
            }
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(parts)
}

/// [`normalize`] of a typed JS specifier (a package manifest's export or
/// entry target): manifests are written on every platform, so a `\` in one
/// reads as a separator. Walked paths keep theirs: on Unix it is a name byte.
pub fn normalize_specifier(p: &str) -> String {
    normalize(&p.replace('\\', "/"))
}

/// `p` as a root-relative slash path with `.` and `..` folded by the one
/// lexical fold; a leading root is dropped.
pub fn normalize(p: &str) -> String {
    let folded = slash_path(&lexical(Path::new(p)));
    folded.trim_start_matches('/').to_owned()
}

/// The directories above `root`, nearest first, up to its repository's
/// boundary: what may hold code a scan of `root` imports. A root that is itself
/// a repository's has none, and the walk ends after a level bound when no
/// boundary shows.
pub fn dirs_above_within_repository(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    const MAX_LEVELS: usize = 16;
    if octocode_engine::search::repository::is_root(root) {
        return Vec::new();
    }
    let mut dirs = Vec::new();
    for dir in root.ancestors().skip(1).take(MAX_LEVELS) {
        dirs.push(dir.to_path_buf());
        if octocode_engine::search::repository::is_root(dir) {
            break;
        }
    }
    dirs
}
