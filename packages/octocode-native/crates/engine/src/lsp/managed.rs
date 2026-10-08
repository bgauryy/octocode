//! Managed language-server installs: the pinned auto-download manifest and the
//! verified cache layout `<root>/<server>/<releaseTag>/<binName>` with its
//! `.ok` completion marker. Discovery reads installs from here; the CLI
//! provisioner downloads, verifies, and writes them.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Archive encodings the manifest declares. Add a variant together with the
/// manifest entry that uses it and its extraction arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveKind {
    Gz,
    Zip,
}

/// One platform's downloadable asset for a server.
#[derive(Debug, Clone)]
pub struct ManifestAsset {
    pub url: &'static str,
    pub archive: ArchiveKind,
    pub bin_name: &'static str,
    /// Path of the executable inside a zip archive; `None` for gz.
    pub bin_path: Option<&'static str>,
    /// SHA-256 of the downloaded asset; download is refused while this is `None`.
    pub sha256: Option<&'static str>,
}

/// A manifest server entry keyed by its bare command name.
#[derive(Debug, Clone)]
pub struct ManifestServer {
    pub language_id: &'static str,
    pub repo: &'static str,
    pub release_tag: &'static str,
    pub platforms: BTreeMap<&'static str, ManifestAsset>,
    pub unsupported_platforms: BTreeMap<&'static str, &'static str>,
}

/// The canonical auto-download manifest.
pub fn manifest() -> &'static BTreeMap<&'static str, ManifestServer> {
    static MANIFEST: OnceLock<BTreeMap<&'static str, ManifestServer>> = OnceLock::new();
    MANIFEST.get_or_init(build_manifest)
}

fn build_manifest() -> BTreeMap<&'static str, ManifestServer> {
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

    let mut ra = BTreeMap::new();
    for (platform, url, sha) in [
        (
            "darwin-arm64",
            "https://github.com/rust-lang/rust-analyzer/releases/download/2026-06-22/rust-analyzer-aarch64-apple-darwin.gz",
            "c8cdf6d5e488752b907d5ee15e31768b59a78d992e9a54b9f9660e1bfdf39f27",
        ),
        (
            "darwin-x64",
            "https://github.com/rust-lang/rust-analyzer/releases/download/2026-06-22/rust-analyzer-x86_64-apple-darwin.gz",
            "feb7c170d2c1a2e4b8a88ac73f937eddb576828e3821b0a63ee0e64bd0bc9440",
        ),
        (
            "linux-arm64",
            "https://github.com/rust-lang/rust-analyzer/releases/download/2026-06-22/rust-analyzer-aarch64-unknown-linux-gnu.gz",
            "bf65b0d4586f127ab11bf33476dd6aac82dad173946c5d3b1cede19d63ae85ed",
        ),
        (
            "linux-x64",
            "https://github.com/rust-lang/rust-analyzer/releases/download/2026-06-22/rust-analyzer-x86_64-unknown-linux-gnu.gz",
            "9602ca5b24dcaa07a5a021274763bed367d8a32da9a226fe3e139de3306569cb",
        ),
        (
            "linux-x64-musl",
            "https://github.com/rust-lang/rust-analyzer/releases/download/2026-06-22/rust-analyzer-x86_64-unknown-linux-musl.gz",
            "fe1d7b0e9733f7a439e4b6f27b8c4cc7afd87ae28fc5b496eb8df31d674b78dd",
        ),
    ] {
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
pub fn manifest_server(name: &str) -> Option<&'static ManifestServer> {
    manifest().get(bare_name(name))
}

fn bare_name(name: &str) -> &str {
    Path::new(name)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(name)
}

/// The canonical `{os}-{arch}[-musl]` platform id the manifest is keyed on.
pub fn platform_id() -> String {
    platform_id_for(
        std::env::consts::OS,
        std::env::consts::ARCH,
        is_musl_linux(),
    )
}

fn platform_id_for(os: &str, arch: &str, musl: bool) -> String {
    // Only the two shipped architectures get their npm-style names; any other
    // keeps its Rust name, so manifest lookup reports it unsupported instead
    // of selecting an arm64 binary (review L12).
    let arch = match arch {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        other => other,
    };
    match os {
        "macos" => format!("darwin-{arch}"),
        "windows" => format!("win32-{arch}"),
        _ => {
            let suffix = if musl { "-musl" } else { "" };
            format!("linux-{arch}{suffix}")
        }
    }
}

/// True when the current Linux runtime links musl libc (Alpine etc.).
fn is_musl_linux() -> bool {
    if std::env::consts::OS != "linux" {
        return false;
    }
    std::fs::read_dir("/lib").is_ok_and(|entries| {
        entries.flatten().any(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.starts_with("ld-musl-"))
        })
    })
}

/// Where a provisioned binary lives once installed:
/// `<root>/<server>/<releaseTag>/<binName>`.
pub fn cached_server_bin_path(root: &Path, name: &str, platform: &str) -> Option<PathBuf> {
    let server = manifest_server(name)?;
    let asset = server.platforms.get(platform)?;
    Some(
        root.join(bare_name(name))
            .join(server.release_tag)
            .join(asset.bin_name),
    )
}

/// The `.ok` completion marker beside an installed binary.
pub fn marker_path(bin_path: &Path) -> PathBuf {
    let mut s = bin_path.as_os_str().to_os_string();
    s.push(".ok");
    PathBuf::from(s)
}

/// The asset digest, binary hash and size, as the `.ok` marker records them.
struct CacheMarker {
    asset_sha256: String,
    binary_sha256: String,
    size: u64,
}

fn read_cache_marker(marker_path: &Path) -> Option<CacheMarker> {
    let text = std::fs::read_to_string(marker_path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let digest = |key: &str| {
        let hex = value.get(key)?.as_str()?;
        (hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit())).then(|| hex.to_string())
    };
    Some(CacheMarker {
        asset_sha256: digest("assetSha256")?,
        binary_sha256: digest("binarySha256")?,
        size: value.get("size")?.as_u64()?,
    })
}

/// File identity of a binary whose hash already matched its marker. A changed
/// size or mtime forces a rehash, so each LSP spawn does not hash a large
/// binary again (review M7).
#[derive(Clone, PartialEq)]
struct VerifiedBinary {
    size: u64,
    modified: std::time::SystemTime,
    binary_sha256: String,
}

fn verified_binaries()
-> &'static std::sync::Mutex<std::collections::HashMap<PathBuf, VerifiedBinary>> {
    static VERIFIED: OnceLock<
        std::sync::Mutex<std::collections::HashMap<PathBuf, VerifiedBinary>>,
    > = OnceLock::new();
    VERIFIED.get_or_init(Default::default)
}

/// The installed binary for `name`, only when it is present AND its `.ok`
/// marker names the asset the manifest pins now and matches the binary's
/// current hash and size. Read-only.
pub fn resolve_cached_server(root: &Path, name: &str, platform: &str) -> Option<PathBuf> {
    let pinned_asset = manifest_server(name)?.platforms.get(platform)?.sha256?;
    let bin_path = cached_server_bin_path(root, name, platform)?;
    let marker = read_cache_marker(&marker_path(&bin_path))?;
    // A binary from another asset (manifest bumped at the same release-tag
    // path) is stale, even when its own hash still matches (review L15).
    if !marker.asset_sha256.eq_ignore_ascii_case(pinned_asset) {
        return None;
    }
    let metadata = std::fs::metadata(&bin_path).ok()?;
    if metadata.len() != marker.size {
        return None;
    }
    let identity = metadata.modified().ok().map(|modified| VerifiedBinary {
        size: metadata.len(),
        modified,
        binary_sha256: marker.binary_sha256.clone(),
    });
    let cache = verified_binaries();
    if let Some(identity) = &identity
        && cache
            .lock()
            .ok()
            .is_some_and(|map| map.get(&bin_path) == Some(identity))
    {
        return Some(bin_path);
    }
    let bytes = std::fs::read(&bin_path).ok()?;
    #[cfg(test)]
    HASH_CALLS.with(|calls| calls.set(calls.get() + 1));
    let matches =
        bytes.len() as u64 == marker.size && crate::digest::sha256(&bytes) == marker.binary_sha256;
    if let Ok(mut map) = cache.lock() {
        match identity {
            Some(identity) if matches => {
                map.insert(bin_path.clone(), identity);
            }
            _ => {
                map.remove(&bin_path);
            }
        }
    }
    matches.then_some(bin_path)
}

#[cfg(test)]
thread_local! {
    /// Binary hashes computed on this thread (test observation of M7 memoization).
    static HASH_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn install(root: &Path, name: &str, platform: &str, bytes: &[u8]) -> PathBuf {
        let bin = cached_server_bin_path(root, name, platform).expect("bin path");
        std::fs::create_dir_all(bin.parent().expect("parent")).expect("dir");
        std::fs::write(&bin, bytes).expect("bin");
        let asset_sha = manifest_server(name).expect("server").platforms[platform]
            .sha256
            .expect("pinned asset");
        install_with_asset(root, name, platform, bytes, asset_sha)
    }

    fn install_with_asset(
        root: &Path,
        name: &str,
        platform: &str,
        bytes: &[u8],
        asset_sha: &str,
    ) -> PathBuf {
        let bin = cached_server_bin_path(root, name, platform).expect("bin path");
        std::fs::create_dir_all(bin.parent().expect("parent")).expect("dir");
        std::fs::write(&bin, bytes).expect("bin");
        let marker = serde_json::json!({
            "assetSha256": asset_sha,
            "binarySha256": crate::digest::sha256(bytes),
            "size": bytes.len(),
        });
        std::fs::write(marker_path(&bin), marker.to_string()).expect("marker");
        bin
    }

    fn temp_root(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "octocode-managed-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ))
    }

    /// Review L15: a binary installed from a different asset than the one the
    /// manifest now pins (same release-tag path) is not trusted.
    #[test]
    fn resolve_rejects_marker_for_a_different_asset() {
        let root = temp_root("asset");
        install_with_asset(&root, "rust-analyzer", "linux-x64", b"ra", &"0".repeat(64));
        assert_eq!(
            resolve_cached_server(&root, "rust-analyzer", "linux-x64"),
            None
        );
        let _ = std::fs::remove_dir_all(root);
    }

    /// Review M7: an unchanged binary is hashed once per process, not on
    /// every resolve; a changed binary is hashed again and rejected.
    #[test]
    fn resolve_hashes_an_unchanged_binary_once() {
        let root = temp_root("memo");
        let bin = install(&root, "rust-analyzer", "linux-x64", b"ra-memo");
        let before = HASH_CALLS.with(std::cell::Cell::get);
        for _ in 0..3 {
            assert_eq!(
                resolve_cached_server(&root, "rust-analyzer", "linux-x64"),
                Some(bin.clone())
            );
        }
        assert_eq!(HASH_CALLS.with(std::cell::Cell::get) - before, 1);
        // Same size, new content; move mtime explicitly so a coarse
        // filesystem clock cannot hide the rewrite.
        let modified = std::fs::metadata(&bin)
            .and_then(|m| m.modified())
            .expect("mtime");
        std::fs::write(&bin, b"ra-mem2").expect("tamper");
        std::fs::File::options()
            .write(true)
            .open(&bin)
            .and_then(|f| f.set_modified(modified + std::time::Duration::from_secs(2)))
            .expect("set mtime");
        assert_eq!(
            resolve_cached_server(&root, "rust-analyzer", "linux-x64"),
            None
        );
        assert_eq!(HASH_CALLS.with(std::cell::Cell::get) - before, 2);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn manifest_has_clangd_and_rust_analyzer() {
        let m = manifest();
        let ra = &m["rust-analyzer"];
        assert_eq!(ra.language_id, "rust");
        assert_eq!(ra.release_tag, "2026-06-22");
        assert_eq!(ra.platforms["linux-x64"].archive, ArchiveKind::Gz);
        assert_eq!(
            m["clangd"].platforms["darwin-arm64"].archive,
            ArchiveKind::Zip
        );
    }

    /// Review L12: an architecture the manifest does not ship must not be
    /// mapped onto another architecture's binary.
    #[test]
    fn platform_id_keeps_unknown_architectures_distinct() {
        assert_eq!(platform_id_for("macos", "aarch64", false), "darwin-arm64");
        assert_eq!(platform_id_for("linux", "x86_64", true), "linux-x64-musl");
        assert_eq!(platform_id_for("windows", "x86_64", false), "win32-x64");
        let riscv = platform_id_for("linux", "riscv64", false);
        assert_eq!(riscv, "linux-riscv64");
        assert!(
            manifest()
                .values()
                .all(|server| !server.platforms.contains_key(riscv.as_str()))
        );
        assert_eq!(platform_id_for("linux", "x86", false), "linux-x86");
    }

    #[test]
    fn sha256_matches_known_vector() {
        assert_eq!(
            crate::digest::sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn resolve_trusts_only_a_matching_marker() {
        let root = std::env::temp_dir().join(format!(
            "octocode-managed-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let bin = install(&root, "rust-analyzer", "linux-x64", b"ra");
        assert_eq!(
            resolve_cached_server(&root, "/usr/bin/rust-analyzer", "linux-x64"),
            Some(bin.clone())
        );
        std::fs::write(&bin, b"rb").expect("tamper");
        assert_eq!(
            resolve_cached_server(&root, "rust-analyzer", "linux-x64"),
            None
        );
        std::fs::write(&bin, b"ra").expect("restore");
        std::fs::remove_file(marker_path(&bin)).expect("marker");
        assert_eq!(
            resolve_cached_server(&root, "rust-analyzer", "linux-x64"),
            None
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
