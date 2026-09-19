//! Static auto-download manifest data for the LSP provisioner: the archive/asset
//! DTOs and the canonical server table (clangd, rust-analyzer) plus the keyed
//! lookup. Pure data with no dependencies on the rest of the provisioning
//! pipeline; the parent module resolves, verifies, and installs from it.
use std::collections::BTreeMap;
use std::path::Path;

/// Archive encodings the manifest can declare. Only `None`/`Gz`/`Zip` are
/// extractable here (parity with TS); tar variants are detect-and-instruct.
/// `None`/`TarGz`/`TarXz` are absent from current manifest data but kept for
/// schema parity with the TS `ArchiveKind` union and its extraction handling.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ArchiveKind {
    None,
    Gz,
    Zip,
    TarGz,
    TarXz,
}

/// One platform's downloadable asset for a server.
#[derive(Debug, Clone)]
pub(super) struct ManifestAsset {
    pub url: &'static str,
    pub archive: ArchiveKind,
    pub bin_name: &'static str,
    /// Path of the executable inside a zip/tar archive; `None` for gz/none.
    pub bin_path: Option<&'static str>,
    /// SHA-256 of the downloaded asset; download is refused while this is `None`.
    pub sha256: Option<&'static str>,
}

/// A manifest server entry keyed by its bare command name.
#[derive(Debug, Clone)]
pub(super) struct ManifestServer {
    pub language_id: &'static str,
    pub repo: &'static str,
    pub release_tag: &'static str,
    pub platforms: BTreeMap<&'static str, ManifestAsset>,
    pub unsupported_platforms: BTreeMap<&'static str, &'static str>,
}

/// The canonical auto-download manifest.
pub(super) fn manifest() -> BTreeMap<&'static str, ManifestServer> {
    let mut servers = BTreeMap::new();

    let mut clangd = BTreeMap::new();
    for (platform, url, sha) in [
        (
            "darwin-arm64",
            "https://github.com/clangd/clangd/releases/download/22.1.0/clangd-mac-22.1.0.zip",
            "e31e271fe11f6dcd7cf87ca74be4a12788ff8ce5a0b07762583e335c058e939a",
        ),
        (
            "darwin-x64",
            "https://github.com/clangd/clangd/releases/download/22.1.0/clangd-mac-22.1.0.zip",
            "e31e271fe11f6dcd7cf87ca74be4a12788ff8ce5a0b07762583e335c058e939a",
        ),
        (
            "linux-x64",
            "https://github.com/clangd/clangd/releases/download/22.1.0/clangd-linux-22.1.0.zip",
            "71eddc5303da9a5bc5e8b509488b5b2c5acf45f20e33b8394e71a12a56d67198",
        ),
    ] {
        clangd.insert(
            platform,
            ManifestAsset {
                url,
                archive: ArchiveKind::Zip,
                bin_name: "clangd",
                bin_path: Some("clangd_22.1.0/bin/clangd"),
                sha256: Some(sha),
            },
        );
    }
    clangd.insert(
        "win32-x64",
        ManifestAsset {
            url: "https://github.com/clangd/clangd/releases/download/22.1.0/clangd-windows-22.1.0.zip",
            archive: ArchiveKind::Zip,
            bin_name: "clangd.exe",
            bin_path: Some("clangd_22.1.0/bin/clangd.exe"),
            sha256: Some("c54e57dbff3ccc9e8352367ddb7030ad3f624073ec58c7477424e7919f578572"),
        },
    );
    let mut clangd_unsupported = BTreeMap::new();
    clangd_unsupported.insert(
        "linux-arm64",
        "clangd publishes no linux-arm64 release asset; install via the system package manager.",
    );
    servers.insert(
        "clangd",
        ManifestServer {
            language_id: "cpp",
            repo: "clangd/clangd",
            release_tag: "22.1.0",
            platforms: clangd,
            unsupported_platforms: clangd_unsupported,
        },
    );

    let base = "https://github.com/rust-lang/rust-analyzer/releases/download/2026-06-22";
    let mut ra = BTreeMap::new();
    for (platform, file, sha) in [
        (
            "darwin-arm64",
            "rust-analyzer-aarch64-apple-darwin.gz",
            "c8cdf6d5e488752b907d5ee15e31768b59a78d992e9a54b9f9660e1bfdf39f27",
        ),
        (
            "darwin-x64",
            "rust-analyzer-x86_64-apple-darwin.gz",
            "feb7c170d2c1a2e4b8a88ac73f937eddb576828e3821b0a63ee0e64bd0bc9440",
        ),
        (
            "linux-arm64",
            "rust-analyzer-aarch64-unknown-linux-gnu.gz",
            "bf65b0d4586f127ab11bf33476dd6aac82dad173946c5d3b1cede19d63ae85ed",
        ),
        (
            "linux-x64",
            "rust-analyzer-x86_64-unknown-linux-gnu.gz",
            "9602ca5b24dcaa07a5a021274763bed367d8a32da9a226fe3e139de3306569cb",
        ),
        (
            "linux-x64-musl",
            "rust-analyzer-x86_64-unknown-linux-musl.gz",
            "fe1d7b0e9733f7a439e4b6f27b8c4cc7afd87ae28fc5b496eb8df31d674b78dd",
        ),
    ] {
        // Leak the composed URL to obtain a &'static str for the static-shaped
        // manifest; called at most once per process on the install path.
        let url: &'static str = Box::leak(format!("{base}/{file}").into_boxed_str());
        ra.insert(
            platform,
            ManifestAsset {
                url,
                archive: ArchiveKind::Gz,
                bin_name: "rust-analyzer",
                bin_path: None,
                sha256: Some(sha),
            },
        );
    }
    ra.insert(
        "win32-arm64",
        ManifestAsset {
            url: "https://github.com/rust-lang/rust-analyzer/releases/download/2026-06-22/rust-analyzer-aarch64-pc-windows-msvc.zip",
            archive: ArchiveKind::Zip,
            bin_name: "rust-analyzer.exe",
            bin_path: Some("rust-analyzer.exe"),
            sha256: Some("30f873713ea3663db10999c23e95b74fe19968c893d5c0e9b8a896b31dbf8cf8"),
        },
    );
    ra.insert(
        "win32-x64",
        ManifestAsset {
            url: "https://github.com/rust-lang/rust-analyzer/releases/download/2026-06-22/rust-analyzer-x86_64-pc-windows-msvc.zip",
            archive: ArchiveKind::Zip,
            bin_name: "rust-analyzer.exe",
            bin_path: Some("rust-analyzer.exe"),
            sha256: Some("6071dc5b28aa6d22c715f63c08d75b827c066be4ea866796587e52ed48b2922f"),
        },
    );
    servers.insert(
        "rust-analyzer",
        ManifestServer {
            language_id: "rust",
            repo: "rust-lang/rust-analyzer",
            release_tag: "2026-06-22",
            platforms: ra,
            unsupported_platforms: BTreeMap::new(),
        },
    );

    servers
}

/// The manifest entry for a server, keyed by its bare command name.
pub(super) fn manifest_server(name: &str) -> Option<ManifestServer> {
    let base = Path::new(name)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(name);
    manifest().get(base).cloned()
}
