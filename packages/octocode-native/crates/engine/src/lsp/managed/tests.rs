use super::*;
use crate::test_support::TmpDir;

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

/// A binary installed from a different asset than the one the
/// manifest now pins (same release-tag path) is not trusted.
#[test]
fn resolve_rejects_marker_for_a_different_asset() {
    let root = TmpDir::new();
    install_with_asset(&root, "rust-analyzer", "linux-x64", b"ra", &"0".repeat(64));
    assert_eq!(
        resolve_cached_server(&root, "rust-analyzer", "linux-x64"),
        None
    );
}

/// Review M7: an unchanged binary is hashed once per process, not on
/// every resolve; a changed binary is hashed again and rejected.
#[test]
fn resolve_hashes_an_unchanged_binary_once() {
    let root = TmpDir::new();
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

/// An architecture the manifest does not ship must not be
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
}
