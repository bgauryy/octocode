use anyhow::{Result, bail};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

/// Resolve each link before consuming a following parent component. Missing
/// components remain logical paths, so callers can reserve files before creation.
pub fn resolve_path(base: &Path, path: &str) -> Result<PathBuf> {
    resolve(&base.join(path), &mut 0)
}

fn resolve(path: &Path, links: &mut usize) -> Result<PathBuf> {
    let mut resolved = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                resolved.pop();
            }
            Component::Normal(name) => {
                resolved.push(name);
                match fs::symlink_metadata(&resolved) {
                    Ok(metadata) if metadata.file_type().is_symlink() => {
                        *links += 1;
                        if *links > 40 {
                            bail!("Too many symbolic links");
                        }
                        let target = fs::read_link(&resolved)?;
                        resolved.pop();
                        resolved = resolve(&resolved.join(target), links)?;
                    }
                    Ok(_) => {
                        resolved = fs::canonicalize(&resolved)?;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
            other => resolved.push(other.as_os_str()),
        }
    }
    Ok(resolved)
}

/// Canonical caseless form of one component: `NFD(casefold(NFD(text)))`.
fn fold(component: &str, key: &mut String) {
    use caseless::Caseless;
    use unicode_normalization::UnicodeNormalization;
    if component.is_ascii() {
        key.extend(component.chars().map(|c| c.to_ascii_lowercase()));
    } else {
        key.extend(component.chars().nfd().default_case_fold().nfd());
    }
}

/// A portable, conservative lease namespace, independent of volume case rules:
/// `"/" + fold(component)` for each prefix/normal/parent component (root and `.`
/// contribute nothing). Equal keys alias; a tree contains keys under `key + "/"`.
/// This is never used to construct a filesystem access path or containment check.
pub fn lease_key(path: &str) -> String {
    let mut key = String::with_capacity(path.len() + 1);
    for component in Path::new(path).components() {
        match component {
            Component::RootDir | Component::CurDir => {}
            other => {
                key.push('/');
                match other.as_os_str().to_str() {
                    Some(text) => fold(text, &mut key),
                    None => key.push_str(&other.as_os_str().to_string_lossy()),
                }
            }
        }
    }
    key
}

/// Proper ancestors of a lease key, nearest last (`/a/b/c` -> `["", "/a", "/a/b"]`).
pub fn ancestors(key: &str) -> Vec<&str> {
    key.match_indices('/')
        .map(|(index, _)| &key[..index])
        .collect()
}

/// Whether two lease keys conflict under file/tree containment.
pub fn keys_overlap(a: &str, ak: &str, b: &str, bk: &str) -> bool {
    let contains = |tree: &str, inner: &str| {
        inner.len() > tree.len() && inner.starts_with(tree) && inner.as_bytes()[tree.len()] == b'/'
    };
    a == b || (ak == "tree" && contains(a, b)) || (bk == "tree" && contains(b, a))
}

pub fn overlap(a: &str, ak: &str, b: &str, bk: &str) -> bool {
    keys_overlap(&lease_key(a), ak, &lease_key(b), bk)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_fold_case_and_normalization_per_component() {
        assert_eq!(lease_key("/Work/SRC/File.TXT"), "/work/src/file.txt");
        assert_eq!(lease_key("/w/Stra\u{df}e"), lease_key("/w/STRASSE"));
        assert_eq!(lease_key("/w/\u{e9}"), lease_key("/w/E\u{301}"));
        assert_eq!(lease_key("."), "");
        assert_eq!(ancestors("/a/b/c"), vec!["", "/a", "/a/b"]);
        assert!(overlap("/w/src", "tree", "/w/SRC/x", "file"));
        assert!(!overlap("/w/src", "tree", "/w/src-other", "file"));
        assert!(!overlap("/w/src", "file", "/w/src/x", "file"));
        assert!(overlap("/w/src/x", "file", "/w/src", "tree"));
        assert!(overlap(".", "tree", "src", "file"));
    }
}
