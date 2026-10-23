use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// `path` made absolute against the working directory and folded lexically
/// (`engine::text::lexical`), so a missing dry-run target normalizes too.
pub(in super::super) fn normalize(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    octocode_engine::text::lexical(&without_verbatim_prefix(absolute))
}

/// A Windows verbatim prefix (`\\?\C:`, `\\?\UNC\server\share`) as its
/// ordinary form, which junctions and other tools accept.
#[cfg(windows)]
fn without_verbatim_prefix(path: PathBuf) -> PathBuf {
    use std::path::{Component, Prefix};
    path.components()
        .map(|component| match component {
            Component::Prefix(prefix) => match prefix.kind() {
                Prefix::VerbatimDisk(drive) => PathBuf::from(format!("{}:", char::from(drive))),
                Prefix::VerbatimUNC(server, share) => PathBuf::from(format!(
                    "\\\\{}\\{}",
                    server.to_string_lossy(),
                    share.to_string_lossy()
                )),
                _ => PathBuf::from(prefix.as_os_str()),
            },
            other => PathBuf::from(other.as_os_str()),
        })
        .collect()
}

#[cfg(not(windows))]
fn without_verbatim_prefix(path: PathBuf) -> PathBuf {
    path
}

pub(super) fn join_path(a: &str, b: &str) -> String {
    normalize(&Path::new(a).join(b))
        .to_string_lossy()
        .into_owned()
}
pub(super) fn resolve_path(path: &str) -> String {
    normalize(Path::new(path)).to_string_lossy().into_owned()
}

pub(in super::super) fn path_exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

/// Public content comparison for freshness checks (`skill check`).
pub(in super::super) fn trees_match(left: &Path, right: &Path) -> bool {
    same_tree(left, right)
}

pub(super) fn same_tree(left: &Path, right: &Path) -> bool {
    if !path_exists(left) || !path_exists(right) {
        return false;
    }
    let Ok(left_meta) = fs::symlink_metadata(left) else {
        return false;
    };
    let Ok(right_meta) = fs::symlink_metadata(right) else {
        return false;
    };
    if left_meta.file_type().is_symlink() || right_meta.file_type().is_symlink() {
        return left_meta.file_type().is_symlink()
            && right_meta.file_type().is_symlink()
            && fs::read_link(left).ok() == fs::read_link(right).ok();
    }
    if left_meta.is_file() || right_meta.is_file() {
        return left_meta.is_file()
            && right_meta.is_file()
            && fs::read(left)
                .ok()
                .zip(fs::read(right).ok())
                .is_some_and(|(a, b)| a == b);
    }
    if !left_meta.is_dir() || !right_meta.is_dir() {
        return false;
    }
    let list = |dir: &Path| -> Option<BTreeSet<String>> {
        let mut names = BTreeSet::new();
        for entry in fs::read_dir(dir).ok()? {
            names.insert(entry.ok()?.file_name().to_string_lossy().into_owned());
        }
        Some(names)
    };
    match (list(left), list(right)) {
        (Some(left_names), Some(right_names)) => {
            left_names == right_names
                && left_names
                    .iter()
                    .all(|name| same_tree(&left.join(name), &right.join(name)))
        }
        _ => false,
    }
}

pub(super) fn copy_tree(source: &Path, destination: &Path) -> std::io::Result<()> {
    let meta = fs::symlink_metadata(source)?;
    if meta.file_type().is_symlink() {
        let target = fs::read_link(source)?;
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, destination)?;
        #[cfg(windows)]
        if source.is_dir() {
            let absolute = source.parent().unwrap_or(Path::new(".")).join(&target);
            junction::create(normalize(&absolute), destination)?;
        } else {
            std::os::windows::fs::symlink_file(&target, destination)?;
        }
        return Ok(());
    }
    if meta.is_dir() {
        fs::create_dir_all(destination)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_tree(&entry.path(), &destination.join(entry.file_name()))?;
        }
        return Ok(());
    }
    fs::copy(source, destination).map(|_| ())
}

pub(in super::super) fn remove_path(path: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    if junction::exists(path)? {
        return junction::delete(path);
    }
    let meta = fs::symlink_metadata(path)?;
    #[cfg(windows)]
    if meta.file_type().is_symlink() && path.is_dir() {
        return fs::remove_dir(path);
    }
    if meta.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

pub(super) fn replace_with(
    destination: &Path,
    prepare: impl FnOnce(&Path) -> std::io::Result<()>,
) -> Result<(), String> {
    let parent = destination.parent().ok_or("destination has no parent")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let transaction = tempfile::Builder::new()
        .prefix(".octocode-skill-")
        .tempdir_in(parent)
        .map_err(|error| error.to_string())?;
    let staged = transaction.path().join("next");
    let backup = transaction.path().join("previous");
    prepare(&staged).map_err(|error| error.to_string())?;
    let previous = path_exists(destination);
    if previous {
        fs::rename(destination, &backup).map_err(|error| error.to_string())?;
    }
    if let Err(error) = fs::rename(&staged, destination) {
        if previous && let Err(rollback) = fs::rename(&backup, destination) {
            let saved = transaction.keep();
            return Err(format!(
                "{error}; rollback failed: {rollback}; previous contents saved in {}",
                saved.display()
            ));
        }
        return Err(error.to_string());
    }
    if previous {
        remove_path(&backup).map_err(|error| {
            format!(
                "Installed, but could not remove backup {}: {error}",
                transaction.keep().display()
            )
        })?;
    }
    Ok(())
}

pub(super) fn replace_directory(source: &Path, destination: &Path) -> Result<(), String> {
    if physical_destination(destination).starts_with(physical_destination(source)) {
        return Err("The installation destination must not be inside its source".into());
    }
    replace_with(destination, |staged| copy_tree(source, staged))
}

pub(super) fn physical_destination(path: &Path) -> PathBuf {
    let absolute = normalize(path);
    let mut current = absolute.as_path();
    let mut missing = Vec::new();
    loop {
        if let Ok(mut resolved) = current.canonicalize() {
            for component in missing.into_iter().rev() {
                resolved.push(component);
            }
            return resolved;
        }
        let (Some(parent), Some(name)) = (current.parent(), current.file_name()) else {
            return absolute;
        };
        missing.push(name.to_owned());
        current = parent;
    }
}
pub(super) fn replace_link(target: &Path, destination: &Path) -> Result<(), String> {
    replace_with(destination, |staged| {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, staged)
        }
        #[cfg(windows)]
        {
            junction::create(target, staged)
        }
    })
}

pub(super) fn points_to(link_path: &Path, target_path: &str) -> bool {
    let Ok(meta) = fs::symlink_metadata(link_path) else {
        return false;
    };
    if !meta.file_type().is_symlink() {
        return false;
    }
    let Ok(raw) = fs::read_link(link_path) else {
        return false;
    };
    let raw = raw.to_string_lossy().into_owned();
    let resolved = if Path::new(&raw).is_absolute() {
        resolve_path(&raw)
    } else {
        join_path(
            &link_path
                .parent()
                .unwrap_or(Path::new("/"))
                .to_string_lossy(),
            &raw,
        )
    };
    resolved == resolve_path(target_path)
}
