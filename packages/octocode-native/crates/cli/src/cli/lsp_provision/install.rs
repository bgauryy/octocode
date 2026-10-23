use octocode_engine::lsp::managed::{
    ArchiveKind, ManifestAsset, cached_server_bin_path, marker_path,
};
use std::io::{BufReader, Read, Seek};
use std::path::{Path, PathBuf};

/// Largest decompressed binary accepted; past it the archive is refused.
pub(super) const MAX_EXTRACTED_BYTES: u64 = 512 * 1024 * 1024;
/// Largest release asset downloaded; a mis-redirected page or a hostile asset
/// stops here, before its checksum is computed.
pub(super) const MAX_DOWNLOAD_BYTES: u64 = 256 * 1024 * 1024;
/// Suffix of the file a download is spooled into, beside the binary it becomes.
pub(super) const SPOOL_SUFFIX: &str = "download";

/// Check the spooled download of `name` against `expected_sha`, then extract
/// the executable from it straight into the cache. The download is read from
/// disk twice and never held in memory.
pub(super) fn verify_and_install(
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
pub(super) fn strip_lead(s: &str) -> &str {
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
pub(super) fn zip_entry_index<R: Read + Seek>(
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
pub(super) fn install_binary(
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
pub(super) fn sweep_interrupted_installs(dir: &Path) {
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
pub(super) struct Spool(pub(super) PathBuf);

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
