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

fn component_eq(a: Component<'_>, b: Component<'_>) -> bool {
    match (a.as_os_str().to_str(), b.as_os_str().to_str()) {
        (Some(a), Some(b)) => caseless::canonical_caseless_match_str(a, b),
        _ => a == b,
    }
}

/// A portable, conservative lease namespace, independent of volume case rules.
/// This is never used to construct a filesystem access path or containment check.
pub fn overlap(a: &str, ak: &str, b: &str, bk: &str) -> bool {
    let mut a = Path::new(a).components();
    let mut b = Path::new(b).components();
    loop {
        match (a.next(), b.next()) {
            (Some(a), Some(b)) if component_eq(a, b) => {}
            (None, None) => return true,
            (None, Some(_)) => return ak == "tree",
            (Some(_), None) => return bk == "tree",
            _ => return false,
        }
    }
}
