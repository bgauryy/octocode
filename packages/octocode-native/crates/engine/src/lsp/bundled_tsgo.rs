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
mod tests {
    use super::*;
    use crate::test_support::TmpDir;
    use std::path::Path;

    fn write_executable(path: &Path) {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(path, "tsgo").expect("write");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        }
    }

    /// The platform package is named like Node's `process.platform-arch`, and
    /// Windows executables carry `.exe`; both are pure, so they are checked
    /// for every platform, not only the host.
    #[test]
    fn the_platform_package_and_executable_follow_node_naming() {
        assert_eq!(
            platform_package("darwin-arm64"),
            "native-preview-darwin-arm64"
        );
        assert_eq!(executable_name("darwin-arm64"), "tsgo");
        assert_eq!(executable_name("linux-x64"), "tsgo");
        assert_eq!(executable_name("win32-x64"), "tsgo.exe");
    }

    /// Any ancestor of the anchor's directory may hold the install's
    /// `node_modules` (a hoisted workspace install, a global one).
    #[test]
    fn a_hoisted_platform_package_is_found_from_any_ancestor() {
        let root = TmpDir::new();
        let tsgo = root.join("node_modules/@typescript/native-preview-linux-x64/lib/tsgo");
        write_executable(&tsgo);
        let anchor = root.join("node_modules/@octocodeai/octocode-native-linux-x64/octocode");
        let found = bundled_tsgo_for(&anchor, "linux-x64").expect("found");
        assert_eq!(found, std::fs::canonicalize(&tsgo).expect("canonical"));
        assert_eq!(bundled_tsgo_for(&anchor, "linux-arm64"), None);
    }

    #[test]
    fn a_windows_package_resolves_the_exe_file() {
        let root = TmpDir::new();
        let exe = root.join("node_modules/@typescript/native-preview-win32-x64/lib/tsgo.exe");
        write_executable(&exe);
        let anchor = root.join("node_modules/octocode/out/octocode.exe");
        assert_eq!(
            bundled_tsgo_for(&anchor, "win32-x64"),
            Some(std::fs::canonicalize(&exe).expect("canonical"))
        );
        // The plain name is the wrong file for Windows.
        assert_eq!(bundled_tsgo_for(&anchor, "linux-x64"), None);
    }

    /// An isolated install (pnpm) links `native-preview` to a store
    /// directory whose sibling is the platform package.
    #[cfg(unix)]
    #[test]
    fn an_isolated_install_resolves_the_platform_package_beside_the_real_package() {
        let root = TmpDir::new();
        let store = root.join("node_modules/.pnpm/@typescript+native-preview@1/node_modules");
        let tsgo = store.join("@typescript/native-preview-linux-x64/lib/tsgo");
        write_executable(&tsgo);
        std::fs::create_dir_all(store.join("@typescript/native-preview")).expect("real package");
        let scope = root.join("node_modules/@typescript");
        std::fs::create_dir_all(&scope).expect("scope");
        std::os::unix::fs::symlink(
            store.join("@typescript/native-preview"),
            scope.join("native-preview"),
        )
        .expect("link");
        let anchor = root.join("node_modules/octocode-native/octocode.node");
        assert_eq!(
            bundled_tsgo_for(&anchor, "linux-x64"),
            Some(std::fs::canonicalize(&tsgo).expect("canonical"))
        );
    }

    /// A file that is not executable is not a server.
    #[cfg(unix)]
    #[test]
    fn a_non_executable_file_is_not_a_server() {
        use std::os::unix::fs::PermissionsExt;
        let root = TmpDir::new();
        let tsgo = root.join("node_modules/@typescript/native-preview-linux-x64/lib/tsgo");
        write_executable(&tsgo);
        std::fs::set_permissions(&tsgo, std::fs::Permissions::from_mode(0o644)).expect("chmod");
        let anchor = root.join("node_modules/octocode-native/octocode.node");
        assert_eq!(bundled_tsgo_for(&anchor, "linux-x64"), None);
    }
}
