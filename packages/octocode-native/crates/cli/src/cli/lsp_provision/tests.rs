use super::commands::{run_clean, run_install, run_status};
use super::download::{download_client, download_error};
use super::install::install_binary;
use super::upstream::{Attestation, GitHub, Upstream};
use super::*;
use flate2::{Compression, write::GzEncoder};
use octocode_engine::lsp::config::LspDiscoveryOptions;
use octocode_engine::lsp::managed::{
    ArchiveKind, AssetIdentity, AssetLock, ManifestAsset, marker_path,
};
mod attestation;
mod commands;
mod downloads;
mod install;
mod provision;
mod three_way;

fn parse(args: &[&str]) -> Result<LspServerCommand, clap::Error> {
    #[derive(clap::Parser)]
    struct Cli {
        #[command(subcommand)]
        command: LspServerCommand,
    }
    <Cli as clap::Parser>::try_parse_from(std::iter::once("lsp-server").chain(args.iter().copied()))
        .map(|cli| cli.command)
}

/// The asset identity the manifest and lock name; resolve only trusts
/// markers naming it.
fn locked_identity(name: &str, platform: &str) -> AssetIdentity {
    AssetIdentity::locked(name, platform).expect("locked asset")
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
        asset: "tool.bin".to_owned(),
        archive,
        bin_name: "tool".to_owned(),
        bin_path: bin_path.map(str::to_owned),
        lock: None,
    }
}

/// An identity for `download` itself, as a lock generated for it would name.
fn identity_of(download: &[u8]) -> AssetIdentity {
    AssetIdentity {
        repo: "example/tool".to_owned(),
        tag: "v1".to_owned(),
        asset: "tool.bin".to_owned(),
        sha256: octocode_engine::digest::sha256(download),
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
    let result = verify_and_install("tool", asset, &spool, &bin, &identity_of(download), limit)
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

/// GitHub as a test needs it: the live digest from `live` (a real
/// [`GitHub`] against a local fixture, or the lock's own digest when
/// `None`), the bytes in `bytes` (`None` = must not download), and a fixed
/// attestation answer.
struct Fake {
    live: Option<GitHub>,
    bytes: Option<Vec<u8>>,
    attestation: Attestation,
}

impl Fake {
    /// Agrees with the lock and serves `bytes`.
    fn serving(bytes: Vec<u8>) -> Self {
        Self {
            live: None,
            bytes: Some(bytes),
            attestation: Attestation::NotChecked("test".into()),
        }
    }
    /// Agrees with the lock and panics on download.
    fn never() -> Self {
        Self {
            bytes: None,
            ..Self::serving(Vec::new())
        }
    }
}

/// Serves an asset whose digest is not the locked one.
fn tampered() -> Fake {
    Fake::serving(gz(b"tampered"))
}

impl Upstream for Fake {
    async fn live_digest(
        &self,
        repo: &str,
        tag: &str,
        asset: &str,
    ) -> Result<Option<String>, String> {
        if let Some(github) = &self.live {
            return github.live_digest(repo, tag, asset).await;
        }
        Ok(octocode_engine::lsp::managed::manifest()
            .values()
            .filter(|server| server.repo == repo && server.release_tag == tag)
            .flat_map(|server| server.platforms.values())
            .find(|entry| entry.asset == asset)
            .and_then(|entry| entry.lock.as_ref())
            .map(|lock| lock.sha256.clone()))
    }
    async fn download(
        &self,
        _url: &str,
        mut file: tokio::fs::File,
    ) -> Result<tokio::fs::File, String> {
        let bytes = self.bytes.as_ref().expect("must not fetch");
        tokio::io::AsyncWriteExt::write_all(&mut file, bytes)
            .await
            .map_err(|e| e.to_string())?;
        Ok(file)
    }
    async fn attest(&self, _repo: &str, _tag: &str, _sha256: &str) -> Attestation {
        self.attestation.clone()
    }
}

/// A local HTTP server answering every request with `status` and `body`
/// (the GitHub API as a fixture); its base URL.
fn serve(status: u16, body: String) -> String {
    serve_recording(status, body).0
}

/// [`serve`], also handing back the head (request line and headers) of every
/// request it has received.
fn serve_recording(
    status: u16,
    body: String,
) -> (String, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
    use std::io::{BufRead, BufReader};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    let requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = std::sync::Arc::clone(&requests);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(&stream);
            let (mut line, mut head) = (String::new(), String::new());
            while reader.read_line(&mut line).is_ok_and(|n| n > 0) && line != "\r\n" {
                head.push_str(&line);
                line.clear();
            }
            seen.lock().expect("requests").push(head);
            let response = format!(
                "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = (&stream).write_all(response.as_bytes());
        }
    });
    (base, requests)
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
