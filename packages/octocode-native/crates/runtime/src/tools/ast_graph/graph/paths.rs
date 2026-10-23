//! Slash-path arithmetic shared by the resolvers.
use super::*;

pub(in super::super) fn dirname(p: &str) -> &str {
    p.rsplit_once('/').map(|x| x.0).unwrap_or(".")
}

pub(in super::super) fn join(a: &str, b: &str) -> String {
    normalize(&format!("{a}/{b}"))
}

pub(in super::super) fn join_within_root(a: &str, b: &str) -> Option<String> {
    climb(Vec::new(), &format!("{a}/{b}")).map(|parts| parts.join("/"))
}

/// The relative `path` walked from `parts`; `None` when a `..` climbs above
/// them or `path` is absolute.
pub(in super::super) fn climb(mut parts: Vec<String>, path: &str) -> Option<Vec<String>> {
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
