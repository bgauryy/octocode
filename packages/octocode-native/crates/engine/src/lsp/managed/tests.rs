use super::*;
use crate::test_support::TmpDir;

fn install(root: &Path, name: &str, platform: &str, bytes: &[u8]) -> PathBuf {
    let identity = AssetIdentity::locked(name, platform).expect("locked asset");
    install_with_identity(root, name, platform, bytes, &identity)
}

fn install_with_identity(
    root: &Path,
    name: &str,
    platform: &str,
    bytes: &[u8],
    identity: &AssetIdentity,
) -> PathBuf {
    let bin = cached_server_bin_path(root, name, platform).expect("bin path");
    std::fs::create_dir_all(bin.parent().expect("parent")).expect("dir");
    std::fs::write(&bin, bytes).expect("bin");
    let marker = cache_marker(identity, &crate::digest::sha256(bytes), bytes.len() as u64);
    std::fs::write(marker_path(&bin), marker.to_string()).expect("marker");
    bin
}

/// A binary whose marker names another asset identity than the manifest and
/// lock name now (same release-tag path) is not trusted, whichever part
/// differs: the bytes, the file name, the tag, or the repository.
#[test]
fn resolve_rejects_a_marker_for_another_asset_identity() {
    let locked = AssetIdentity::locked("rust-analyzer", "linux-x64").expect("locked");
    let others = [
        AssetIdentity {
            sha256: "0".repeat(64),
            ..locked.clone()
        },
        AssetIdentity {
            asset: format!("{}.old", locked.asset),
            ..locked.clone()
        },
        AssetIdentity {
            tag: format!("{}-1", locked.tag),
            ..locked.clone()
        },
        AssetIdentity {
            repo: "someone/fork".into(),
            ..locked.clone()
        },
    ];
    for other in others {
        let root = TmpDir::new();
        install_with_identity(&root, "rust-analyzer", "linux-x64", b"ra", &other);
        assert_eq!(
            resolve_cached_server(&root, "rust-analyzer", "linux-x64"),
            None,
            "{other:?}"
        );
    }
    let root = TmpDir::new();
    let bin = install_with_identity(&root, "rust-analyzer", "linux-x64", b"ra", &locked);
    assert_eq!(
        resolve_cached_server(&root, "rust-analyzer", "linux-x64"),
        Some(bin)
    );
}

/// The embedded lock covers every asset the manifest names, for the
/// manifest's own repository and tag (`dev.mjs lsp:lock` keeps it so).
#[test]
fn the_lock_covers_every_manifest_asset() {
    for (name, server) in manifest() {
        assert!(
            server.release_lock.is_some(),
            "{name}: lock names another release"
        );
        for (platform, asset) in &server.platforms {
            let lock = asset
                .lock
                .as_ref()
                .unwrap_or_else(|| panic!("{name} {platform}: {} is not in the lock", asset.asset));
            assert!(
                lock.url.ends_with(&format!("/{}", asset.asset)),
                "{}",
                lock.url
            );
            assert!(
                lock.url.contains(&format!("/{}/", server.release_tag)),
                "{}",
                lock.url
            );
            assert!(lock.size > 0);
        }
    }
}

/// A lock entry recorded for another tag than the manifest declares locks
/// nothing, so an install is refused rather than verified against it.
#[test]
fn a_lock_for_another_release_locks_nothing() {
    let text = lock::embedded_text().replace(
        &format!("\"tag\": \"{}\"", manifest()["clangd"].release_tag),
        "\"tag\": \"0.0.0\"",
    );
    let servers = build_manifest(&lock::Lock::parse(&text));
    assert!(servers["clangd"].release_lock.is_none());
    assert!(
        servers["clangd"]
            .platforms
            .values()
            .all(|a| a.lock.is_none())
    );
    assert!(servers["rust-analyzer"].release_lock.is_some());
}

/// Name fragments a release asset uses for each OS and CPU a platform id
/// (`<os>-<arch>[-musl]`) names.
const OS_TOKENS: &[(&str, &[&str])] = &[
    ("darwin", &["mac", "apple-darwin", "darwin"]),
    ("linux", &["linux"]),
    ("win32", &["windows"]),
];
const ARCH_TOKENS: &[(&str, &[&str])] = &[
    ("x64", &["x86_64", "x64", "amd64"]),
    ("arm64", &["aarch64", "arm64"]),
];

fn names_any(asset: &str, tokens: &[&str]) -> bool {
    let asset = asset.to_ascii_lowercase();
    tokens.iter().any(|token| asset.contains(token))
}

/// A platform must map to an asset built for its own OS and CPU: the lock is
/// generated for whatever name the manifest declares, so only this catches a
/// `darwin-x64` entry pointing at the linux build (the #481 bug class). A
/// server whose assets name no CPU at all (clangd ships one zip per OS) is
/// held to the OS alone; once any of its assets names a CPU, all must.
#[test]
fn every_platform_maps_to_an_asset_built_for_its_os_and_cpu() {
    for (name, server) in manifest() {
        let names_cpus = server.platforms.values().any(|asset| {
            ARCH_TOKENS
                .iter()
                .any(|(_, tokens)| names_any(&asset.asset, tokens))
        });
        for (platform, asset) in &server.platforms {
            let mut parts = platform.split('-');
            let (os, arch) = (parts.next().expect("os"), parts.next().expect("arch"));
            let musl = parts.next() == Some("musl");
            let context = format!("{name} {platform} -> {}", asset.asset);
            for (id, tokens) in OS_TOKENS {
                assert_eq!(names_any(&asset.asset, tokens), *id == os, "{context}: os");
            }
            if names_cpus {
                for (id, tokens) in ARCH_TOKENS {
                    assert_eq!(
                        names_any(&asset.asset, tokens),
                        *id == arch,
                        "{context}: cpu"
                    );
                }
            }
            if os == "linux" && names_cpus {
                assert_eq!(names_any(&asset.asset, &["musl"]), musl, "{context}: libc");
            }
        }
    }
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
