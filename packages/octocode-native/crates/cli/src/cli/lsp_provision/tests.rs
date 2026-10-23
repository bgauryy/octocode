use super::commands::{run_clean, run_install, run_status};
use super::download::{download_client, download_error};
use super::install::install_binary;
use super::*;
use flate2::{Compression, write::GzEncoder};
use octocode_engine::lsp::config::LspDiscoveryOptions;
use octocode_engine::lsp::managed::{ArchiveKind, ManifestAsset, marker_path};
mod commands;
mod downloads;
mod install;
mod provision;

fn parse(args: &[&str]) -> Result<LspServerCommand, clap::Error> {
    #[derive(clap::Parser)]
    struct Cli {
        #[command(subcommand)]
        command: LspServerCommand,
    }
    <Cli as clap::Parser>::try_parse_from(std::iter::once("lsp-server").chain(args.iter().copied()))
        .map(|cli| cli.command)
}

/// The asset digest the manifest pins; resolve only trusts markers naming it.
fn pinned_asset_sha(name: &str, platform: &str) -> &'static str {
    manifest_server(name).expect("server").platforms[platform]
        .sha256
        .expect("pinned asset")
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

/// Whether a new installer could take the lock at once (the file may stay).
fn lock_is_free(path: &std::path::Path) -> bool {
    std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
        .ok()
        .is_some_and(|file| {
            octocode_io::lock::lock_within(file, Duration::ZERO, Duration::from_millis(10)).is_ok()
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
    octocode_io::lock::lock_within(file, Duration::ZERO, Duration::from_millis(10)).expect("held")
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
