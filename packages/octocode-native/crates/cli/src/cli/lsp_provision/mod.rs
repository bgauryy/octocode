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
    platform_id, resolve_cached_server,
};
use octocode_native::config::LspAutoInstall;
use octocode_native::runtime::ToolRuntime;
use std::future::Future;
use std::io::{BufReader, Read, Seek};
use std::path::{Path, PathBuf};
use std::time::Duration;

const MAX_REDIRECTS: usize = 5;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const READ_TIMEOUT: Duration = Duration::from_secs(60);
/// Whole-download deadline.
const DOWNLOAD_DEADLINE: Duration = Duration::from_secs(5 * 60);
/// How long a second installer waits for the first: the first's whole download
/// plus margin, so a live install is always outwaited, never raced.
const PEER_INSTALL_WAIT: Duration = Duration::from_secs(DOWNLOAD_DEADLINE.as_secs() + 30);
const PEER_INSTALL_POLL: Duration = Duration::from_millis(250);

/// GitHub / HashiCorp release hosts permitted for downloads and every
/// redirect hop.
const ALLOWED_HOSTS: [&str; 4] = [
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
    "releases.hashicorp.com",
];

/// True when `url` is https and its host is on the release allowlist.
pub fn host_allowed(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|parsed| {
        parsed.scheme() == "https"
            && parsed
                .host_str()
                .is_some_and(|host| ALLOWED_HOSTS.contains(&host))
    })
}

/// Largest decompressed binary accepted; past it the archive is refused.
const MAX_EXTRACTED_BYTES: u64 = 512 * 1024 * 1024;
/// Largest release asset downloaded; a mis-redirected page or a hostile asset
/// stops here, before its checksum is computed.
const MAX_DOWNLOAD_BYTES: u64 = 256 * 1024 * 1024;
/// Suffix of the file a download is spooled into, beside the binary it becomes.
const SPOOL_SUFFIX: &str = "download";

/// Check the spooled download of `name` against `expected_sha`, then extract
/// the executable from it straight into the cache. The download is read from
/// disk twice and never held in memory.
fn verify_and_install(
    name: &str,
    asset: &ManifestAsset,
    download: &Path,
    bin_path: &Path,
    expected_sha: &str,
    limit: u64,
) -> Result<(), String> {
    let read_error = |e: std::io::Error| format!("Reading the download failed: {e}");
    let mut file = std::fs::File::open(download).map_err(read_error)?;
    let (_, actual) =
        octocode_engine::digest::copy_hashed(&mut file, std::io::sink()).map_err(read_error)?;
    if actual != expected_sha {
        return Err(format!(
            "Checksum mismatch for {name}: expected {expected_sha}, got {actual}."
        ));
    }
    file.rewind().map_err(read_error)?;
    let file = BufReader::new(file);
    match asset.archive {
        ArchiveKind::Gz => install_binary(
            bin_path,
            flate2::read::GzDecoder::new(file),
            limit,
            expected_sha,
        ),
        ArchiveKind::Zip => {
            let target = asset
                .bin_path
                .ok_or_else(|| "zip asset is missing binPath in manifest".to_string())?;
            let corrupt =
                |e: zip::result::ZipError| format!("Downloaded zip archive is corrupt: {e}");
            let mut archive = zip::ZipArchive::new(file).map_err(corrupt)?;
            let index = zip_entry_index(&mut archive, target)?.ok_or_else(|| {
                format!(
                    "Could not find '{target}' in zip archive - try installing via your package manager."
                )
            })?;
            let entry = archive.by_index(index).map_err(corrupt)?;
            install_binary(bin_path, entry, limit, expected_sha)
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

/// Index of the zip entry matching `target` after leading-`./` normalization;
/// `Ok(None)` when there is no such entry.
fn zip_entry_index<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    target: &str,
) -> Result<Option<usize>, String> {
    let normalized_target = strip_lead(target);
    for i in 0..archive.len() {
        let entry = archive
            .by_index_raw(i)
            .map_err(|e| format!("Downloaded zip archive is corrupt: {e}"))?;
        // An entry name that does not decode cannot equal the UTF-8 target.
        if entry
            .name()
            .is_ok_and(|name| strip_lead(&name) == normalized_target)
        {
            return Ok(Some(i));
        }
    }
    Ok(None)
}

/// Stream the executable from `binary` into `bin_path` atomically, owner-only,
/// refusing it past `limit` bytes, then write the `.ok` completion marker last
/// with the digest of exactly the bytes written.
fn install_binary(
    bin_path: &Path,
    binary: impl Read,
    limit: u64,
    asset_sha256: &str,
) -> Result<(), String> {
    let dir = bin_path
        .parent()
        .ok_or_else(|| format!("bin path has no parent: {bin_path:?}"))?;
    octocode_io::atomic::create_private_dir_all(dir)
        .map_err(|e| format!("creating {dir:?}: {e}"))?;

    let (size, binary_sha256) =
        octocode_io::atomic::write_atomic_with(bin_path, 0o700, false, |file| {
            let copied = octocode_engine::digest::copy_hashed(binary.take(limit + 1), file)?;
            if copied.0 > limit {
                return Err(std::io::Error::other(format!(
                    "Archive output exceeds the {limit}-byte limit."
                )));
            }
            Ok(copied)
        })
        .map_err(|e| format!("installing {bin_path:?}: {e}"))?;

    let marker = serde_json::json!({
        "assetSha256": asset_sha256,
        "binarySha256": binary_sha256,
        "size": size,
    });
    octocode_io::atomic::write_atomic_mode(
        &marker_path(bin_path),
        marker.to_string().as_bytes(),
        0o600,
        false,
    )
    .map_err(|e| format!("writing completion marker: {e}"))?;
    Ok(())
}

/// Remove the spool and atomic-write temporaries an interrupted installer left
/// in `dir`. Only installers holding the directory's lock write there, so none
/// of them is still in use.
fn sweep_interrupted_installs(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let leftover = name.starts_with('.')
            && (name.ends_with(&format!(".{SPOOL_SUFFIX}")) || name.ends_with(".tmp"));
        if leftover {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// The spooled download, removed however provisioning ends.
struct Spool(PathBuf);

impl Drop for Spool {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
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

/// Take the per-target install lock, waiting up to `wait` for a concurrent
/// installer; the caller then re-checks the cache, so a finished peer's install
/// is reused rather than downloaded again. It is an advisory `flock` the kernel
/// releases when its holder exits, so a crashed installer never leaves it stale.
async fn acquire_or_wait(
    path: PathBuf,
    wait: Duration,
    poll: Duration,
) -> Option<octocode_io::lock::LockedFile> {
    tokio::task::spawn_blocking(move || {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .ok()?;
        octocode_io::lock::lock_within(file, wait, poll).ok()
    })
    .await
    .ok()
    .flatten()
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
/// writes the raw asset for a URL into the spool file it is given and hands
/// the file back (injected so the full verify/extract/install path is testable
/// without network).
pub async fn provision_server<F, Fut>(
    root: &Path,
    name: &str,
    platform: &str,
    mode: LspAutoInstall,
    fetch: F,
) -> ProvisionOutcome
where
    F: FnOnce(&'static str, tokio::fs::File) -> Fut,
    Fut: Future<Output = Result<tokio::fs::File, String>>,
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
    if mode == LspAutoInstall::Off {
        return ProvisionOutcome::fail(
            "Auto-install is off. Set OCTOCODE_LSP_AUTO_INSTALL=prompt|auto (or pass --force) to allow downloading.",
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
    if let Err(e) = octocode_io::atomic::create_private_dir_all(dir) {
        return ProvisionOutcome::fail(format!("creating {dir:?}: {e}"));
    }
    let lock_path = dir.join(".lock");
    let Some(_lock) =
        acquire_or_wait(lock_path.clone(), PEER_INSTALL_WAIT, PEER_INSTALL_POLL).await
    else {
        return ProvisionOutcome::fail(format!(
            "Another install of {name} is still in progress ({lock_path:?})."
        ));
    };

    // Another process may have finished while this one waited for the lock.
    if let Some(winner) = resolve_cached_server(root, name, platform) {
        return ProvisionOutcome::present(winner, "already-present");
    }
    sweep_interrupted_installs(dir);
    let spool = match octocode_io::atomic::unique_sibling(&bin_path, SPOOL_SUFFIX) {
        Ok(path) => Spool(path),
        Err(e) => return ProvisionOutcome::fail(format!("Cannot create the download file: {e}")),
    };
    let file = match octocode_io::atomic::create_new(&spool.0, 0o600) {
        Ok(file) => tokio::fs::File::from_std(file),
        Err(e) => return ProvisionOutcome::fail(format!("Cannot create the download file: {e}")),
    };
    match fetch(asset.url, file).await {
        // Flushing waits for the last write in flight before the file is read.
        Ok(mut file) => {
            if let Err(e) = tokio::io::AsyncWriteExt::flush(&mut file).await {
                return ProvisionOutcome::fail(format!("Writing the download failed: {e}"));
            }
        }
        Err(e) => return ProvisionOutcome::fail(e),
    }
    let (server, download, target) = (name.to_owned(), spool.0.clone(), bin_path.clone());
    let installed = tokio::task::spawn_blocking(move || {
        verify_and_install(
            &server,
            asset,
            &download,
            &target,
            expected_sha,
            MAX_EXTRACTED_BYTES,
        )
    })
    .await;
    match installed {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return ProvisionOutcome::fail(e),
        Err(_) => {
            return ProvisionOutcome::fail(format!("Installing {name} stopped unexpectedly."));
        }
    }
    ProvisionOutcome::present(bin_path, "downloaded")
}

/// HTTP client for release downloads: redirects are followed by hand, and
/// connect and per-read stalls are bounded.
fn download_client(read_timeout: Duration) -> Result<reqwest::Client, String> {
    octocode_io::http::client(&octocode_io::http::ClientPolicy {
        connect_timeout: Some(CONNECT_TIMEOUT),
        read_timeout: Some(read_timeout),
        ..octocode_io::http::ClientPolicy::default()
    })
    .map_err(|e| format!("http client init failed: {e}"))
}

/// Download `url` into `file`, following redirects manually and re-checking
/// the host allowlist on every hop, within `DOWNLOAD_DEADLINE`. Signed
/// release-asset query tokens are never echoed into errors.
async fn fetch_allowlisted(
    url: &'static str,
    file: tokio::fs::File,
) -> Result<tokio::fs::File, String> {
    tokio::time::timeout(DOWNLOAD_DEADLINE, fetch_following_redirects(url, file))
        .await
        .unwrap_or_else(|_| {
            Err(format!(
                "Download timed out after {}s",
                DOWNLOAD_DEADLINE.as_secs()
            ))
        })
}

/// A transport error without its request URL: release-asset redirects carry
/// signed query tokens, and reqwest's message would print them.
fn download_error(error: reqwest::Error) -> String {
    format!("Download failed: {}", error.without_url())
}

async fn fetch_following_redirects(
    url: &'static str,
    mut file: tokio::fs::File,
) -> Result<tokio::fs::File, String> {
    let client = download_client(READ_TIMEOUT)?;
    let mut current = url.to_string();
    for _ in 0..=MAX_REDIRECTS {
        if !host_allowed(&current) {
            let host = url::Url::parse(&current)
                .ok()
                .and_then(|u| u.host_str().map(str::to_string))
                .unwrap_or_else(|| "(unparseable url)".to_string());
            return Err(format!("Blocked non-allowlisted/insecure host: {host}"));
        }
        let response = client.get(&current).send().await.map_err(download_error)?;
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
        if let Some(len) = response.content_length()
            && len > MAX_DOWNLOAD_BYTES
        {
            return Err(format!(
                "Download exceeds the {MAX_DOWNLOAD_BYTES}-byte limit (Content-Length {len})"
            ));
        }
        use futures_util::StreamExt;
        use tokio::io::AsyncWriteExt;
        let mut written = 0_u64;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(download_error)?;
            written = written.saturating_add(chunk.len() as u64);
            if written > MAX_DOWNLOAD_BYTES {
                return Err(format!(
                    "Download exceeds the {MAX_DOWNLOAD_BYTES}-byte limit"
                ));
            }
            file.write_all(&chunk)
                .await
                .map_err(|e| format!("Writing the download failed: {e}"))?;
        }
        return Ok(file);
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
        return super::write_json(&serde_json::json!({
            "filePath": path,
            "languageId": lang,
            "serverAvailable": server_available,
            "serverCommand": server_command,
            "probeError": probe_error,
        }));
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

/// `octocode lsp-server`: the managed (auto-downloadable) language servers.
#[derive(clap::Subcommand)]
pub(super) enum LspServerCommand {
    /// List the auto-downloadable servers and whether each is installed.
    List,
    /// Download and install servers into the managed cache.
    Install {
        /// Server names (e.g. `rust-analyzer`, `clangd`).
        names: Vec<String>,
        /// Install every auto-downloadable server.
        #[arg(long)]
        all: bool,
        /// Download even when `lsp.autoInstall` is `prompt` or `off`.
        #[arg(long)]
        force: bool,
    },
    /// Remove installed servers from the managed cache.
    Uninstall {
        /// Server names.
        names: Vec<String>,
    },
    /// Delete the whole managed cache (a dry run without `--yes`).
    Clean {
        /// Confirm the deletion.
        #[arg(long)]
        yes: bool,
    },
    /// Show the server a file (default: the workspace) routes to, and whether it resolves.
    Status {
        /// File whose language decides the server.
        file: Option<String>,
    },
}

/// Dispatch the `lsp-server` subcommand.
pub(super) async fn run(runtime: &ToolRuntime, command: LspServerCommand, json: bool) -> u8 {
    let discovery = discovery(runtime);
    let Some(root) = discovery.managed_root() else {
        eprintln!("The Octocode home is unavailable; cannot locate managed language servers.");
        return 5;
    };
    let platform = platform_id();
    match command {
        LspServerCommand::List => run_list(&root, &platform, json),
        LspServerCommand::Install { names, all, force } => {
            let targets: Vec<String> = if all {
                manifest().keys().map(|k| (*k).to_owned()).collect()
            } else {
                names
            };
            let mode = if force {
                LspAutoInstall::Auto
            } else {
                runtime.config().resolved.lsp.auto_install
            };
            run_install(&root, &platform, targets, mode, json, fetch_allowlisted).await
        }
        LspServerCommand::Uninstall { names } => run_uninstall(&root, &platform, names, json),
        LspServerCommand::Clean { yes } => run_clean(&root, yes, json),
        LspServerCommand::Status { file } => {
            run_status(&discovery, &root, &platform, file.as_deref(), json)
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
        return super::write_json(&serde_json::json!({ "servers": rows }));
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
    mode: LspAutoInstall,
    json: bool,
    fetch: F,
) -> u8
where
    F: Fn(&'static str, tokio::fs::File) -> Fut,
    Fut: Future<Output = Result<tokio::fs::File, String>>,
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
        super::write_json(&serde_json::json!({ "install": results }));
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
        super::write_json(&serde_json::json!({ "uninstall": results }));
    }
    0
}

fn run_clean(root: &Path, yes: bool, json: bool) -> u8 {
    let root_display = root.to_string_lossy().into_owned();
    if !yes {
        if json {
            super::write_json(&serde_json::json!({ "clean": "dry-run", "root": root_display }));
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
            );
        } else {
            eprintln!("Could not remove the managed LSP cache at {root_display}: {error}");
        }
        return 5;
    }
    if json {
        super::write_json(&serde_json::json!({ "clean": "done", "root": root_display }));
    } else {
        println!("cleaned: {root_display}");
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::GzEncoder};

    fn parse(args: &[&str]) -> Result<LspServerCommand, clap::Error> {
        #[derive(clap::Parser)]
        struct Cli {
            #[command(subcommand)]
            command: LspServerCommand,
        }
        <Cli as clap::Parser>::try_parse_from(
            std::iter::once("lsp-server").chain(args.iter().copied()),
        )
        .map(|cli| cli.command)
    }

    /// Each flag means one thing on one subcommand: `--force` overrides the
    /// install policy, `--yes` confirms a clean; neither is taken elsewhere.
    #[test]
    fn each_flag_belongs_to_one_subcommand() {
        assert!(matches!(
            parse(&["install", "clangd", "--force"]),
            Ok(LspServerCommand::Install { force: true, .. })
        ));
        assert!(matches!(
            parse(&["clean", "--yes"]),
            Ok(LspServerCommand::Clean { yes: true })
        ));
        assert!(parse(&["install", "clangd", "--yes"]).is_err());
        assert!(parse(&["clean", "--force"]).is_err());
        assert!(
            parse(&["status", "a.rs", "b.rs"]).is_err(),
            "status takes one file"
        );
        assert!(matches!(
            parse(&["status", "a.rs"]),
            Ok(LspServerCommand::Status { file: Some(file) }) if file == "a.rs"
        ));
    }

    /// The asset digest the manifest pins; resolve only trusts markers naming it.
    fn pinned_asset_sha(name: &str, platform: &str) -> &'static str {
        manifest_server(name).expect("server").platforms[platform]
            .sha256
            .expect("pinned asset")
    }

    #[test]
    fn a_stalled_download_fails_within_the_read_timeout() {
        // A peer that accepts and never answers must not hold the install lock.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let _peer = std::thread::spawn(move || {
            let (_stream, _) = listener.accept().expect("accept");
            std::thread::sleep(Duration::from_secs(5));
        });
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        // Built inside the runtime, as production builds it inside the async fetch.
        let (result, elapsed) = runtime.block_on(async {
            let client = download_client(Duration::from_millis(200)).expect("client");
            // Platform client setup is separate from a request's read timeout.
            let started = std::time::Instant::now();
            let result = client.get(format!("http://{addr}/")).send().await;
            (result, started.elapsed())
        });
        let error = result.expect_err("a stalled peer must not hang the request");
        assert!(
            error.is_timeout(),
            "the stalled read must time out: {error}"
        );
        assert!(elapsed < Duration::from_secs(4), "request took {elapsed:?}");
    }

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
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
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

    fn asset(archive: ArchiveKind, bin_path: Option<&'static str>) -> ManifestAsset {
        ManifestAsset {
            url: "https://github.com/example/tool",
            archive,
            bin_name: "tool",
            bin_path,
            sha256: None,
        }
    }

    /// Spool `download` the way `fetch` does, then verify and install it with
    /// its own digest pinned. Returns the outcome and the cache directory.
    fn install_download(
        asset: &ManifestAsset,
        download: &[u8],
        limit: u64,
    ) -> (Result<Vec<u8>, String>, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let spool = dir.path().join("asset.download");
        std::fs::write(&spool, download).expect("spool");
        let bin = dir.path().join("cache").join("tool");
        let pinned = octocode_engine::digest::sha256(download);
        let result = verify_and_install("tool", asset, &spool, &bin, &pinned, limit)
            .map(|()| std::fs::read(&bin).expect("installed binary"));
        (result, dir)
    }

    /// Entries in the cache directory, so a failed install proves it left nothing.
    fn cache_entries(dir: &tempfile::TempDir) -> Vec<String> {
        std::fs::read_dir(dir.path().join("cache"))
            .map(|entries| {
                entries
                    .flatten()
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn a_verified_gzip_download_installs_its_binary_and_marker() {
        let payload = b"#!/bin/sh\necho rust-analyzer\n";
        let (installed, dir) = install_download(&asset(ArchiveKind::Gz, None), &gz(payload), 1024);
        assert_eq!(installed.expect("installed"), payload);
        let marker: serde_json::Value = serde_json::from_slice(
            &std::fs::read(marker_path(&dir.path().join("cache/tool"))).expect("marker"),
        )
        .expect("marker json");
        assert_eq!(
            marker["binarySha256"],
            octocode_engine::digest::sha256(payload)
        );
        assert_eq!(
            marker["assetSha256"],
            octocode_engine::digest::sha256(&gz(payload))
        );
        assert_eq!(marker["size"], payload.len());
    }

    #[test]
    fn a_zip_download_installs_the_entry_named_after_normalizing_dot_slash() {
        let payload = b"clangd-binary";
        let archive = zip_with(&[
            ("clangd_22.1.0/README", b"readme"),
            ("clangd_22.1.0/bin/clangd", payload),
        ]);
        for target in ["clangd_22.1.0/bin/clangd", "./clangd_22.1.0/bin/clangd"] {
            let (installed, _dir) =
                install_download(&asset(ArchiveKind::Zip, Some(target)), &archive, 1024);
            assert_eq!(installed.expect(target), payload);
        }
        // Normalized on the stored entry name too.
        let dotted = zip_with(&[("./bin/clangd", payload)]);
        let (installed, _dir) =
            install_download(&asset(ArchiveKind::Zip, Some("bin/clangd")), &dotted, 1024);
        assert_eq!(installed.expect("dotted entry"), payload);
    }

    #[test]
    fn a_download_that_cannot_install_leaves_nothing_in_the_cache() {
        let zip = zip_with(&[("x", b"y")]);
        let cases: [(&str, ManifestAsset, Vec<u8>, u64, &str); 5] = [
            // A decompression bomb is refused at the limit, never held in memory.
            (
                "oversize gzip",
                asset(ArchiveKind::Gz, None),
                gz(&[0_u8; 1024]),
                512,
                "exceeds the 512-byte limit",
            ),
            (
                "oversize zip entry",
                asset(ArchiveKind::Zip, Some("x")),
                zip_with(&[("x", &[7_u8; 64])]),
                16,
                "exceeds the 16-byte limit",
            ),
            (
                "missing entry",
                asset(ArchiveKind::Zip, Some("nope")),
                zip.clone(),
                1024,
                "Could not find 'nope'",
            ),
            (
                "no binPath",
                asset(ArchiveKind::Zip, None),
                zip,
                1024,
                "missing binPath",
            ),
            // Bytes that are not an archive are a corrupt download, not a missing entry.
            (
                "not a zip",
                asset(ArchiveKind::Zip, Some("bin/clangd")),
                b"not a zip archive".to_vec(),
                1024,
                "corrupt",
            ),
        ];
        for (case, asset, download, limit, reason) in cases {
            let (installed, dir) = install_download(&asset, &download, limit);
            let error = installed.expect_err(case);
            assert!(error.contains(reason), "{case}: {error}");
            assert!(
                cache_entries(&dir).is_empty(),
                "{case}: {:?}",
                cache_entries(&dir)
            );
        }
    }

    #[test]
    fn a_download_that_does_not_match_its_pin_is_never_extracted() {
        let dir = tempfile::tempdir().expect("tempdir");
        let spool = dir.path().join("asset.download");
        std::fs::write(&spool, gz(b"tampered")).expect("spool");
        let bin = dir.path().join("cache").join("tool");
        let pinned = octocode_engine::digest::sha256(&gz(b"genuine"));
        let error = verify_and_install(
            "tool",
            &asset(ArchiveKind::Gz, None),
            &spool,
            &bin,
            &pinned,
            1024,
        )
        .expect_err("mismatch");
        assert!(error.starts_with("Checksum mismatch for tool"), "{error}");
        assert!(!dir.path().join("cache").exists());
    }

    /// Whether a new installer could take the lock at once (the file may stay).
    fn lock_is_free(path: &std::path::Path) -> bool {
        std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)
            .ok()
            .is_some_and(|file| {
                octocode_io::lock::lock_within(file, Duration::ZERO, Duration::from_millis(10))
                    .is_ok()
            })
    }

    /// Hold the install lock the way a concurrent installer would.
    fn hold(path: &std::path::Path) -> octocode_io::lock::LockedFile {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)
            .expect("lock file");
        octocode_io::lock::lock_within(file, Duration::ZERO, Duration::from_millis(10))
            .expect("held")
    }

    #[test]
    fn a_busy_install_lock_is_waited_for_then_taken() {
        // Concurrent installers: the loser waits for the winner instead of failing.
        let dir = tempfile::tempdir().expect("tempdir");
        let lock = dir.path().join(".lock");
        let held = hold(&lock);
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            drop(held);
        });
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let started = std::time::Instant::now();
        let taken = runtime.block_on(acquire_or_wait(
            lock.clone(),
            Duration::from_secs(5),
            Duration::from_millis(25),
        ));
        release.join().expect("release");
        assert!(taken.is_some(), "the lock is taken once its holder lets go");
        assert!(started.elapsed() >= Duration::from_millis(250));
    }

    #[test]
    fn a_lock_file_left_by_a_crashed_installer_does_not_block() {
        // A holder that died leaves only the file; the kernel already released it.
        let dir = tempfile::tempdir().expect("tempdir");
        let lock = dir.path().join(".lock");
        std::fs::write(&lock, b"").expect("leftover lock file");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let started = std::time::Instant::now();
        let taken = runtime.block_on(acquire_or_wait(
            lock,
            Duration::from_millis(200),
            Duration::from_millis(25),
        ));
        assert!(taken.is_some(), "a leftover file is not a held lock");
        assert!(started.elapsed() < Duration::from_millis(150));
    }

    #[test]
    fn a_lock_held_past_the_wait_is_refused() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lock = dir.path().join(".lock");
        let _held = hold(&lock);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let taken = runtime.block_on(acquire_or_wait(
            lock,
            Duration::from_millis(150),
            Duration::from_millis(25),
        ));
        assert!(taken.is_none());
    }

    #[test]
    fn a_download_error_never_echoes_a_signed_url() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let message = runtime.block_on(async {
            let client = download_client(Duration::from_millis(200)).expect("client");
            // Port 1 refuses the connection; the URL carries a signed token.
            let error = client
                .get("http://127.0.0.1:1/asset?X-Amz-Signature=SECRET-TOKEN")
                .send()
                .await
                .expect_err("refused");
            download_error(error)
        });
        assert!(!message.contains("SECRET-TOKEN"), "{message}");
        assert!(message.starts_with("Download failed"), "{message}");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn an_installed_binary_resolves_without_another_download() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let platform = "linux-x64";
        let bin = cached_server_bin_path(root, "rust-analyzer", platform).expect("bin path");
        // install_binary is the exact step provision_server runs after verification.
        install_binary(
            &bin,
            &b"rust-analyzer-bytes"[..],
            MAX_EXTRACTED_BYTES,
            pinned_asset_sha("rust-analyzer", platform),
        )
        .expect("install");
        assert_eq!(
            std::fs::read(&bin).expect("read bin"),
            b"rust-analyzer-bytes"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&bin).expect("meta").permissions().mode();
            // Owner-only executable: the binary lives under the user's home, and
            // the one atomic writer never grants group or other access.
            assert_eq!(mode & 0o700, 0o700, "installed binary is owner-executable");
            assert_eq!(
                mode & 0o077,
                0,
                "installed binary is not readable by others"
            );
            // The directory holding it is private too: no other user can
            // replace the binary between install and launch.
            let dir = bin.parent().expect("managed directory");
            let dir_mode = std::fs::metadata(dir)
                .expect("dir meta")
                .permissions()
                .mode();
            assert_eq!(dir_mode & 0o077, 0, "managed directory is private");
            // The completion marker is written the same way: owner-only, atomic.
            let marker_mode = std::fs::metadata(marker_path(&bin))
                .expect("marker")
                .permissions()
                .mode();
            assert_eq!(marker_mode & 0o077, 0, "completion marker is owner-only");
        }
        // Marker present + trusted → resolve returns the path.
        assert_eq!(
            resolve_cached_server(root, "rust-analyzer", platform),
            Some(bin.clone())
        );
        // provision_server short-circuits to already-present (fetch must not run).
        let again =
            provision_server(root, "rust-analyzer", platform, LspAutoInstall::Auto, never).await;
        assert_eq!(again.source, Some("already-present"));
    }

    /// Each refusal fails the outcome and names its reason.
    #[tokio::test(flavor = "current_thread")]
    async fn provision_refusals_name_their_reason() {
        type Fetch = fn(&'static str, tokio::fs::File) -> Spooled;
        let cases: [(&str, &str, LspAutoInstall, Fetch, &str); 3] = [
            (
                "rust-analyzer",
                "linux-x64",
                LspAutoInstall::Auto,
                tampered,
                "Checksum mismatch",
            ),
            (
                "clangd",
                "linux-x64",
                LspAutoInstall::Off,
                never,
                "Auto-install is off",
            ),
            (
                "clangd",
                "linux-arm64",
                LspAutoInstall::Auto,
                never,
                "no linux-arm64 release asset",
            ),
        ];
        for (name, platform, mode, fetch, reason) in cases {
            let dir = tempfile::tempdir().expect("tempdir");
            let outcome = provision_server(dir.path(), name, platform, mode, fetch).await;
            assert!(!outcome.ok, "{reason}");
            let error = outcome.error.expect("error should be set");
            assert!(error.contains(reason), "{reason}: {error}");
        }
    }

    #[test]
    fn uninstall_removes_only_within_cache_root() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let bin = cached_server_bin_path(root, "rust-analyzer", "linux-x64").expect("bin path");
        install_binary(
            &bin,
            &b"bytes"[..],
            MAX_EXTRACTED_BYTES,
            pinned_asset_sha("rust-analyzer", "linux-x64"),
        )
        .expect("install");
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

    type Spooled = futures_util::future::BoxFuture<'static, Result<tokio::fs::File, String>>;

    fn never(_url: &'static str, _file: tokio::fs::File) -> Spooled {
        panic!("must not fetch")
    }

    /// Serves an asset whose digest is not the pinned one.
    fn tampered(_url: &'static str, mut file: tokio::fs::File) -> Spooled {
        Box::pin(async move {
            tokio::io::AsyncWriteExt::write_all(&mut file, &gz(b"tampered"))
                .await
                .map_err(|e| e.to_string())?;
            Ok(file)
        })
    }

    /// Entries left in the per-target install directory, the lock aside.
    fn install_dir_leftovers(root: &std::path::Path) -> Vec<String> {
        std::fs::read_dir(root.join("rust-analyzer/2026-06-22"))
            .expect("install dir")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name != ".lock")
            .collect()
    }

    /// A refused download is removed, and so are the spool and temporaries a
    /// killed installer left behind, once the next installer holds the lock.
    #[tokio::test(flavor = "current_thread")]
    async fn no_download_outlives_its_install() {
        let dir = tempfile::tempdir().expect("tempdir");
        let install_dir = dir.path().join("rust-analyzer/2026-06-22");
        std::fs::create_dir_all(&install_dir).expect("install dir");
        for leftover in [".rust-analyzer-0f.download", ".rust-analyzer-1e.tmp"] {
            std::fs::write(install_dir.join(leftover), b"interrupted").expect("leftover");
        }
        let outcome = provision_server(
            dir.path(),
            "rust-analyzer",
            "linux-x64",
            LspAutoInstall::Auto,
            tampered,
        )
        .await;
        assert!(outcome.error.expect("error").contains("Checksum mismatch"));
        assert_eq!(install_dir_leftovers(dir.path()), Vec::<String>::new());
    }

    /// The CLI drives a current-thread runtime; installing must not need a
    /// multi-thread one, and the per-target lock is free afterwards.
    #[tokio::test(flavor = "current_thread")]
    async fn install_runs_on_the_current_thread_runtime() {
        let dir = tempfile::tempdir().expect("tempdir");
        let code = run_install(
            dir.path(),
            "linux-x64",
            vec!["rust-analyzer".to_owned()],
            LspAutoInstall::Auto,
            true,
            tampered,
        )
        .await;
        assert_eq!(code, 3);
        let lock = dir.path().join("rust-analyzer/2026-06-22/.lock");
        assert!(lock_is_free(&lock));
    }

    /// A panic mid-install still releases the lock, so the next install is
    /// not refused as "in progress".
    #[tokio::test(flavor = "current_thread")]
    async fn a_panicking_install_releases_its_lock() {
        use futures_util::FutureExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let panicked = std::panic::AssertUnwindSafe(provision_server(
            dir.path(),
            "rust-analyzer",
            "linux-x64",
            LspAutoInstall::Auto,
            never,
        ))
        .catch_unwind()
        .await;
        assert!(panicked.is_err());
        assert!(lock_is_free(
            &dir.path().join("rust-analyzer/2026-06-22/.lock")
        ));
        let retry = provision_server(
            dir.path(),
            "rust-analyzer",
            "linux-x64",
            LspAutoInstall::Auto,
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
