//! octocode's own native TypeScript server (`tsgo`), found in octocode's
//! install tree (`octocode-mcp` and `octocode` depend on it).
//!
//! The server is the platform package of `@typescript/native-preview`
//! (`@typescript/native-preview-<platform>`), whose `lib/tsgo` is the real
//! executable: running it directly skips the package's Node launcher. Only
//! octocode's install tree is searched, never the scanned workspace, so this
//! lookup is safe to run for an untrusted checkout.

use crate::lsp::commands::is_executable_path;
use std::path::{Path, PathBuf};

/// The bundled tsgo for this platform, searched from the directory holding
/// `anchor` (octocode's executable) up through its ancestors.
pub(crate) fn bundled_tsgo(anchor: &Path) -> Option<PathBuf> {
    bundled_tsgo_for(anchor, &crate::lsp::managed::npm_platform_id())
}

/// `@typescript/native-preview-<platform>` without its scope.
fn platform_package(platform: &str) -> String {
    format!("native-preview-{platform}")
}

fn executable_name(platform: &str) -> &'static str {
    if platform.starts_with("win32") {
        "tsgo.exe"
    } else {
        "tsgo"
    }
}

fn bundled_tsgo_for(anchor: &Path, platform: &str) -> Option<PathBuf> {
    let package = platform_package(platform);
    let executable = executable_name(platform);
    let in_package = |scope: &Path| {
        let candidate = scope.join(&package).join("lib").join(executable);
        is_executable_path(&candidate)
            .then(|| std::fs::canonicalize(&candidate).unwrap_or(candidate))
    };
    anchor.parent()?.ancestors().find_map(|dir| {
        let scope = dir.join("node_modules").join("@typescript");
        in_package(&scope).or_else(|| {
            // An isolated install (pnpm) links `native-preview` to a store
            // directory; its platform package is a sibling of the real one.
            let real = std::fs::canonicalize(scope.join("native-preview")).ok()?;
            in_package(real.parent()?)
        })
    })
}

#[cfg(test)]
mod tests;
