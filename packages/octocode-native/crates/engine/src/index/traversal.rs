//! Live filesystem traversal helpers for freshness verification.
//!
//! These walk the current on-disk tree (bounded by entry and depth limits),
//! render root-relative display paths, and apply exclusion rules. They depend
//! only on `std` and hold no index state, so they live apart from the store
//! machinery in `super`.

use std::fs;
use std::path::Path;

pub(super) fn collect_current_paths_bounded(
    root: &Path,
    exclusions: &[String],
    max_entries: usize,
    max_depth: usize,
    files: &mut Vec<String>,
    unverifiable: &mut Vec<String>,
    scanned_entries: &mut usize,
) -> bool {
    let mut pending = std::collections::VecDeque::from([(root.to_path_buf(), 0_usize)]);
    let mut scanned = 0_usize;
    let mut complete = true;
    while let Some((directory, depth)) = pending.pop_front() {
        let remaining = max_entries.saturating_sub(scanned);
        if remaining == 0 {
            return false;
        }
        let mut entries = Vec::new();
        let read = match fs::read_dir(&directory) {
            Ok(read) => read,
            Err(_) => {
                unverifiable.push(relative_display(root, &directory));
                complete = false;
                continue;
            }
        };
        for entry in read {
            match entry {
                Ok(entry) => entries.push(entry.path()),
                Err(_) => {
                    unverifiable.push(relative_display(root, &directory));
                    complete = false;
                }
            }
            if entries.len() > remaining {
                return false;
            }
        }
        entries.sort();
        scanned = scanned.saturating_add(entries.len());
        *scanned_entries = scanned;
        for path in entries {
            let relative = relative_display(root, &path);
            if path_is_excluded(&relative, exclusions) {
                continue;
            }
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(_) => {
                    unverifiable.push(relative);
                    complete = false;
                    continue;
                }
            };
            if metadata.file_type().is_symlink() {
                unverifiable.push(relative);
                complete = false;
            } else if metadata.is_dir() {
                if depth >= max_depth {
                    unverifiable.push(relative);
                    complete = false;
                } else {
                    pending.push_back((path, depth + 1));
                }
            } else if metadata.is_file() {
                files.push(relative);
            } else {
                unverifiable.push(relative);
                complete = false;
            }
        }
    }
    files.sort();
    complete
}

fn relative_display(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let displayed = relative.to_string_lossy().replace('\\', "/");
    if displayed.is_empty() {
        ".".to_owned()
    } else {
        displayed
    }
}

fn path_is_excluded(relative: &str, exclusions: &[String]) -> bool {
    exclusions.iter().any(|exclusion| {
        let normalized = exclusion.trim_matches('/');
        !normalized.is_empty()
            && (relative == normalized
                || relative.starts_with(&format!("{normalized}/"))
                || (!normalized.contains('/')
                    && relative.split('/').any(|part| part == normalized)))
    })
}
