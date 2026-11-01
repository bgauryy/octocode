//! Slash-path arithmetic shared by the graph builder, its resolvers and the
//! store: a leaf, so every module of the code graph reaches it directly.
use octocode_engine::text::{lexical, slash_path};
use std::path::{Component, Path};

/// The directory of slash path `p`; `.` for a root-level file.
pub(crate) fn dirname(p: &str) -> &str {
    p.rsplit_once('/').map_or(".", |(dir, _)| dir)
}

/// The directory of slash path `p`; the empty root for a root-level file
/// (the form [`join_within_root`] and workspace keys use).
pub(crate) fn parent(p: &str) -> &str {
    p.rsplit_once('/').map_or("", |(dir, _)| dir)
}

pub(crate) fn join(a: &str, b: &str) -> String {
    normalize(&format!("{a}/{b}"))
}

pub(crate) fn join_within_root(a: &str, b: &str) -> Option<String> {
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

/// `p` as a root-relative slash path with `.` and `..` folded by the one
/// lexical fold; a leading root is dropped.
pub(crate) fn normalize(p: &str) -> String {
    let folded = slash_path(&lexical(Path::new(&p.replace('\\', "/"))));
    folded.trim_start_matches('/').to_owned()
}
