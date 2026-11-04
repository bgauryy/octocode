//! Managed language-server installs: the auto-download manifest, the lock
//! that records GitHub's release digests, and the verified cache layout
//! `<root>/<server>/<releaseTag>/<binName>` with its `.ok` completion marker.
//! Discovery reads installs from here; the CLI provisioner downloads,
//! verifies, and writes them.
//!
//! The manifest holds only what a person decides: the repository, the release
//! tag, each platform's asset name, and where the executable sits in it. No
//! hash is typed here. `managed.lock.json` is generated from the GitHub
//! release API by `node skills-dev/octocode-dev/scripts/dev.mjs lsp:lock`,
//! which also reads [`SERVERS`] below: keep one string literal per field.
mod lock;
mod platform;

pub use lock::{AssetLock, ReleaseLock};
pub(crate) use platform::npm_platform_id;
pub use platform::platform_id;
#[cfg(test)]
use platform::platform_id_for;
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestAsset {
    /// The release asset's exact file name.
    pub asset: String,
    pub archive: ArchiveKind,
    pub bin_name: String,
    /// Path of the executable inside a zip archive; `None` for gz.
    pub bin_path: Option<String>,
    /// What the lock recorded from GitHub for this asset; `None` while the
    /// lock was not regenerated for this release, and download is refused.
    pub lock: Option<AssetLock>,
}

/// A manifest server entry keyed by its bare command name.
#[derive(Debug, Clone)]
pub struct ManifestServer {
    pub language_id: &'static str,
    pub repo: &'static str,
    pub release_tag: &'static str,
    pub platforms: BTreeMap<&'static str, ManifestAsset>,
    pub unsupported_platforms: BTreeMap<&'static str, &'static str>,
    /// The release as the lock recorded it; `None` when the lock names another
    /// repository or tag than the manifest.
    pub release_lock: Option<ReleaseLock>,
}

/// One platform's asset as a person declares it. `{tag}` in `asset` and
/// `bin_path` stands for the server's release tag.
struct AssetDecl {
    platform: &'static str,
    asset: &'static str,
    archive: ArchiveKind,
    bin_path: Option<&'static str>,
}

const fn zip(platform: &'static str, asset: &'static str, bin_path: &'static str) -> AssetDecl {
    AssetDecl {
        platform,
        asset,
        archive: ArchiveKind::Zip,
        bin_path: Some(bin_path),
    }
}

const fn gz(platform: &'static str, asset: &'static str) -> AssetDecl {
    AssetDecl {
        platform,
        asset,
        archive: ArchiveKind::Gz,
        bin_path: None,
    }
}

struct ServerDecl {
    name: &'static str,
    language_id: &'static str,
    repo: &'static str,
    release_tag: &'static str,
    assets: &'static [AssetDecl],
    unsupported: &'static [(&'static str, &'static str)],
}

/// The auto-download manifest as declared. `lsp:lock --bump` rewrites a
/// `release_tag` literal here; every other field is a person's decision.
const SERVERS: &[ServerDecl] = &[
    ServerDecl {
        name: "clangd",
        language_id: "cpp",
        repo: "clangd/clangd",
        release_tag: "22.1.0",
        assets: &[
            zip(
                "darwin-arm64",
                "clangd-mac-{tag}.zip",
                "clangd_{tag}/bin/clangd",
            ),
            zip(
                "darwin-x64",
                "clangd-mac-{tag}.zip",
                "clangd_{tag}/bin/clangd",
            ),
            zip(
                "linux-x64",
                "clangd-linux-{tag}.zip",
                "clangd_{tag}/bin/clangd",
            ),
            zip(
                "win32-x64",
                "clangd-windows-{tag}.zip",
                "clangd_{tag}/bin/clangd.exe",
            ),
        ],
        unsupported: &[(
            "linux-arm64",
            "clangd publishes no linux-arm64 release asset; install via the system package manager.",
        )],
    },
    ServerDecl {
        name: "rust-analyzer",
        language_id: "rust",
        repo: "rust-lang/rust-analyzer",
        release_tag: "2026-06-22",
        assets: &[
            gz("darwin-arm64", "rust-analyzer-aarch64-apple-darwin.gz"),
            gz("darwin-x64", "rust-analyzer-x86_64-apple-darwin.gz"),
            gz("linux-arm64", "rust-analyzer-aarch64-unknown-linux-gnu.gz"),
            gz("linux-x64", "rust-analyzer-x86_64-unknown-linux-gnu.gz"),
            gz(
                "linux-x64-musl",
                "rust-analyzer-x86_64-unknown-linux-musl.gz",
            ),
            zip(
                "win32-arm64",
                "rust-analyzer-aarch64-pc-windows-msvc.zip",
                "rust-analyzer.exe",
            ),
            zip(
                "win32-x64",
                "rust-analyzer-x86_64-pc-windows-msvc.zip",
                "rust-analyzer.exe",
            ),
        ],
        unsupported: &[],
    },
];

/// The canonical auto-download manifest, joined with the embedded lock.
pub fn manifest() -> &'static BTreeMap<&'static str, ManifestServer> {
    static MANIFEST: OnceLock<BTreeMap<&'static str, ManifestServer>> = OnceLock::new();
    MANIFEST.get_or_init(|| build_manifest(lock::embedded()))
}

fn build_manifest(lock: &lock::Lock) -> BTreeMap<&'static str, ManifestServer> {
    SERVERS
        .iter()
        .map(|decl| {
            let release = lock.release(decl.name, decl.repo, decl.release_tag);
            let expand = |template: &str| template.replace("{tag}", decl.release_tag);
            let platforms = decl
                .assets
                .iter()
                .map(|asset| {
                    let name = expand(asset.asset);
                    let bin_path = asset.bin_path.map(expand);
                    let bin_name = bin_path
                        .as_deref()
                        .and_then(|path| path.rsplit('/').next())
                        .unwrap_or(decl.name)
                        .to_owned();
                    let entry = ManifestAsset {
                        lock: release.and_then(|release| release.asset(&name)).cloned(),
                        asset: name,
                        archive: asset.archive,
                        bin_name,
                        bin_path,
                    };
                    (asset.platform, entry)
                })
                .collect();
            let server = ManifestServer {
                language_id: decl.language_id,
                repo: decl.repo,
                release_tag: decl.release_tag,
                platforms,
                unsupported_platforms: decl.unsupported.iter().copied().collect(),
                release_lock: release.map(|release| release.summary.clone()),
            };
            (decl.name, server)
        })
        .collect()
}

/// The locked identity of one platform's asset: what an install verifies the
/// download against and what its `.ok` marker records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetIdentity {
    pub repo: String,
    pub tag: String,
    pub asset: String,
    pub sha256: String,
}

impl AssetIdentity {
    /// The identity the manifest and lock name now for `name` on `platform`;
    /// `None` for an unknown server or platform, or an asset not in the lock.
    pub fn locked(name: &str, platform: &str) -> Option<Self> {
        let server = manifest_server(name)?;
        Self::of(server, server.platforms.get(platform)?)
    }

    /// The identity of `asset` of `server`; `None` when it is not locked.
    pub fn of(server: &ManifestServer, asset: &ManifestAsset) -> Option<Self> {
        Some(Self {
            repo: server.repo.to_owned(),
            tag: server.release_tag.to_owned(),
            asset: asset.asset.clone(),
            sha256: asset.lock.as_ref()?.sha256.clone(),
        })
    }
}

/// The `.ok` marker for a binary installed from `identity`: its asset identity
/// plus the hash and size of exactly the bytes written.
pub fn cache_marker(identity: &AssetIdentity, binary_sha256: &str, size: u64) -> serde_json::Value {
    serde_json::json!({
        "repo": identity.repo,
        "tag": identity.tag,
        "asset": identity.asset,
        "assetSha256": identity.sha256,
        "binarySha256": binary_sha256,
        "size": size,
    })
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

/// Where a provisioned binary lives once installed:
/// `<root>/<server>/<releaseTag>/<binName>`.
pub fn cached_server_bin_path(root: &Path, name: &str, platform: &str) -> Option<PathBuf> {
    let server = manifest_server(name)?;
    let asset = server.platforms.get(platform)?;
    Some(
        root.join(bare_name(name))
            .join(server.release_tag)
            .join(&asset.bin_name),
    )
}

/// The `.ok` completion marker beside an installed binary.
pub fn marker_path(bin_path: &Path) -> PathBuf {
    let mut s = bin_path.as_os_str().to_os_string();
    s.push(".ok");
    PathBuf::from(s)
}

/// The asset identity, binary hash and size, as the `.ok` marker records them.
struct CacheMarker {
    identity: AssetIdentity,
    binary_sha256: String,
    size: u64,
}

fn read_cache_marker(marker_path: &Path) -> Option<CacheMarker> {
    let text = std::fs::read_to_string(marker_path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let digest = |key: &str| {
        let hex = value.get(key)?.as_str()?;
        (hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| hex.to_ascii_lowercase())
    };
    let text = |key: &str| value.get(key)?.as_str().map(str::to_owned);
    Some(CacheMarker {
        identity: AssetIdentity {
            repo: text("repo")?,
            tag: text("tag")?,
            asset: text("asset")?,
            sha256: digest("assetSha256")?,
        },
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
/// marker names the asset identity the manifest and lock name now and matches
/// the binary's current hash and size. Read-only.
pub fn resolve_cached_server(root: &Path, name: &str, platform: &str) -> Option<PathBuf> {
    let locked = AssetIdentity::locked(name, platform)?;
    let bin_path = cached_server_bin_path(root, name, platform)?;
    let marker = read_cache_marker(&marker_path(&bin_path))?;
    // A binary from another asset (another repository, tag, file, or bytes
    // at the same release-tag path) is stale, even when its own hash matches.
    if marker.identity != locked {
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
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&bin_path)
            == Some(identity)
    {
        return Some(bin_path);
    }
    let bytes = std::fs::read(&bin_path).ok()?;
    #[cfg(test)]
    HASH_CALLS.with(|calls| calls.set(calls.get() + 1));
    let matches =
        bytes.len() as u64 == marker.size && crate::digest::sha256(&bytes) == marker.binary_sha256;
    {
        let mut map = cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
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
mod tests;
