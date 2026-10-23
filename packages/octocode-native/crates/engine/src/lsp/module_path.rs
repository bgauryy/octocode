//! Where octocode's own code was installed.
//!
//! octocode runs as one native executable (CLI and MCP server alike), so
//! `current_exe` is the install tree that holds the bundled `tsgo` and a
//! `typescript-language-server` installed beside it.

use std::path::PathBuf;

/// Where octocode's install tree starts: the running executable, symlinks
/// resolved; `None` when the platform cannot say.
pub(crate) fn install_anchor() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .map(|path| std::fs::canonicalize(&path).unwrap_or(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_anchor_is_the_running_executable() {
        let exe = std::fs::canonicalize(std::env::current_exe().expect("exe")).expect("canonical");
        assert_eq!(install_anchor(), Some(exe));
    }
}
