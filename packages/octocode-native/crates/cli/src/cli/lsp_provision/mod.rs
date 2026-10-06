//! LSP language-server provisioning for the native CLI: the download, verify,
//! extract, and install path into the managed cache that discovery reads.
//!
//! Provisioning requires pinned SHA-256 assets and allowed HTTPS hosts on
//! every hop. It writes and marks completed executables atomically under
//! per-target locks. Archive support is limited to `gz` and `zip`.
use octocode_engine::lsp::config::{
    LspDiscoveryOptions, default_server_for_file, detect_language_id, is_command_available,
};
use octocode_engine::lsp::managed::{
    ArchiveKind, ManifestAsset, cached_server_bin_path, manifest, manifest_server, marker_path,
    platform_id, resolve_cached_server, sha256_hex,
};
use octocode_native::runtime::ToolRuntime;
use std::future::Future;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Auto-install policy, from `lsp.autoInstall`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvisionMode {
    Off,
    Prompt,
    Auto,
}

const MAX_REDIRECTS: usize = 5;
const LOCK_STALE: Duration = Duration::from_secs(10 * 60);

/// GitHub / HashiCorp release hosts permitted for downloads and every
/// redirect hop.
const ALLOWED_HOSTS: [&str; 4] = [
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
    "releases.hashicorp.com",
];

/// The configured auto-install policy. Defaults to `Prompt` when unset.
pub fn provision_mode(raw: Option<&str>) -> ProvisionMode {
    match raw.unwrap_or("").trim().to_ascii_lowercase().as_str() {
        "auto" => ProvisionMode::Auto,
        "off" => ProvisionMode::Off,
        _ => ProvisionMode::Prompt,
    }
}

/// True when `url` is https and its host is on the release allowlist.
pub fn host_allowed(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|parsed| {
        parsed.scheme() == "https"
            && parsed
                .host_str()
                .is_some_and(|host| ALLOWED_HOSTS.contains(&host))
    })
}

/// Decode the downloaded asset into the final executable bytes.
fn extract_binary(asset: &ManifestAsset, raw: &[u8]) -> Result<Vec<u8>, String> {
    match asset.archive {
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
    }
}

/// Strip a single leading `./` or `/`.
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

/// An exclusive per-target `.lock` file, removed when dropped, so an error
/// or a panic during the install never leaves it behind.
struct InstallLock(PathBuf);

impl InstallLock {
    /// Acquire `path`, reclaiming a stale lock left by a crashed installer.
    fn acquire(path: PathBuf) -> Option<Self> {
        let create = || {
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .is_ok()
        };
        if create() {
            return Some(Self(path));
        }
        let stale = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .is_ok_and(|mtime| {
                SystemTime::now()
                    .duration_since(mtime)
                    .is_ok_and(|age| age > LOCK_STALE)
            });
        if stale {
            let _ = std::fs::remove_file(&path);
            if create() {
                return Some(Self(path));
            }
        }
        None
    }
}

impl Drop for InstallLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
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
/// supplies the raw asset bytes for a URL (injected so the full verify/
/// extract/install path is testable without network).
pub async fn provision_server<F, Fut>(
    root: &Path,
    name: &str,
    platform: &str,
    mode: ProvisionMode,
    fetch: F,
) -> ProvisionOutcome
where
    F: FnOnce(&'static str) -> Fut,
    Fut: Future<Output = Result<Vec<u8>, String>>,
{
    let Some(server) = manifest_server(name) else {
        return ProvisionOutcome::fail(format!("{name} is not an auto-downloadable server."));
    };
    if let Some(reason) = server.unsupported_platforms.get(platform) {
        return ProvisionOutcome::fail(*reason);
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
        return ProvisionOutcome::fail("bin path has no parent");
    };
    if let Err(e) = std::fs::create_dir_all(dir) {
        return ProvisionOutcome::fail(format!("creating {dir:?}: {e}"));
    }
    let lock_path = dir.join(".lock");
    let Some(_lock) = InstallLock::acquire(lock_path.clone()) else {
        return ProvisionOutcome::fail(format!(
            "Another install of {name} is in progress ({lock_path:?})."
        ));
    };

    // Another process may have finished while this one waited for the lock.
    if let Some(winner) = resolve_cached_server(root, name, platform) {
        return ProvisionOutcome::present(winner, "already-present");
    }
    let downloaded = match fetch(asset.url).await {
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
    ProvisionOutcome::present(bin_path, "downloaded")
}

/// Fetch `url` following redirects manually, re-checking the host allowlist on
/// every hop. Signed release-asset query tokens are never echoed into errors.
async fn fetch_allowlisted(url: &'static str) -> Result<Vec<u8>, String> {
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
        // Cap the download so a mis-redirected HTML page or a hostile/oversized
        // asset cannot OOM the process before the checksum is ever computed.
        const MAX_DOWNLOAD_BYTES: usize = 256 * 1024 * 1024;
        if let Some(len) = response.content_length()
            && len > MAX_DOWNLOAD_BYTES as u64
        {
            return Err(format!(
                "Download exceeds the {MAX_DOWNLOAD_BYTES}-byte limit (Content-Length {len})"
            ));
        }
        use futures_util::StreamExt;
        let mut buffer: Vec<u8> = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("Download failed: {e}"))?;
            if buffer.len().saturating_add(chunk.len()) > MAX_DOWNLOAD_BYTES {
                return Err(format!(
                    "Download exceeds the {MAX_DOWNLOAD_BYTES}-byte limit"
                ));
            }
            buffer.extend_from_slice(&chunk);
        }
        return Ok(buffer);
    }
    Err("Too many redirects".to_string())
}

/// The runtime's resolved language-server discovery settings, so status and
/// installs see what `lspSearch` sees.
fn discovery(runtime: &ToolRuntime) -> LspDiscoveryOptions {
    let execution = runtime.lsp_execution_config();
    execution.discovery(execution.config_path.as_deref().map(PathBuf::from))
}

/// Resolve how `file_path`'s language server would launch, through the same
/// discovery `lspSearch` uses. Without a file, list the managed installs.
fn run_status(
    discovery: &LspDiscoveryOptions,
    root: &Path,
    platform: &str,
    file_path: Option<&str>,
    json: bool,
) -> u8 {
    let Some(path) = file_path else {
        let installed: Vec<serde_json::Value> = manifest()
            .keys()
            .filter_map(|name| {
                resolve_cached_server(root, name, platform)
                    .map(|bin| serde_json::json!({ "name": name, "path": bin }))
            })
            .collect();
        if json {
            return super::write_json(
                &serde_json::json!({ "managedRoot": root, "installed": installed }),
                true,
            );
        }
        println!("LSP status");
        println!("  managed installs: {}", root.display());
        for row in &installed {
            println!(
                "    {}  {}",
                row["name"].as_str().unwrap_or_default(),
                row["path"].as_str().unwrap_or_default()
            );
        }
        println!("  Pass a file path to see how its language server resolves.");
        return 0;
    };

    let workspace =
        octocode_engine::lsp::workspace::resolve_workspace_root_for_file(path.to_owned())
            .unwrap_or_else(|_| {
                std::env::current_dir()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| ".".to_owned())
            });
    let language_id = detect_language_id(path);
    let config = default_server_for_file(path, &workspace, discovery);
    let (server_available, probe_error) =
        match config.as_ref().map(|c| is_command_available(&c.command)) {
            Some(Ok(available)) => (available, None),
            Some(Err(error)) => (false, Some(error)),
            None => (false, None),
        };
    let server_command: Option<String> = config.map(|c| c.command);
    let lang = language_id.as_deref().unwrap_or("unknown");

    // A path has a separator or an extension that contains no separator;
    // anything else is probably a server name passed by mistake.
    let looks_like_path = path.contains('/') || path.contains('\\') || {
        path.rfind('.')
            .is_some_and(|i| i + 1 < path.len() && !path[i + 1..].contains('/'))
    };
    let unresolved = !server_available
        && language_id
            .as_deref()
            .is_none_or(|l| l.is_empty() || l == "plaintext");

    if json {
        return super::write_json(
            &serde_json::json!({
                "filePath": path,
                "languageId": lang,
                "serverAvailable": server_available,
                "serverCommand": server_command,
                "probeError": probe_error,
            }),
            true,
        );
    }

    if unresolved && !looks_like_path {
        eprintln!(
            "  '{path}' looks like a server name, not a file. \
             status resolves a FILE's language \u{2192} server (e.g. src/main.rs); \
             run `lsp-server list` to see supported servers."
        );
    }

    println!("LSP status for {path}");
    println!("  language:  {lang}");
    if server_available {
        let cmd = server_command.as_deref().unwrap_or("unknown");
        println!("  resolved:  {cmd}");
    } else {
        println!("  resolved:  unavailable");
        if let Some(error) = &probe_error {
            println!("  Could not check the server command: {error}");
        }
        if let Some(lang_id) = &language_id {
            println!(
                "  No language server is available for this file (language: {lang_id}). \
                 Install a matching language server or add an lsp-servers.json entry."
            );
        } else {
            println!("  Could not determine language for this file.");
        }
    }
    0
}

/// Dispatch the `lsp-server` subcommand.
pub async fn run(
    runtime: &ToolRuntime,
    action: &str,
    names: Vec<String>,
    all: bool,
    yes: bool,
    force: bool,
    json: bool,
) -> u8 {
    let discovery = discovery(runtime);
    let Some(root) = discovery.managed_root() else {
        eprintln!("The Octocode home is unavailable; cannot locate managed language servers.");
        return 5;
    };
    let platform = platform_id();
    // `names` carries the optional file-path argument for status/which.
    match action {
        "list" => run_list(&root, &platform, json),
        "install" => {
            let targets: Vec<String> = if all {
                manifest().keys().map(|k| (*k).to_owned()).collect()
            } else {
                names
            };
            let mode = if yes || force {
                ProvisionMode::Auto
            } else {
                provision_mode(Some(&runtime.config().resolved.lsp.auto_install))
            };
            run_install(&root, &platform, targets, mode, json, fetch_allowlisted).await
        }
        "uninstall" => run_uninstall(&root, &platform, names, json),
        "clean" => run_clean(&root, yes, json),
        "status" | "which" => run_status(
            &discovery,
            &root,
            &platform,
            names.first().map(String::as_str),
            json,
        ),
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
    for (name, server) in servers {
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

async fn run_install<F, Fut>(
    root: &Path,
    platform: &str,
    targets: Vec<String>,
    mode: ProvisionMode,
    json: bool,
    fetch: F,
) -> u8
where
    F: Fn(&'static str) -> Fut,
    Fut: Future<Output = Result<Vec<u8>, String>>,
{
    if targets.is_empty() {
        eprintln!("Specify a server to install, or use --all.");
        return 2;
    }
    let mut results = Vec::new();
    let mut worst: u8 = 0;
    for name in targets {
        let outcome = provision_server(root, &name, platform, mode, &fetch).await;
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
    // A missing cache is already clean; any other failure must not be
    // reported as success.
    if let Err(error) = std::fs::remove_dir_all(root)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        if json {
            super::write_json(
                &serde_json::json!({ "clean": "failed", "root": root_display, "error": error.to_string() }),
                true,
            );
        } else {
            eprintln!("Could not remove the managed LSP cache at {root_display}: {error}");
        }
        return 5;
    }
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

    #[test]
    fn clean_reports_success_only_when_the_cache_is_gone() {
        let dir = tempfile::tempdir().expect("dir");
        let cache = dir.path().join("lsp");
        std::fs::create_dir_all(cache.join("server")).expect("cache");
        assert_eq!(run_clean(&cache, false, true), 0, "dry run");
        assert!(cache.exists(), "dry run keeps the cache");
        assert_eq!(run_clean(&cache, true, true), 0);
        assert!(!cache.exists());
        assert_eq!(
            run_clean(&cache, true, true),
            0,
            "an absent cache is already clean"
        );
        // A path that is a file cannot be removed as a directory: failure is
        // reported, never "done".
        let file = dir.path().join("not-a-dir");
        std::fs::write(&file, "x").expect("file");
        assert_eq!(run_clean(&file, true, true), 5);
    }
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

    #[tokio::test(flavor = "current_thread")]
    async fn atomic_install_then_resolve_roundtrip() {
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
        let again =
            provision_server(root, "rust-analyzer", platform, ProvisionMode::Auto, never).await;
        assert_eq!(again.source, Some("already-present"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn provision_refuses_on_checksum_mismatch() {
        let dir = tempfile::tempdir().expect("tempdir");
        let outcome = provision_server(
            dir.path(),
            "rust-analyzer",
            "linux-x64",
            ProvisionMode::Auto,
            tampered,
        )
        .await;
        assert!(!outcome.ok);
        assert!(
            outcome
                .error
                .expect("error should be set")
                .contains("Checksum mismatch")
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn provision_off_mode_refuses() {
        let dir = tempfile::tempdir().expect("tempdir");
        let outcome =
            provision_server(dir.path(), "clangd", "linux-x64", ProvisionMode::Off, never).await;
        assert!(!outcome.ok);
        assert!(
            outcome
                .error
                .expect("error should be set")
                .contains("Auto-install is off")
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn provision_unsupported_platform_refuses() {
        let dir = tempfile::tempdir().expect("tempdir");
        let outcome = provision_server(
            dir.path(),
            "clangd",
            "linux-arm64",
            ProvisionMode::Auto,
            never,
        )
        .await;
        assert!(!outcome.ok);
        assert!(
            outcome
                .error
                .expect("error should be set")
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

    fn status(file: Option<&str>, json: bool) -> u8 {
        let dir = tempfile::tempdir().expect("tempdir");
        run_status(
            &LspDiscoveryOptions::default(),
            dir.path(),
            "linux-x64",
            file,
            json,
        )
    }

    #[test]
    fn status_without_a_file_lists_managed_installs() {
        assert_eq!(status(None, false), 0);
        assert_eq!(status(None, true), 0);
    }

    #[test]
    fn status_resolves_known_and_unknown_extensions() {
        assert_eq!(status(Some("src/main.rs"), false), 0);
        assert_eq!(status(Some("main.rs"), true), 0);
        assert_eq!(status(Some("file.unknownxyz"), false), 0);
    }

    fn never(_url: &'static str) -> std::future::Ready<Result<Vec<u8>, String>> {
        panic!("must not fetch")
    }

    fn tampered(_url: &'static str) -> std::future::Ready<Result<Vec<u8>, String>> {
        std::future::ready(Ok(gz(b"tampered")))
    }

    /// The CLI drives a current-thread runtime; installing must not need a
    /// multi-thread one, and the per-target lock is gone afterwards.
    #[tokio::test(flavor = "current_thread")]
    async fn install_runs_on_the_current_thread_runtime() {
        let dir = tempfile::tempdir().expect("tempdir");
        let code = run_install(
            dir.path(),
            "linux-x64",
            vec!["rust-analyzer".to_owned()],
            ProvisionMode::Auto,
            true,
            tampered,
        )
        .await;
        assert_eq!(code, 3);
        let lock = dir.path().join("rust-analyzer/2026-06-22/.lock");
        assert!(!lock.exists());
    }

    /// A panic mid-install still releases the lock, so the next install is
    /// not refused as "in progress" for the stale-lock window.
    #[tokio::test(flavor = "current_thread")]
    async fn a_panicking_install_releases_its_lock() {
        use futures_util::FutureExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let panicked = std::panic::AssertUnwindSafe(provision_server(
            dir.path(),
            "rust-analyzer",
            "linux-x64",
            ProvisionMode::Auto,
            never,
        ))
        .catch_unwind()
        .await;
        assert!(panicked.is_err());
        assert!(!dir.path().join("rust-analyzer/2026-06-22/.lock").exists());
        let retry = provision_server(
            dir.path(),
            "rust-analyzer",
            "linux-x64",
            ProvisionMode::Auto,
            tampered,
        )
        .await;
        assert!(retry.error.expect("error").contains("Checksum mismatch"));
    }

    #[test]
    fn resolve_ignores_binary_without_marker() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let bin = cached_server_bin_path(root, "clangd", "linux-x64").expect("path");
        std::fs::create_dir_all(bin.parent().expect("bin has parent dir")).expect("create dir");
        std::fs::write(&bin, b"unverified").expect("write file");
        // No .ok marker → not trusted.
        assert!(resolve_cached_server(root, "clangd", "linux-x64").is_none());
    }
}
