use super::*;

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
