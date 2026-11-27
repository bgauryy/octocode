//! Credential file text never reaches a response or the disk through a side
//! door: a history patch, a repository symlink, a saved answer.

mod ambiguous_name;
mod history_patches;
mod local_names;
mod revocation;
mod symlinks;

use std::path::Path;

/// Every file under `dir` holding `marker`.
pub(crate) fn files_holding(dir: &Path, marker: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in std::fs::read_dir(&next).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if std::fs::read(&path).is_ok_and(|bytes| {
                bytes
                    .windows(marker.len())
                    .any(|chunk| chunk == marker.as_bytes())
            }) {
                found.push(path.display().to_string());
            }
        }
    }
    found
}
