//! Native Octocode home policy, owned by @octocodeai/config alongside src/home.ts.
use std::path::{Component, Path, PathBuf};

pub fn octocode_home(override_path: Option<&str>, cwd: &Path, os_home: &Path) -> PathBuf {
    let path = match override_path.map(str::trim).filter(|s| !s.is_empty()) {
        Some(value) => cwd.join(value),
        None => os_home.join(".octocode"),
    };
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}
