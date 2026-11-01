use super::*;

mod credential_directories;
mod denials;
mod read_filter;

/// Callers own cleanup.
fn fixture() -> PathBuf {
    tempfile::Builder::new()
        .prefix("octocode-policy-")
        .tempdir()
        .expect("path policy test setup should succeed")
        .keep()
}

#[cfg(windows)]
#[test]
fn native_windows_separators_resolve_to_the_same_file() {
    let root = tempfile::tempdir().expect("fixture");
    std::fs::create_dir(root.path().join("dir")).expect("directory");
    std::fs::write(root.path().join("dir/file.txt"), "content").expect("file");
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.path().to_path_buf()),
        ..Default::default()
    })
    .expect("policy");
    let forward = policy
        .validate_read("dir/file.txt")
        .expect("forward separator");
    let backward = policy
        .validate_read(r"dir\file.txt")
        .expect("backward separator");
    assert_eq!(forward.canonical, backward.canonical);
}

#[cfg(unix)]
#[test]
fn literal_backslash_paths_do_not_resolve_to_directory_siblings() {
    let root = tempfile::tempdir().expect("fixture");
    let literal = root.path().join("back\\slash.txt");
    let nested = root.path().join("back/slash.txt");
    std::fs::create_dir(root.path().join("back")).expect("directory");
    std::fs::write(&literal, "literal filename").expect("literal file");
    std::fs::write(&nested, "directory sibling").expect("nested file");
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.path().to_path_buf()),
        ..Default::default()
    })
    .expect("policy");
    for requested in [Path::new("back\\slash.txt"), literal.as_path()] {
        let validated = policy.validate_read(requested).expect("literal read");
        assert_eq!(
            std::fs::read_to_string(&validated.canonical).expect("read"),
            "literal filename"
        );
        assert_eq!(
            policy.workspace_relative(&validated.canonical).as_deref(),
            Some("back\\slash.txt")
        );
        assert_eq!(policy.redact(requested), "back\\slash.txt");
    }
    let validated = policy.validate_read("back/slash.txt").expect("nested read");
    assert_eq!(
        std::fs::read_to_string(validated.canonical).expect("read"),
        "directory sibling"
    );
}

/// A walk's discovery decisions equal `permits_discovery` for every
/// entry: plain files, directories, sensitive names, and symlinks that
/// stay inside or escape the allowed roots.
#[cfg(unix)]
#[test]
fn walk_discovery_matches_permits_discovery_for_every_entry() {
    let root = fixture();
    let outside = fixture();
    std::fs::create_dir_all(root.join("src/nested")).expect("dirs");
    std::fs::write(root.join("src/lib.rs"), "x").expect("file");
    std::fs::write(root.join("src/nested/.env.production"), "x").expect("sensitive");
    std::fs::create_dir_all(root.join(".ssh")).expect("credential dir");
    std::fs::write(outside.join("secret.txt"), "x").expect("outside");
    std::os::unix::fs::symlink(outside.join("secret.txt"), root.join("src/escape"))
        .expect("escaping link");
    std::os::unix::fs::symlink(root.join("src/lib.rs"), root.join("src/inside"))
        .expect("inner link");
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.clone()),
        ..Default::default()
    })
    .expect("policy");
    let walk = policy.discovery_walk();
    for entry in [
        "src",
        "src/lib.rs",
        "src/nested",
        "src/nested/.env.production",
        ".ssh",
        "src/escape",
        "src/inside",
        "src/missing.rs",
    ] {
        let path = root.join(entry);
        assert_eq!(
            walk.permits(&path),
            policy.permits_discovery(&path),
            "{entry}"
        );
    }
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(outside);
}

/// A walk that visits each directory before its entries (as every local
/// walk does) resolves real paths top-down: one canonicalization for the
/// whole tree, with every decision still equal to `permits_discovery`,
/// under a root reached through a symlinked ancestor (`/var` →
/// `/private/var` on macOS) and beside links that stay in or escape.
#[cfg(unix)]
#[test]
fn a_top_down_walk_canonicalizes_once_and_decides_as_permits_discovery() {
    let root = fixture();
    let outside = fixture();
    std::fs::write(outside.join("secret.txt"), "x").expect("outside");
    let mut entries = Vec::new();
    for top in ["a", "b"] {
        for mid in ["c", "d"] {
            let dir = format!("{top}/{mid}/deep");
            std::fs::create_dir_all(root.join(&dir)).expect("dirs");
            std::fs::write(root.join(&dir).join("lib.rs"), "x").expect("file");
            std::fs::write(root.join(&dir).join(".env.local"), "x").expect("sensitive");
            entries.extend([
                top.to_owned(),
                format!("{top}/{mid}"),
                dir.clone(),
                format!("{dir}/lib.rs"),
                format!("{dir}/.env.local"),
            ]);
        }
    }
    std::fs::create_dir_all(root.join("a/.ssh")).expect("credential dir");
    std::os::unix::fs::symlink(outside.join("secret.txt"), root.join("a/c/escape"))
        .expect("escaping link");
    std::os::unix::fs::symlink(root.join("b"), root.join("a/inside")).expect("inner link");
    entries.extend(["a/.ssh".into(), "a/c/escape".into(), "a/inside".into()]);
    entries.sort();
    entries.dedup();
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.clone()),
        ..Default::default()
    })
    .expect("policy");
    let walk = policy.discovery_walk();
    let before = walk::CANONICALIZED.with(std::cell::Cell::get);
    for entry in &entries {
        let path = root.join(entry);
        let kind = std::fs::symlink_metadata(&path)
            .ok()
            .map(|meta| meta.file_type());
        assert_eq!(
            walk.permits_entry(&path, kind),
            policy.permits_discovery(&path),
            "{entry}"
        );
    }
    let canonicalized = walk::CANONICALIZED.with(std::cell::Cell::get) - before;
    assert_eq!(canonicalized, 1, "only the walk root's parent");
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(outside);
}

#[test]
fn newly_created_allowed_root_uses_its_resolved_parent_without_allowing_siblings() {
    let root = fixture();
    let allowed = root.join("new").join("home");
    let sibling = root.join("new").join("other");
    let policy = PathPolicy::new(PathPolicyConfig {
        additional_roots: vec![allowed.clone()],
        ..Default::default()
    })
    .expect("policy");
    assert!(!allowed.exists());
    std::fs::create_dir_all(&allowed).expect("new root");
    assert!(policy.validate_output(allowed.join("tmp/clone")).is_ok());
    assert!(policy.validate_output(&sibling).is_err());
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn new_allowed_root_redirected_by_symlink_is_still_denied() {
    let root = fixture();
    let allowed = root.join("new").join("home");
    let outside = fixture();
    let policy = PathPolicy::new(PathPolicyConfig {
        additional_roots: vec![allowed.clone()],
        ..Default::default()
    })
    .expect("policy");
    std::fs::create_dir_all(allowed.parent().expect("parent")).expect("parent directory");
    std::os::unix::fs::symlink(&outside, &allowed).expect("redirect root");
    assert!(policy.validate_output(allowed.join("tmp/clone")).is_err());
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(outside);
}

#[test]
fn permits_regular_workspace_files_and_redacts_the_workspace_prefix() {
    let root = fixture();
    let file = root.join("source.rs");
    std::fs::write(&file, "safe").expect("path policy test setup should succeed");
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.clone()),
        ..Default::default()
    })
    .expect("path policy test setup should succeed");
    let result = policy
        .validate_read(&file)
        .expect("path policy test setup should succeed");
    assert_eq!(
        result.canonical,
        std::fs::canonicalize(&file).expect("path policy test setup should succeed")
    );
    assert_eq!(result.display, "source.rs");
    assert_eq!(
        policy.redact(root.join("src/sub/../a.ts")),
        Path::new("src").join("a.ts").to_string_lossy()
    );
    std::fs::remove_dir_all(root).expect("path policy test setup should succeed");
}

#[test]
fn home_is_not_allowed_unless_opted_in() {
    let workspace = fixture();
    let home = fixture();
    let outside = home.join("notes.txt");
    std::fs::write(&outside, "x").expect("path policy test setup should succeed");
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(workspace.clone()),
        home_dir: Some(home.clone()),
        include_home: false,
        ..Default::default()
    })
    .expect("path policy test setup should succeed");
    assert!(
        !policy
            .allowed_roots()
            .iter()
            .any(|root| home == *root || home.starts_with(root)),
        "home must not be a default allowed root"
    );
    assert_eq!(
        policy
            .validate_read(&outside)
            .expect_err("home read must be denied by default")
            .code,
        PolicyErrorCode::OutsideAllowedRoots
    );
    let opted = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(workspace.clone()),
        home_dir: Some(home.clone()),
        include_home: true,
        ..Default::default()
    })
    .expect("path policy test setup should succeed");
    assert!(
        opted.validate_read(&outside).is_ok(),
        "explicit include_home must grant home access"
    );
    std::fs::remove_dir_all(workspace).expect("path policy test setup should succeed");
    std::fs::remove_dir_all(home).expect("path policy test setup should succeed");
}

#[test]
fn relative_paths_resolve_against_the_workspace_root_not_the_process_cwd() {
    let workspace = fixture();
    std::fs::create_dir_all(workspace.join("src")).expect("src dir");
    std::fs::write(workspace.join("src/a.ts"), "x").expect("write fixture");
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(workspace.clone()),
        ..Default::default()
    })
    .expect("policy");
    let read = policy
        .validate_read("src/a.ts")
        .expect("relative path must resolve under WORKSPACE_ROOT");
    assert_eq!(
        read.canonical,
        std::fs::canonicalize(workspace.join("src/a.ts")).expect("canonical")
    );
    assert_eq!(read.display, "src/a.ts");
    let dir = policy.validate(".").expect("`.` is the workspace root");
    assert_eq!(
        dir.canonical,
        std::fs::canonicalize(&workspace).expect("canonical")
    );
    std::fs::remove_dir_all(workspace).expect("remove workspace fixture");
}

#[test]
fn additional_roots_are_allowed_and_builtin_ignores_still_apply() {
    // `local.allowedPaths` reaches the policy as `additional_roots`.
    let workspace = fixture();
    let extra = fixture();
    let visible = extra.join("notes.txt");
    let sensitive = extra.join("terraform.tfstate");
    std::fs::write(&visible, "value").expect("write fixture");
    std::fs::write(&sensitive, "value").expect("write fixture");
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(workspace.clone()),
        additional_roots: vec![extra.clone()],
        ..Default::default()
    })
    .expect("policy");
    assert!(policy.validate_read(&visible).is_ok());
    assert_eq!(
        policy
            .validate_read(&sensitive)
            .expect_err("built-in ignore must apply under additional roots")
            .code,
        PolicyErrorCode::IgnoredPath
    );
    std::fs::remove_dir_all(workspace).expect("remove fixture");
    std::fs::remove_dir_all(extra).expect("remove fixture");
}

#[test]
fn nearest_existing_dir_names_the_closest_admitted_parent() {
    let root = fixture();
    std::fs::create_dir_all(root.join("src/a")).expect("dirs");
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.clone()),
        ..Default::default()
    })
    .expect("policy");
    assert_eq!(
        policy.nearest_existing_dir("src/nativ/x.rs").as_deref(),
        Some("src")
    );
    assert_eq!(
        policy.nearest_existing_dir("src/a/missing").as_deref(),
        Some("src/a")
    );
    assert_eq!(policy.nearest_existing_dir("nope").as_deref(), Some("."));
    std::fs::remove_dir_all(root).expect("remove fixture");
}

#[test]
fn nearest_existing_dir_stops_at_policy_root() {
    let root = fixture();
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.join("ws")),
        ..Default::default()
    })
    .expect("policy");
    std::fs::create_dir_all(root.join("ws")).expect("ws");
    // The temp dir above the workspace exists but is outside the policy.
    let outside = root.join("elsewhere/missing");
    assert_eq!(
        policy.nearest_existing_dir(&outside.to_string_lossy()),
        None
    );
    std::fs::remove_dir_all(root).expect("remove fixture");
}
