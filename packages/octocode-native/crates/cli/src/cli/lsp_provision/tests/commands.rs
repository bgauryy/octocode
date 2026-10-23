use super::*;

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
