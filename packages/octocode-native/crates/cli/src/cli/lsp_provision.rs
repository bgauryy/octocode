//! LSP language-server provisioning for the native CLI: the download, verify,
//! extract, and install path into the managed cache that discovery reads.
//!
//! Provisioning requires pinned SHA-256 assets and allowed HTTPS hosts on
//! every hop. It writes and marks completed executables atomically under
//! per-target locks. Archive support is limited to `gz` and `zip`.
mod commands;
mod download;
mod install;
pub(super) use commands::{LspServerCommand, run};
use download::fetch_allowlisted;
pub use download::host_allowed;
pub use install::uninstall_server;
use install::{
    MAX_DOWNLOAD_BYTES, MAX_EXTRACTED_BYTES, SPOOL_SUFFIX, Spool, sweep_interrupted_installs,
    verify_and_install,
};
use octocode_engine::lsp::managed::{
    cached_server_bin_path, manifest_server, resolve_cached_server,
};
use octocode_native::config::LspAutoInstall;
use std::future::Future;
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
        let file = octocode_io::lock::open_lock_file(&path).ok()?;
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
        Self {
            ok: false,
            path: None,
            source: None,
            error: Some(error.into()),
        }
    }
    fn present(path: PathBuf, source: &'static str) -> Self {
        Self {
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

#[cfg(test)]
mod tests;
