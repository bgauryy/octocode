//! LSP language-server provisioning for the native CLI: the download, verify,
//! extract, and install path into the managed cache that discovery reads.
//!
//! An install verifies three things that must agree, none typed by hand: the
//! digest `managed.lock.json` recorded from GitHub, GitHub's live digest for
//! the same asset (skipped, and said so, when the API cannot be reached), and
//! the size and SHA-256 of the downloaded bytes. For an immutable release the
//! release attestation must also cover the asset when `gh` can verify it.
//! Downloads use allowed HTTPS hosts on
//! every hop. It writes and marks completed executables atomically under
//! per-target locks. Archive support is limited to `gz` and `zip`.
mod commands;
mod download;
mod install;
mod upstream;
pub(super) use commands::{LspServerCommand, run};
pub use download::host_allowed;
pub use install::uninstall_server;
use install::{
    MAX_DOWNLOAD_BYTES, MAX_EXTRACTED_BYTES, SPOOL_SUFFIX, Spool, sweep_interrupted_installs,
    verify_and_install,
};
use octocode_engine::lsp::managed::{
    AssetIdentity, ManifestAsset, ManifestServer, cached_server_bin_path, manifest_server,
    resolve_cached_server,
};
use octocode_native::config::LspAutoInstall;
use std::path::{Path, PathBuf};
use std::time::Duration;
use upstream::{Attestation, Upstream};

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
    /// Which checks a download passed, in words; `None` without a download.
    pub verification: Option<String>,
}

impl ProvisionOutcome {
    fn fail(error: impl Into<String>) -> Self {
        Self {
            ok: false,
            path: None,
            source: None,
            error: Some(error.into()),
            verification: None,
        }
    }
    fn present(path: PathBuf, source: &'static str) -> Self {
        Self {
            ok: true,
            path: Some(path.to_string_lossy().into_owned()),
            source: Some(source),
            error: None,
            verification: None,
        }
    }
}

/// Provision `name` into the managed cache under `root`. Idempotent.
/// `upstream` answers GitHub's live digest, downloads the asset into the
/// spool file it is given, and checks attestations (injected so the full
/// verify/extract/install path is testable without network).
async fn provision_server(
    root: &Path,
    name: &str,
    platform: &str,
    mode: LspAutoInstall,
    upstream: &impl Upstream,
) -> ProvisionOutcome {
    let Some(server) = manifest_server(name) else {
        return ProvisionOutcome::fail(format!("{name} is not an auto-downloadable server."));
    };
    if let Some(reason) = server.unsupported_platforms.get(platform) {
        return ProvisionOutcome::fail(*reason);
    }
    let Some(asset) = server.platforms.get(platform) else {
        return ProvisionOutcome::fail(format!("No {name} asset for platform {platform}."));
    };
    provision_entry(root, name, platform, server, asset, mode, upstream).await
}

/// Provision `asset` of `server`, the manifest's entry for `name` on
/// `platform` (a test passes one locked to its own bytes).
async fn provision_entry(
    root: &Path,
    name: &str,
    platform: &str,
    server: &ManifestServer,
    asset: &ManifestAsset,
    mode: LspAutoInstall,
    upstream: &impl Upstream,
) -> ProvisionOutcome {
    if let Some(existing) = resolve_cached_server(root, name, platform) {
        return ProvisionOutcome::present(existing, "already-present");
    }
    if mode == LspAutoInstall::Off {
        return ProvisionOutcome::fail(
            "Auto-install is off. Set OCTOCODE_LSP_AUTO_INSTALL=prompt|auto (or pass --force) to allow downloading.",
        );
    }
    let (Some(lock), Some(identity)) = (asset.lock.as_ref(), AssetIdentity::of(server, asset))
    else {
        return ProvisionOutcome::fail(format!(
            "{} is not in managed.lock.json for {}@{}; regenerate it with `node skills-dev/octocode-dev/scripts/dev.mjs lsp:lock`. Refusing to download unverified bytes.",
            asset.asset, server.repo, server.release_tag
        ));
    };
    if !host_allowed(&lock.url) {
        return ProvisionOutcome::fail("Blocked non-allowlisted/insecure download host.");
    }
    // The live cross-check: GitHub's digest now must equal the lock's. A
    // different one means the asset was replaced after this build was made.
    let mut checks = vec![format!(
        "sha256 of the bytes matches managed.lock.json ({})",
        identity.sha256
    )];
    match upstream
        .live_digest(server.repo, server.release_tag, &asset.asset)
        .await
    {
        Ok(Some(live)) if live == identity.sha256 => {
            checks.push("GitHub's live digest agrees".to_owned());
        }
        Ok(live) => {
            return ProvisionOutcome::fail(format!(
                "{name}: {} asset changed upstream since this Octocode build: managed.lock.json has sha256 {}, GitHub now reports {}. Refusing to install.",
                asset.asset,
                identity.sha256,
                live.map_or_else(|| "no such asset or digest".to_owned(), |d| format!("sha256 {d}"))
            ));
        }
        Err(reason) => checks.push(format!(
            "GitHub's live digest was not checked ({reason}); verified against managed.lock.json only"
        )),
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
    match upstream.download(&lock.url, file).await {
        // Flushing waits for the last write in flight before the file is read.
        Ok(mut file) => {
            if let Err(e) = tokio::io::AsyncWriteExt::flush(&mut file).await {
                return ProvisionOutcome::fail(format!("Writing the download failed: {e}"));
            }
            // The lock records the asset's size: a different byte count is
            // refused before any hashing or extraction.
            match file.metadata().await {
                Ok(meta) if meta.len() == lock.size => {}
                Ok(meta) => {
                    return ProvisionOutcome::fail(format!(
                        "Checksum mismatch for {name}: the downloaded {} is {} bytes but managed.lock.json records {}, so it is corrupted or substituted.",
                        asset.asset,
                        meta.len(),
                        lock.size
                    ));
                }
                Err(e) => {
                    return ProvisionOutcome::fail(format!("Reading the download failed: {e}"));
                }
            }
        }
        Err(e) => return ProvisionOutcome::fail(e),
    }
    // An immutable release can carry a GitHub release attestation: refuse
    // only when a verified one does not cover this asset; when gh cannot
    // verify, say so. Without one, the three-way check above is the level
    // reached.
    if server.release_lock.as_ref().is_some_and(|r| r.immutable) {
        match upstream
            .attest(server.repo, server.release_tag, &identity.sha256)
            .await
        {
            Attestation::Verified => checks.push("release attestation verified".to_owned()),
            Attestation::NotChecked(reason) => checks.push(format!(
                "the release is immutable but its attestation was not checked ({reason})"
            )),
            Attestation::Failed(reason) => {
                return ProvisionOutcome::fail(format!(
                    "{name}: the release attestation of {} does not verify: {reason}",
                    asset.asset
                ));
            }
        }
    } else {
        checks.push("no release attestation is published".to_owned());
    }
    let (server_name, download, target) = (name.to_owned(), spool.0.clone(), bin_path.clone());
    let asset = asset.clone();
    let installed = tokio::task::spawn_blocking(move || {
        verify_and_install(
            &server_name,
            &asset,
            &download,
            &target,
            &identity,
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
    ProvisionOutcome {
        verification: Some(checks.join("; ")),
        ..ProvisionOutcome::present(bin_path, "downloaded")
    }
}

#[cfg(test)]
mod tests;
