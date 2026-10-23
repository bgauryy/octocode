use super::*;

#[test]
fn missing_paths_echo_the_requested_path_not_its_basename() {
    let root = fixture();
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.clone()),
        ..Default::default()
    })
    .expect("policy");
    for input in ["packages/nope/dir", "./packages/nope/dir"] {
        let error = policy.validate(input).expect_err("missing");
        assert_eq!(error.code, PolicyErrorCode::NotFound);
        assert_eq!(error.message, "Path does not exist: packages/nope/dir");
    }
    let absolute = root.join("gone/file.rs");
    let error = policy.validate(&absolute).expect_err("missing");
    assert_eq!(error.message, "Path does not exist: gone/file.rs");
    std::fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn rejects_sensitive_paths_and_directories_as_reads() {
    let root = fixture();
    let secret = root.join(".env");
    std::fs::write(&secret, "TOKEN=x").expect("path policy test setup should succeed");
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.clone()),
        ..Default::default()
    })
    .expect("path policy test setup should succeed");
    assert_eq!(
        policy
            .validate_read(secret)
            .expect_err("path policy should reject the test input")
            .code,
        PolicyErrorCode::IgnoredPath
    );
    assert_eq!(
        policy
            .validate_read(&root)
            .expect_err("path policy should reject the test input")
            .code,
        PolicyErrorCode::NotRegular
    );
    assert_eq!(
        policy
            .validate(&root)
            .expect("path policy test setup should succeed")
            .canonical,
        std::fs::canonicalize(&root).expect("path policy test setup should succeed")
    );
    assert_eq!(
        policy
            .validate("   ")
            .expect_err("path policy should reject the test input")
            .code,
        PolicyErrorCode::EmptyPath
    );
    std::fs::remove_dir_all(root).expect("path policy test setup should succeed");
}

#[cfg(unix)]
#[test]
fn rejects_symlink_escape_and_output_under_symlinked_parent() {
    use std::os::unix::fs::symlink;

    let root = fixture();
    let outside = fixture();
    let outside_file = outside.join("secret.txt");
    std::fs::write(&outside_file, "secret").expect("path policy test setup should succeed");
    symlink(&outside_file, root.join("escape.txt")).expect("path policy test setup should succeed");
    symlink(&outside, root.join("escape-dir")).expect("path policy test setup should succeed");
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.clone()),
        ..Default::default()
    })
    .expect("path policy test setup should succeed");
    let escape = policy
        .validate_read(root.join("escape.txt"))
        .expect_err("path policy should reject the test input");
    assert_eq!(escape.code, PolicyErrorCode::SymlinkEscape);
    // The link and where it resolves (outside paths stay redacted).
    assert!(
        escape
            .message
            .starts_with("Symlink 'escape.txt' resolves to 'secret.txt', outside"),
        "{}",
        escape.message
    );
    for nul in ["a\0b.txt", "\0"] {
        assert_eq!(
            policy.validate_read(root.join(nul)).expect_err("NUL").code,
            PolicyErrorCode::InvalidInput
        );
        assert_eq!(
            policy
                .validate_output(root.join(nul))
                .expect_err("NUL")
                .code,
            PolicyErrorCode::InvalidInput
        );
    }
    assert_eq!(
        policy
            .validate_output(root.join("escape-dir/new.txt"))
            .expect_err("path policy should reject the test input")
            .code,
        PolicyErrorCode::OutsideAllowedRoots
    );
    std::fs::remove_dir_all(root).expect("path policy test setup should succeed");
    std::fs::remove_dir_all(outside).expect("path policy test setup should succeed");
}

#[cfg(unix)]
#[test]
fn outside_denial_names_the_requested_path_but_never_a_symlink_target() {
    use std::os::unix::fs::symlink;

    let workspace = fixture();
    let outside = fixture();
    let requested = outside.join("app.ts");
    std::fs::write(&requested, "x").expect("write outside fixture");
    symlink(&requested, workspace.join("link.ts")).expect("symlink fixture");
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(workspace.clone()),
        ..Default::default()
    })
    .expect("policy");
    let denied = policy.validate_read(&requested).expect_err("outside path");
    assert!(
        denied
            .message
            .contains(&*normalize(&requested).to_string_lossy()),
        "{}",
        denied.message
    );
    let escaped = policy
        .validate_read(workspace.join("link.ts"))
        .expect_err("symlink escape");
    assert!(
        !escaped.message.contains(&*outside.to_string_lossy()),
        "{}",
        escaped.message
    );
    std::fs::remove_dir_all(workspace).expect("remove workspace fixture");
    std::fs::remove_dir_all(outside).expect("remove outside fixture");
}

#[test]
fn relative_outside_denial_echoes_only_the_caller_input() {
    let workspace = fixture();
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(workspace.join("nested")),
        ..Default::default()
    })
    .expect("policy");
    std::fs::create_dir_all(workspace.join("nested")).expect("nested root");
    let denied = policy
        .validate_read("../../outside.txt")
        .expect_err("outside path");
    assert!(
        denied.message.contains("'../../outside.txt'"),
        "{}",
        denied.message
    );
    let prefix = workspace
        .parent()
        .expect("parent")
        .to_string_lossy()
        .into_owned();
    assert!(
        !denied
            .message
            .split(" (allowed")
            .next()
            .unwrap_or_default()
            .contains(&prefix),
        "{}",
        denied.message
    );
    std::fs::remove_dir_all(workspace).expect("remove workspace fixture");
}

#[test]
fn outside_read_denial_does_not_reveal_existence() {
    let workspace = fixture();
    let outside = fixture();
    let present = outside.join("present.txt");
    let absent = outside.join("absent.txt");
    std::fs::write(&present, "x").expect("write outside fixture");
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(workspace.clone()),
        ..Default::default()
    })
    .expect("policy");
    for path in [present, absent] {
        assert_eq!(
            policy.validate_read(path).expect_err("outside path").code,
            PolicyErrorCode::OutsideAllowedRoots
        );
    }
    std::fs::remove_dir_all(workspace).expect("remove workspace fixture");
    std::fs::remove_dir_all(outside).expect("remove outside fixture");
}

/// Allowed roots are named as `redact` shows them in every build: the
/// workspace as `.`, home roots as `~/...`, never an absolute layout; a root
/// inside another allowed root is left out.
#[test]
fn outside_denial_names_the_allowed_roots_redacted_once_each() {
    let workspace = fixture();
    let home = fixture();
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(workspace.clone()),
        additional_roots: vec![
            home.join(".octocode"),
            home.join(".octocode/tmp/clone"),
            workspace.join("nested"),
        ],
        home_dir: Some(home.clone()),
        ..Default::default()
    })
    .expect("policy");
    let denied = policy
        .validate("/nowhere/at/all.txt")
        .expect_err("outside path");
    assert_eq!(denied.code, PolicyErrorCode::OutsideAllowedRoots);
    assert_eq!(
        denied.message,
        "Path '/nowhere/at/all.txt' is outside allowed directories (allowed: ., ~/.octocode)"
    );
    std::fs::remove_dir_all(workspace).expect("remove workspace fixture");
    std::fs::remove_dir_all(home).expect("remove home fixture");
}
