//! `security.credentialFiles: allow`: what it lifts and what no setting lifts.
use super::*;

struct Layout {
    _dir: Fixture,
    policy: PathPolicy,
    workspace: PathBuf,
    octocode_home: PathBuf,
    os_home: PathBuf,
}

fn layout(allow: bool) -> Layout {
    let dir = fixture();
    let root = dir.canonicalize().expect("real");
    let os_home = root.join("user");
    let workspace = os_home.join("project");
    let octocode_home = os_home.join(".octocode");
    std::fs::create_dir_all(&workspace).expect("workspace");
    std::fs::create_dir_all(&octocode_home).expect("home");
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(workspace.clone()),
        additional_roots: vec![octocode_home.clone(), os_home.clone()],
        home_dir: Some(os_home.clone()),
        allow_credential_files: allow,
        octocode_home: Some(octocode_home.clone()),
        ..Default::default()
    })
    .expect("policy");
    Layout {
        _dir: dir,
        policy,
        workspace,
        octocode_home,
        os_home,
    }
}

fn file(path: &Path) -> PathBuf {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
    std::fs::write(path, "x").expect("file");
    path.to_path_buf()
}

/// Every spelling the policy folds lifts together: the setting builds on the
/// discovery rules, it does not bypass them.
#[test]
fn allow_lifts_credential_names_in_the_workspace_in_every_spelling() {
    let names = [
        ".env",
        ".env.local",
        "credentials.json",
        "server.pem",
        "KEY.PEM",
        ".ENV",
        "id_rsa.old",
    ];
    let allowed = layout(true);
    let withheld = layout(false);
    for name in names {
        let path = file(&allowed.workspace.join("src").join(name));
        assert!(allowed.policy.validate_read(&path).is_ok(), "{name}");
        let path = file(&withheld.workspace.join("src").join(name));
        assert!(withheld.policy.validate_read(&path).is_err(), "{name}");
    }
}

#[test]
fn allow_never_lifts_sensitive_directories() {
    let layout = layout(true);
    for name in [".ssh/config", ".aws/credentials", ".git/config", ".gnupg/x"] {
        let path = file(&layout.workspace.join(name));
        assert!(layout.policy.validate_read(&path).is_err(), "{name}");
    }
}

/// Octocode's own stored login and the user's home dotfiles stay withheld,
/// directly and through a workspace symlink.
#[test]
fn allow_never_lifts_octocode_credentials_or_home_dotfiles() {
    let layout = layout(true);
    let stored = file(&layout.octocode_home.join("credentials.json"));
    let key = file(&layout.octocode_home.join(".key"));
    let netrc = file(&layout.os_home.join(".netrc"));
    for path in [&stored, &key, &netrc] {
        assert!(layout.policy.validate_read(path).is_err(), "{path:?}");
    }
    #[cfg(unix)]
    {
        let link = layout.workspace.join("innocent.txt");
        std::os::unix::fs::symlink(&stored, &link).expect("link");
        assert!(layout.policy.validate_read(&link).is_err());
    }
    // A project below the home is not "directly in" it.
    let project = file(&layout.os_home.join("other/credentials.json"));
    assert!(layout.policy.validate_read(&project).is_ok());
}

/// A symlinked or differently cased home names the same home, and its dot
/// directories hold tool logins. A project below the home stays a project.
#[test]
fn allow_keeps_home_dot_directories_and_any_spelling_of_the_home() {
    let layout = layout(true);
    for name in [".claude/.credentials.json", ".codex/auth.json", ".npmrc"] {
        let path = file(&layout.os_home.join(name));
        assert!(layout.policy.validate_read(&path).is_err(), "{name}");
    }
    #[cfg(unix)]
    {
        let root = layout.os_home.parent().expect("root");
        let link = root.join("homelink");
        std::os::unix::fs::symlink(&layout.os_home, &link).expect("link");
        let policy = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(layout.os_home.clone()),
            home_dir: Some(link),
            allow_credential_files: true,
            ..Default::default()
        })
        .expect("policy");
        let npmrc = layout.os_home.join(".npmrc");
        assert!(policy.validate_read(&npmrc).is_err());
        let project = file(&layout.os_home.join("code/.env"));
        assert!(policy.validate_read(&project).is_ok());
    }
    let upper = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(layout.os_home.clone()),
        home_dir: Some(PathBuf::from(
            layout.os_home.to_string_lossy().to_uppercase(),
        )),
        allow_credential_files: true,
        ..Default::default()
    })
    .expect("policy");
    assert!(upper.validate_read(layout.os_home.join(".npmrc")).is_err());
}

/// What leaves a local read uses the strict policy, and a write never targets
/// a credential name.
#[test]
fn the_egress_policy_withholds_whatever_the_setting() {
    let layout = layout(true);
    let path = file(&layout.workspace.join(".env"));
    assert!(layout.policy.validate_read(&path).is_ok());
    let egress = layout.policy.withholding_credentials();
    assert!(egress.validate_read(&path).is_err());
    assert!(!egress.allows_credential_files());
}

/// A discovery walk agrees with a direct read: the name-only shortcut must not
/// decide while the setting makes the answer depend on the location.
#[test]
fn a_walk_admits_what_a_read_admits() {
    let layout = layout(true);
    let path = file(&layout.workspace.join("src/.env"));
    assert!(!layout.policy.withholds(&path, EntryKind::File));
    let stored = file(&layout.octocode_home.join("credentials.json"));
    assert!(layout.policy.withholds(&stored, EntryKind::File));
    assert!(
        layout
            .policy
            .withholds(&layout.workspace.join(".ssh"), EntryKind::Dir)
    );
}
