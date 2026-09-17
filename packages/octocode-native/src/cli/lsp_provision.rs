//! LSP language-server provisioning for the native CLI: manifest data, managed
//! cache resolution, and the download/verify/extract/install path.
//!
//! Faithful port of `octocode-engine/src/lsp/serverManifest.ts` +
//! `serverProvisioner.ts`. Security invariants preserved: pinned-SHA gate
//! (refuse when `sha256` is null), https host allowlist on every hop, atomic
//! temp-write + chmod + rename, `.ok` completion marker written last, per-target
//! lock. Archive support matches TS: `none`, `gz`, `zip` only; `tar.gz`/`tar.xz`
//! return an explicit "install manually" error.
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Archive encodings the manifest can declare. Only `None`/`Gz`/`Zip` are
/// extractable here (parity with TS); tar variants are detect-and-instruct.
/// `None`/`TarGz`/`TarXz` are absent from current manifest data but kept for
/// schema parity with the TS `ArchiveKind` union and its extraction handling.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveKind {
    None,
    Gz,
    Zip,
    TarGz,
    TarXz,
}

/// One platform's downloadable asset for a server.
#[derive(Debug, Clone)]
pub struct ManifestAsset {
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
pub struct ManifestServer {
    pub language_id: &'static str,
    pub repo: &'static str,
    pub release_tag: &'static str,
    pub platforms: BTreeMap<&'static str, ManifestAsset>,
    pub unsupported_platforms: BTreeMap<&'static str, &'static str>,
}

/// Auto-install policy, mirroring `OCTOCODE_LSP_AUTO_INSTALL`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvisionMode {
    Off,
    Prompt,
    Auto,
}

const MAX_REDIRECTS: usize = 5;
const LOCK_STALE: Duration = Duration::from_secs(10 * 60);

/// GitHub / HashiCorp release hosts permitted for downloads (and every redirect
/// hop). Mirrors the TS `ALLOWED_HOSTS` set.
const ALLOWED_HOSTS: [&str; 4] = [
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
    "releases.hashicorp.com",
];

/// The auto-download manifest (source of truth: `serverManifestData.ts`).
pub fn manifest() -> BTreeMap<&'static str, ManifestServer> {
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
pub fn manifest_server(name: &str) -> Option<ManifestServer> {
    let base = Path::new(name)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(name);
    manifest().get(base).cloned()
}

/// Detect the canonical `{os}-{arch}[-musl]` platform id the manifest is keyed on.
pub fn platform_id() -> String {
    let arch = if std::env::consts::ARCH == "x86_64" {
        "x64"
    } else {
        "arm64"
    };
    match std::env::consts::OS {
        "macos" => format!("darwin-{arch}"),
        "windows" => format!("win32-{arch}"),
        _ => {
            let suffix = if is_musl_linux() { "-musl" } else { "" };
            format!("linux-{arch}{suffix}")
        }
    }
}

/// True when the current Linux runtime links musl libc (Alpine etc.).
fn is_musl_linux() -> bool {
    if std::env::consts::OS != "linux" {
        return false;
    }
    std::fs::read_dir("/lib")
        .map(|entries| {
            entries.flatten().any(|e| {
                e.file_name()
                    .to_str()
                    .map(|n| n.starts_with("ld-musl-"))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

/// The configured auto-install policy. Defaults to `Prompt` when unset.
pub fn provision_mode(raw: Option<&str>) -> ProvisionMode {
    match raw.unwrap_or("").trim().to_ascii_lowercase().as_str() {
        "auto" => ProvisionMode::Auto,
        "off" => ProvisionMode::Off,
        _ => ProvisionMode::Prompt,
    }
}

/// Lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// True when `url` is https and its host is on the release allowlist.
pub fn host_allowed(url: &str) -> bool {
    match url::Url::parse(url) {
        Ok(parsed) => {
            parsed.scheme() == "https"
                && parsed
                    .host_str()
                    .map(|h| ALLOWED_HOSTS.contains(&h))
                    .unwrap_or(false)
        }
        Err(_) => false,
    }
}

/// Where a provisioned binary lives once installed:
/// `<root>/<server>/<releaseTag>/<binName>`.
pub fn cached_server_bin_path(root: &Path, name: &str, platform: &str) -> Option<PathBuf> {
    let server = manifest_server(name)?;
    let asset = server.platforms.get(platform)?;
    let base = Path::new(name)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(name);
    Some(
        root.join(base)
            .join(server.release_tag)
            .join(asset.bin_name),
    )
}

/// A `.ok` completion marker: the binary's own hash + size.
struct CacheMarker {
    binary_sha256: String,
    size: u64,
}

fn read_cache_marker(marker_path: &Path) -> Option<CacheMarker> {
    let text = std::fs::read_to_string(marker_path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let binary_sha256 = value.get("binarySha256")?.as_str()?.to_string();
    if binary_sha256.len() != 64 || !binary_sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let size = value.get("size")?.as_u64()?;
    Some(CacheMarker {
        binary_sha256,
        size,
    })
}

/// Return an absolute path only when a managed binary is present AND its `.ok`
/// marker matches the binary's current hash and size (read-only, always safe).
pub fn resolve_cached_server(root: &Path, name: &str, platform: &str) -> Option<PathBuf> {
    let bin_path = cached_server_bin_path(root, name, platform)?;
    if !bin_path.exists() {
        return None;
    }
    let marker = read_cache_marker(&marker_path(&bin_path))?;
    let bytes = std::fs::read(&bin_path).ok()?;
    if bytes.len() as u64 != marker.size {
        return None;
    }
    if sha256_hex(&bytes) == marker.binary_sha256 {
        Some(bin_path)
    } else {
        None
    }
}

fn marker_path(bin_path: &Path) -> PathBuf {
    let mut s = bin_path.as_os_str().to_os_string();
    s.push(".ok");
    PathBuf::from(s)
}

/// Decode the downloaded asset into the final executable bytes.
pub fn extract_binary(asset: &ManifestAsset, raw: &[u8]) -> Result<Vec<u8>, String> {
    match asset.archive {
        ArchiveKind::None => Ok(raw.to_vec()),
        ArchiveKind::Gz => {
            use flate2::read::GzDecoder;
            let mut decoder = GzDecoder::new(raw);
            let mut out = Vec::new();
            decoder
                .read_to_end(&mut out)
                .map_err(|e| format!("gzip decode failed: {e}"))?;
            Ok(out)
        }
        ArchiveKind::Zip => {
            let bin_path = asset
                .bin_path
                .ok_or_else(|| "zip asset is missing binPath in manifest".to_string())?;
            extract_from_zip(raw, bin_path).ok_or_else(|| {
                format!(
                    "Could not find '{bin_path}' in zip archive - try installing via your package manager."
                )
            })
        }
        ArchiveKind::TarGz | ArchiveKind::TarXz => Err(
            "Archive format is not supported; install the server via your package manager."
                .to_string(),
        ),
    }
}

/// Strip a single leading `./` or `/` (mirrors the TS `/^\.?\//` normalization).
fn strip_lead(s: &str) -> &str {
    if let Some(rest) = s.strip_prefix("./") {
        rest
    } else if let Some(rest) = s.strip_prefix('/') {
        rest
    } else {
        s
    }
}

/// Extract the entry matching `target` (after leading-`./` normalization) from a
/// zip archive. Returns its uncompressed bytes.
pub fn extract_from_zip(bytes: &[u8], target: &str) -> Option<Vec<u8>> {
    let normalized_target = strip_lead(target);
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).ok()?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).ok()?;
        let name = entry.name().to_string();
        if strip_lead(&name) == normalized_target {
            let mut out = Vec::new();
            entry.read_to_end(&mut out).ok()?;
            return Some(out);
        }
    }
    None
}

/// Atomically install `bytes` at `bin_path` (temp write + chmod + rename) and
/// write the `.ok` completion marker last.
pub fn atomic_install(bin_path: &Path, bytes: &[u8], asset_sha256: &str) -> Result<(), String> {
    let dir = bin_path
        .parent()
        .ok_or_else(|| format!("bin path has no parent: {bin_path:?}"))?;
    std::fs::create_dir_all(dir).map_err(|e| format!("creating {dir:?}: {e}"))?;

    let tmp_path = {
        let mut s = bin_path.as_os_str().to_os_string();
        s.push(format!(".tmp-{}", std::process::id()));
        PathBuf::from(s)
    };
    std::fs::write(&tmp_path, bytes).map_err(|e| format!("writing {tmp_path:?}: {e}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("chmod {tmp_path:?}: {e}"))?;
    }

    std::fs::rename(&tmp_path, bin_path)
        .map_err(|e| format!("renaming {tmp_path:?} to {bin_path:?}: {e}"))?;

    let marker = serde_json::json!({
        "assetSha256": asset_sha256,
        "binarySha256": sha256_hex(bytes),
        "size": bytes.len(),
    });
    std::fs::write(marker_path(bin_path), marker.to_string())
        .map_err(|e| format!("writing completion marker: {e}"))?;
    Ok(())
}

/// Remove a server from the managed cache only (never external installs). Guards
/// deletion to inside `root`.
pub fn uninstall_server(root: &Path, name: &str, platform: &str) -> bool {
    let Some(bin_path) = cached_server_bin_path(root, name, platform) else {
        return false;
    };
    // <root>/<server>
    let Some(server_dir) = bin_path.parent().and_then(Path::parent) else {
        return false;
    };
    if !server_dir.exists() {
        return false;
    }
    let (Ok(server_dir), Ok(root)) = (server_dir.canonicalize(), root.canonicalize()) else {
        return false;
    };
    if !server_dir.starts_with(&root) || server_dir == root {
        return false;
    }
    std::fs::remove_dir_all(&server_dir).is_ok()
}

/// Try to acquire a `.lock` file; reclaim a stale lock from a crashed installer.
fn acquire_lock(lock_path: &Path) -> bool {
    use std::fs::OpenOptions;
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(lock_path)
    {
        Ok(_) => true,
        Err(_) => {
            let stale = std::fs::metadata(lock_path)
                .and_then(|m| m.modified())
                .map(|mtime| {
                    SystemTime::now()
                        .duration_since(mtime)
                        .map(|age| age > LOCK_STALE)
                        .unwrap_or(false)
                })
                .unwrap_or(false);
            if stale {
                let _ = std::fs::remove_file(lock_path);
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(lock_path)
                    .is_ok()
            } else {
                false
            }
        }
    }
}

/// Outcome of a provision attempt.
#[derive(Debug, PartialEq, Eq)]
pub struct ProvisionOutcome {
    pub ok: bool,
    pub path: Option<String>,
    pub source: Option<&'static str>,
    pub error: Option<String>,
}

impl ProvisionOutcome {
    fn fail(error: impl Into<String>) -> Self {
        ProvisionOutcome {
            ok: false,
            path: None,
            source: None,
            error: Some(error.into()),
        }
    }
    fn present(path: PathBuf, source: &'static str) -> Self {
        ProvisionOutcome {
            ok: true,
            path: Some(path.to_string_lossy().into_owned()),
            source: Some(source),
            error: None,
        }
    }
}

/// Provision `name` into the managed cache under `root`. Idempotent. `fetch`
/// supplies the raw asset bytes for a URL (injected so the full verify/extract/
/// install path is testable without network).
pub fn provision_server<F>(
    root: &Path,
    name: &str,
    platform: &str,
    mode: ProvisionMode,
    fetch: F,
) -> ProvisionOutcome
where
    F: FnOnce(&str) -> Result<Vec<u8>, String>,
{
    let Some(server) = manifest_server(name) else {
        return ProvisionOutcome::fail(format!("{name} is not an auto-downloadable server."));
    };
    if let Some(reason) = server.unsupported_platforms.get(platform) {
        return ProvisionOutcome::fail(reason.to_string());
    }
    let Some(asset) = server.platforms.get(platform) else {
        return ProvisionOutcome::fail(format!("No {name} asset for platform {platform}."));
    };

    if let Some(existing) = resolve_cached_server(root, name, platform) {
        return ProvisionOutcome::present(existing, "already-present");
    }
    if mode == ProvisionMode::Off {
        return ProvisionOutcome::fail(
            "Auto-install is off. Set OCTOCODE_LSP_AUTO_INSTALL=prompt|auto (or pass --yes) to allow downloading.",
        );
    }
    let Some(expected_sha) = asset.sha256 else {
        return ProvisionOutcome::fail(format!(
            "{name} has no pinned sha256 in the manifest yet; refusing to download unverified bytes."
        ));
    };
    if !host_allowed(asset.url) {
        return ProvisionOutcome::fail("Blocked non-allowlisted/insecure download host.");
    }

    let Some(bin_path) = cached_server_bin_path(root, name, platform) else {
        return ProvisionOutcome::fail(format!("Cannot compute cache path for {name}."));
    };
    let Some(dir) = bin_path.parent() else {
        return ProvisionOutcome::fail("bin path has no parent".to_string());
    };
    if let Err(e) = std::fs::create_dir_all(dir) {
        return ProvisionOutcome::fail(format!("creating {dir:?}: {e}"));
    }
    let lock_path = dir.join(".lock");
    if !acquire_lock(&lock_path) {
        return ProvisionOutcome::fail(format!(
            "Another install of {name} is in progress ({lock_path:?})."
        ));
    }

    let result = (|| {
        // Re-check after lock (another process may have finished).
        if let Some(winner) = resolve_cached_server(root, name, platform) {
            return ProvisionOutcome::present(winner, "already-present");
        }
        let downloaded = match fetch(asset.url) {
            Ok(bytes) => bytes,
            Err(e) => return ProvisionOutcome::fail(e),
        };
        let actual = sha256_hex(&downloaded);
        if actual != expected_sha {
            return ProvisionOutcome::fail(format!(
                "Checksum mismatch for {name}: expected {expected_sha}, got {actual}."
            ));
        }
        let extracted = match extract_binary(asset, &downloaded) {
            Ok(bytes) => bytes,
            Err(e) => return ProvisionOutcome::fail(e),
        };
        if let Err(e) = atomic_install(&bin_path, &extracted, expected_sha) {
            return ProvisionOutcome::fail(e);
        }
        ProvisionOutcome::present(bin_path.clone(), "downloaded")
    })();

    let _ = std::fs::remove_file(&lock_path);
    result
}

/// The OS home directory (`HOME`/`USERPROFILE`), falling back to `.`.
fn os_home() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// Root of the managed server cache: `OCTOCODE_LSP_CACHE_DIR` override else
/// `<octocode-home>/lsp`.
pub fn managed_cache_root() -> PathBuf {
    if let Ok(override_dir) = std::env::var("OCTOCODE_LSP_CACHE_DIR") {
        let trimmed = override_dir.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    let env: BTreeMap<String, String> = std::env::vars().collect();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    octocode_native::config::octocode_home(&env, &cwd, &os_home()).join("lsp")
}

/// Fetch `url` following redirects manually, re-checking the host allowlist on
/// every hop. Signed release-asset query tokens are never echoed into errors.
async fn fetch_allowlisted(url: &str) -> Result<Vec<u8>, String> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| format!("http client init failed: {e}"))?;
    let mut current = url.to_string();
    for _ in 0..=MAX_REDIRECTS {
        if !host_allowed(&current) {
            let host = url::Url::parse(&current)
                .ok()
                .and_then(|u| u.host_str().map(str::to_string))
                .unwrap_or_else(|| "(unparseable url)".to_string());
            return Err(format!("Blocked non-allowlisted/insecure host: {host}"));
        }
        let response = client
            .get(&current)
            .send()
            .await
            .map_err(|e| format!("Download failed: {e}"))?;
        let status = response.status();
        if status.is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| "Redirect without Location header".to_string())?;
            let next = url::Url::parse(&current)
                .map_err(|e| format!("invalid url: {e}"))?
                .join(location)
                .map_err(|e| format!("invalid redirect target: {e}"))?;
            current = next.to_string();
            continue;
        }
        if !status.is_success() {
            return Err(format!("Download failed: HTTP {}", status.as_u16()));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|e| format!("Download failed: {e}"))?;
        return Ok(bytes.to_vec());
    }
    Err("Too many redirects".to_string())
}

/// Dispatch the `lsp-server` subcommand.
pub async fn run(
    action: &str,
    names: Vec<String>,
    all: bool,
    yes: bool,
    force: bool,
    json: bool,
) -> u8 {
    let root = managed_cache_root();
    let platform = platform_id();
    match action {
        "list" => run_list(&root, &platform, json),
        "install" => run_install(&root, &platform, names, all, yes, force, json).await,
        "uninstall" | "remove" => run_uninstall(&root, &platform, names, json),
        "clean" => run_clean(&root, yes, json),
        other => {
            eprintln!("Unknown lsp-server subcommand: {other}");
            2
        }
    }
}

fn run_list(root: &Path, platform: &str, json: bool) -> u8 {
    let servers = manifest();
    if json {
        let rows: Vec<serde_json::Value> = servers
            .iter()
            .map(|(name, server)| {
                let installed = resolve_cached_server(root, name, platform).is_some();
                serde_json::json!({
                    "name": name,
                    "languageId": server.language_id,
                    "releaseTag": server.release_tag,
                    "repo": server.repo,
                    "status": if installed { "installed" } else { "not installed" },
                })
            })
            .collect();
        return super::write_json(&serde_json::json!({ "servers": rows }), true);
    }
    println!("Auto-downloadable language servers");
    for (name, server) in &servers {
        let installed = resolve_cached_server(root, name, platform).is_some();
        let status = if installed {
            "installed (managed cache)"
        } else {
            "not installed"
        };
        println!("  {:<12} {:<16} {status}", server.language_id, name);
    }
    0
}

async fn run_install(
    root: &Path,
    platform: &str,
    names: Vec<String>,
    all: bool,
    yes: bool,
    force: bool,
    json: bool,
) -> u8 {
    let targets: Vec<String> = if all {
        manifest().keys().map(|k| k.to_string()).collect()
    } else {
        names
    };
    if targets.is_empty() {
        eprintln!("Specify a server to install, or use --all.");
        return 2;
    }
    let mode = if yes || force {
        ProvisionMode::Auto
    } else {
        provision_mode(std::env::var("OCTOCODE_LSP_AUTO_INSTALL").ok().as_deref())
    };
    let mut results = Vec::new();
    let mut worst: u8 = 0;
    for name in targets {
        // Pre-fetch outside the sync provisioner via a blocking bridge so the
        // resolve/lock/verify/install steps stay synchronous and testable.
        let outcome = provision_server(root, &name, platform, mode, |url| {
            let url = url.to_string();
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(fetch_allowlisted(&url))
            })
        });
        if outcome.ok {
            if !json {
                println!(
                    "{name}: {} -> {}",
                    outcome.source.unwrap_or("ok"),
                    outcome.path.clone().unwrap_or_default()
                );
            }
        } else {
            worst = 3;
            if !json {
                println!("{name}: {}", outcome.error.clone().unwrap_or_default());
            }
        }
        results.push(serde_json::json!({
            "name": name,
            "ok": outcome.ok,
            "source": outcome.source,
            "path": outcome.path,
            "error": outcome.error,
        }));
    }
    if json {
        super::write_json(&serde_json::json!({ "install": results }), true);
    }
    worst
}

fn run_uninstall(root: &Path, platform: &str, names: Vec<String>, json: bool) -> u8 {
    if names.is_empty() {
        eprintln!("Specify a server to uninstall.");
        return 2;
    }
    let mut results = Vec::new();
    for name in &names {
        let removed = uninstall_server(root, name, platform);
        if !json {
            if removed {
                println!("{name}: removed from managed cache");
            } else {
                println!("{name}: not in managed cache (nothing to remove)");
            }
        }
        results.push(serde_json::json!({ "name": name, "removed": removed }));
    }
    if json {
        super::write_json(&serde_json::json!({ "uninstall": results }), true);
    }
    0
}

fn run_clean(root: &Path, yes: bool, json: bool) -> u8 {
    let root_display = root.to_string_lossy().into_owned();
    if !yes {
        if json {
            super::write_json(
                &serde_json::json!({ "clean": "dry-run", "root": root_display }),
                true,
            );
        } else {
            println!(
                "Would remove the managed LSP cache at {root_display}. Re-run with --yes to confirm."
            );
        }
        return 0;
    }
    let _ = std::fs::remove_dir_all(root);
    if json {
        super::write_json(
            &serde_json::json!({ "clean": "done", "root": root_display }),
            true,
        );
    } else {
        println!("cleaned: {root_display}");
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::GzEncoder};
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn gz(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(bytes).expect("gz write");
        encoder.finish().expect("gz finish")
    }

    fn zip_with(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(Cursor::new(&mut buf));
            let opts =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            for (name, data) in entries {
                writer.start_file(*name, opts).expect("start_file");
                writer.write_all(data).expect("zip write");
            }
            writer.finish().expect("zip finish");
        }
        buf
    }

    #[test]
    fn manifest_has_clangd_and_rust_analyzer() {
        let m = manifest();
        assert!(m.contains_key("clangd"));
        assert!(m.contains_key("rust-analyzer"));
        let ra = &m["rust-analyzer"];
        assert_eq!(ra.language_id, "rust");
        assert_eq!(ra.release_tag, "2026-06-22");
        assert_eq!(ra.platforms["linux-x64"].archive, ArchiveKind::Gz);
        assert_eq!(
            m["clangd"].platforms["darwin-arm64"].archive,
            ArchiveKind::Zip
        );
    }

    #[test]
    fn sha256_matches_known_vector() {
        // echo -n "abc" | sha256sum
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn host_allowlist_enforces_https_and_hosts() {
        assert!(host_allowed(
            "https://github.com/clangd/clangd/releases/download/x.zip"
        ));
        assert!(host_allowed(
            "https://release-assets.githubusercontent.com/foo?token=abc"
        ));
        assert!(!host_allowed("http://github.com/foo")); // not https
        assert!(!host_allowed("https://evil.example.com/foo")); // not allowlisted
        assert!(!host_allowed("not a url"));
    }

    #[test]
    fn provision_mode_parses() {
        assert_eq!(provision_mode(Some("auto")), ProvisionMode::Auto);
        assert_eq!(provision_mode(Some(" OFF ")), ProvisionMode::Off);
        assert_eq!(provision_mode(None), ProvisionMode::Prompt);
        assert_eq!(provision_mode(Some("nonsense")), ProvisionMode::Prompt);
    }

    #[test]
    fn extract_gz_roundtrips() {
        let payload = b"#!/bin/sh\necho rust-analyzer\n";
        let asset = ManifestAsset {
            url: "u",
            archive: ArchiveKind::Gz,
            bin_name: "rust-analyzer",
            bin_path: None,
            sha256: None,
        };
        let out = extract_binary(&asset, &gz(payload)).expect("gz extract");
        assert_eq!(out, payload);
    }

    #[test]
    fn extract_zip_finds_entry_by_normalized_path() {
        let payload = b"clangd-binary";
        let archive = zip_with(&[
            ("clangd_22.1.0/README", b"readme"),
            ("clangd_22.1.0/bin/clangd", payload),
        ]);
        // exact match
        assert_eq!(
            extract_from_zip(&archive, "clangd_22.1.0/bin/clangd").as_deref(),
            Some(payload.as_slice())
        );
        // leading ./ normalized on the target
        assert_eq!(
            extract_from_zip(&archive, "./clangd_22.1.0/bin/clangd").as_deref(),
            Some(payload.as_slice())
        );
        // missing entry
        assert!(extract_from_zip(&archive, "nope").is_none());
    }

    #[test]
    fn extract_zip_missing_binpath_errors() {
        let asset = ManifestAsset {
            url: "u",
            archive: ArchiveKind::Zip,
            bin_name: "clangd",
            bin_path: None,
            sha256: None,
        };
        assert!(extract_binary(&asset, &zip_with(&[("x", b"y")])).is_err());
    }

    #[test]
    fn tar_archives_are_unsupported() {
        let asset = ManifestAsset {
            url: "u",
            archive: ArchiveKind::TarGz,
            bin_name: "x",
            bin_path: None,
            sha256: None,
        };
        assert!(extract_binary(&asset, b"whatever").is_err());
    }

    #[test]
    fn atomic_install_then_resolve_roundtrip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let platform = "linux-x64";
        let bin = cached_server_bin_path(root, "rust-analyzer", platform).expect("bin path");
        // atomic_install is the exact step provision_server runs after verify+extract.
        atomic_install(&bin, b"rust-analyzer-bytes", "pinned-asset-sha").expect("install");
        assert_eq!(
            std::fs::read(&bin).expect("read bin"),
            b"rust-analyzer-bytes"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&bin).expect("meta").permissions().mode();
            assert_eq!(mode & 0o111, 0o111, "installed binary is executable");
        }
        // Marker present + trusted → resolve returns the path.
        assert_eq!(
            resolve_cached_server(root, "rust-analyzer", platform),
            Some(bin.clone())
        );
        // provision_server short-circuits to already-present (fetch must not run).
        let again = provision_server(
            root,
            "rust-analyzer",
            platform,
            ProvisionMode::Auto,
            |_url| panic!("must not fetch when already present"),
        );
        assert_eq!(again.source, Some("already-present"));
    }

    #[test]
    fn provision_refuses_on_checksum_mismatch() {
        let dir = tempfile::tempdir().expect("tempdir");
        let outcome = provision_server(
            dir.path(),
            "rust-analyzer",
            "linux-x64",
            ProvisionMode::Auto,
            |_url| {
                Ok(gz(b"tampered")) // hash will not match the pinned manifest sha
            },
        );
        assert!(!outcome.ok);
        assert!(outcome.error.unwrap().contains("Checksum mismatch"));
    }

    #[test]
    fn provision_off_mode_refuses() {
        let dir = tempfile::tempdir().expect("tempdir");
        let outcome = provision_server(
            dir.path(),
            "clangd",
            "linux-x64",
            ProvisionMode::Off,
            |_url| panic!("must not fetch in off mode"),
        );
        assert!(!outcome.ok);
        assert!(outcome.error.unwrap().contains("Auto-install is off"));
    }

    #[test]
    fn provision_unsupported_platform_refuses() {
        let dir = tempfile::tempdir().expect("tempdir");
        let outcome = provision_server(
            dir.path(),
            "clangd",
            "linux-arm64",
            ProvisionMode::Auto,
            |_url| panic!("must not fetch for unsupported platform"),
        );
        assert!(!outcome.ok);
        assert!(
            outcome
                .error
                .unwrap()
                .contains("no linux-arm64 release asset")
        );
    }

    #[test]
    fn uninstall_removes_only_within_cache_root() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let bin = cached_server_bin_path(root, "rust-analyzer", "linux-x64").expect("bin path");
        atomic_install(&bin, b"bytes", "pinned-asset-sha").expect("install");
        assert!(resolve_cached_server(root, "rust-analyzer", "linux-x64").is_some());
        assert!(uninstall_server(root, "rust-analyzer", "linux-x64"));
        assert!(resolve_cached_server(root, "rust-analyzer", "linux-x64").is_none());
        // Second uninstall is a no-op (nothing to remove).
        assert!(!uninstall_server(root, "rust-analyzer", "linux-x64"));
    }

    #[test]
    fn resolve_ignores_binary_without_marker() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let bin = cached_server_bin_path(root, "clangd", "linux-x64").expect("path");
        std::fs::create_dir_all(bin.parent().unwrap()).unwrap();
        std::fs::write(&bin, b"unverified").unwrap();
        // No .ok marker → not trusted.
        assert!(resolve_cached_server(root, "clangd", "linux-x64").is_none());
    }
}
