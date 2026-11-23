use super::*;

/// A path not looked at on disk: the file rules apply.
fn is_sensitive_path(path: &Path) -> bool {
    super::is_sensitive_path(path, EntryKind::Unknown)
}

/// Issue #477: only a directory the caller knows is one bypasses the
/// file-name rules; a sensitive ancestor withholds whatever the kind.
#[test]
fn the_entry_kind_decides_file_name_rules() {
    let folder = Path::new("/tmp/work/src/credentials");
    assert!(!super::is_sensitive_path(folder, EntryKind::Dir));
    assert!(super::is_sensitive_path(folder, EntryKind::File));
    assert!(super::is_sensitive_path(folder, EntryKind::Unknown));
    assert!(super::is_sensitive_path(
        Path::new("/tmp/work/.ssh"),
        EntryKind::Dir
    ));
    assert!(super::is_sensitive_path(
        Path::new("/tmp/work/.aws/config"),
        EntryKind::Dir
    ));
    let dir = tempfile::tempdir().expect("tempdir");
    let on_disk = dir.path().join("credentials");
    assert_eq!(EntryKind::stat(&on_disk), EntryKind::Unknown);
    std::fs::create_dir(&on_disk).expect("dir");
    assert_eq!(EntryKind::stat(&on_disk), EntryKind::Dir);
    std::fs::write(on_disk.join("x.pem"), "").expect("file");
    assert_eq!(EntryKind::stat(&on_disk.join("x.pem")), EntryKind::File);
}

#[test]
fn discovery_and_access_lists_cover_sensitive_paths() {
    assert!(crate::policy::prune::defaults().any(|name| name == ".ssh"));
    assert!(
        CREDENTIAL_FILE_EXTENSIONS
            .iter()
            .any(|extension| "private-key.pem".ends_with(extension))
    );
    assert!(is_sensitive_path(Path::new("/tmp/work/.aws/credentials")));
    assert!(!is_sensitive_path(Path::new("/tmp/work/src/lib.rs")));
}

/// A directory named `secrets/` or `private/` is ordinary source;
/// only a credential filename pattern withholds a file inside it.
#[test]
fn directory_names_never_withhold_only_credential_filenames_do() {
    for visible in [
        "/tmp/work/src/private/util.ts",
        "/tmp/work/secrets/README.md",
        "/tmp/work/src/secrets/store.ts",
        "/private/var/folders/x/work/src/lib.rs",
        "/private/tmp/work/main.rs",
    ] {
        assert!(!is_sensitive_path(Path::new(visible)), "{visible}");
    }
    for withheld in [
        "/tmp/work/secrets/prod.env",
        "/tmp/work/private/key.pem",
        "/tmp/work/private/id_rsa",
        "/tmp/work/secrets/.env.local",
        "/tmp/work/private/release.keystore",
        "/tmp/work/secrets/credentials.json",
    ] {
        assert!(is_sensitive_path(Path::new(withheld)), "{withheld}");
        assert_eq!(sensitive_directory(withheld), None, "{withheld}");
        assert!(
            withheld_message("x", Path::new(withheld)).contains("a credential file name"),
            "{withheld}"
        );
    }
}

/// Read, tree and search share one credential list, so every name
/// the search excludes is withheld from reads too, and the stores the
/// lists used to miss are covered.
#[test]
fn one_credential_list_covers_read_and_search() {
    for name in CREDENTIAL_FILE_NAMES {
        assert!(
            is_sensitive_path(&Path::new("/tmp/work").join(name)),
            "{name}"
        );
    }
    for name in [
        "secrets.json",
        "credentials.json",
        "service-account.json",
        "auth.json",
        "api-keys.json",
        "keyfile",
        "google-services.json",
        ".pgpass",
        ".htpasswd",
        ".vault-token",
        ".s3cfg",
        ".boto",
        ".my.cnf",
        ".dockercfg",
        ".envrc",
        ".flaskenv",
        "app.env",
        "config.env",
        ".env.local",
        "id_rsa.old",
        "deploy.ppk",
        "credentials.json.bak",
    ] {
        assert!(
            is_sensitive_path(&Path::new("/tmp/work").join(name)),
            "{name}"
        );
    }
    for name in [
        "environment.ts",
        "env.rs",
        ".envoy.yaml",
        "keys.rs",
        "auth.ts",
    ] {
        assert!(
            !is_sensitive_path(&Path::new("/tmp/work").join(name)),
            "{name}"
        );
    }
}

/// The notice names what it withheld: a `.git` pointer file is not
/// a credential file, and a symlink the walk did not follow is not one
/// outside the allowed roots.
#[cfg(unix)]
#[test]
fn withheld_notice_labels_git_files_and_unfollowed_symlinks() {
    let root = tempfile::tempdir().expect("root");
    let sub = root.path().join("sub");
    std::fs::create_dir_all(sub.join("real")).expect("dirs");
    std::fs::write(sub.join(".git"), "gitdir: ../.git/modules/sub\n").expect("gitfile");
    std::os::unix::fs::symlink(sub.join("real"), sub.join("link")).expect("link");
    let mut withheld = Withheld::default();
    withheld.record(&sub.join(".git"));
    withheld.record(&sub.join("link"));
    withheld.record(&root.path().join("gone"));
    let notice = withheld.notice().expect("notice");
    assert_eq!(
        notice,
        "3 entries withheld by path policy: 1 `.git` file (submodule or worktree pointers); 1 symlink not followed; 1 entry outside the allowed roots or unresolvable.",
    );
    assert!(
        withheld_message("sub/.git", &sub.join(".git")).contains("a `.git` file"),
        "the read refusal names the pointer file too"
    );
}

#[test]
fn access_list_covers_gh_and_gpg_credentials() {
    assert!(is_sensitive_path(Path::new(
        "/home/user/.config/gh/hosts.yml"
    )));
    assert!(is_sensitive_path(Path::new("/home/user/.config/gh")));
    assert!(is_sensitive_path(Path::new("/home/user/.config/hub")));
    assert!(is_sensitive_path(Path::new(
        "/home/user/.gnupg/private-keys-v1.d/anything"
    )));
    assert!(is_sensitive_path(Path::new(
        "/home/user/.docker/config.json"
    )));
}

#[test]
fn keymap_sources_are_not_source_maps() {
    assert_eq!(generated_file_pattern("speakupmap.map"), None);
    assert_eq!(
        generated_file_pattern("app.js.map").as_deref(),
        Some("*.js.map")
    );
    assert_eq!(
        generated_file_pattern("index.d.ts.map").as_deref(),
        Some("*.ts.map")
    );
    assert_eq!(
        generated_file_pattern("site.min.css").as_deref(),
        Some("*.min.css")
    );
}

/// A case-insensitive filesystem opens `.SSH` for `.ssh`, and APFS folds `ſ` to
/// `s` and `K` (Kelvin) to `k` as well. The policy cannot tell what a volume
/// folds without asking it for every path, so it withholds every spelling, on
/// every filesystem.
#[test]
fn withheld_names_match_in_any_case() {
    for withheld in [
        "/tmp/work/.SSH/config",
        "/tmp/work/.Ssh/known",
        "/tmp/work/.ſsh/config",
        "/tmp/work/sub/.AWS/credentials",
        "/tmp/work/.GIT/config",
        "/tmp/work/sub/key.PEM",
        "/tmp/work/.ENV",
        "/tmp/work/.Env.local",
        "/tmp/work/sub/Credentials.JSON",
        "/tmp/work/sub/ID_RSA.BAK",
        "/tmp/work/id_rsa.OLD",
        "/tmp/work/ID_RSA",
        "/tmp/work/a/.CONFIG/gh/hosts.yml",
        "/tmp/work/a/.config/GH/hosts.yml",
        "/tmp/work/library/keychains/k.db",
        "/tmp/work/googleservice-info.PLIST",
        "/tmp/work/app.\u{212A}EY",
        "/tmp/work/key\u{FB01}le",
        "/tmp/work/.pgpa\u{DF}",
    ] {
        assert!(is_sensitive_path(Path::new(withheld)), "{withheld}");
    }
    for visible in [
        "/tmp/work/SRC/LIB.RS",
        "/tmp/work/ENV.RS",
        "/tmp/work/Auth.ts",
    ] {
        assert!(!is_sensitive_path(Path::new(visible)), "{visible}");
    }
}

/// The walk's shortcut under a cleared directory says what the whole path says.
#[test]
fn a_walk_withholds_names_in_any_case() {
    use std::ffi::OsStr;
    let under = |name: &str, kind| is_sensitive_under_cleared(OsStr::new(name), kind);
    assert_eq!(under(".SSH", EntryKind::Dir), Some(true));
    assert_eq!(under(".Git", EntryKind::Dir), Some(true));
    assert_eq!(under(".ſsh", EntryKind::Dir), Some(true));
    assert_eq!(under(".ENV", EntryKind::File), Some(true));
    assert_eq!(under("KEY.PEM", EntryKind::File), Some(true));
    assert_eq!(under("ID_RSA.BAK", EntryKind::File), Some(true));
    assert_eq!(under("LIB.RS", EntryKind::File), Some(false));
    assert_eq!(under("GH", EntryKind::Dir), None, "completes .config/gh");
    assert_eq!(under("Keychains", EntryKind::Dir), None);
}

/// The notice and the refusal count a spelled-otherwise entry as the entry it is.
#[cfg(unix)]
#[test]
fn withheld_notice_labels_any_case_of_a_git_file() {
    let root = tempfile::tempdir().expect("root");
    let pointer = root.path().join(".GIT");
    std::fs::write(&pointer, "gitdir: ../.git/modules/sub\n").expect("gitfile");
    let mut withheld = Withheld::default();
    withheld.record(&pointer);
    assert_eq!(withheld.git_files, 1);
    assert!(withheld_message(".GIT", &pointer).contains("a `.git` file"));
}

/// The key beside Octocode's encrypted `credentials.json` has no stem, so the
/// extension rule alone would leave it readable by every local tool.
#[test]
fn the_bare_key_file_is_a_credential() {
    assert!(super::is_sensitive_path(
        Path::new("/h/.octocode/.key"),
        EntryKind::File
    ));
    assert!(super::is_sensitive_path(
        Path::new("/h/.octocode/.key.bak"),
        EntryKind::File
    ));
    assert!(!super::is_sensitive_path(
        Path::new("/h/.octocode/.key"),
        EntryKind::Dir
    ));
}

/// A repository names its files from its root: the same rule as a local path,
/// however the request spells the path.
#[test]
fn a_repository_file_passes_the_one_name_rule() {
    for withheld in [
        ".env",
        ".Env.Prod",
        "KEY.PEM",
        "deploy/server.pem",
        ".ssh/config",
        "a/.SSH/config",
        "./.env",
        "a/../.env",
        "a//.env",
        ".env/",
        "../.env",
        ".config/gh/hosts.yml",
        "Library/Keychains/login.keychain",
        "id_rsa.bak",
        "a\\.ssh\\config",
    ] {
        assert!(is_sensitive_repository_file(withheld), "{withheld}");
    }
    for kept in [
        "src/credentials/mod.rs",
        ".github/workflows/ci.yml",
        "docs/secrets/readme.md",
        ".environment/setup.md",
        "README.md",
        ".env/../README.md",
    ] {
        assert!(!is_sensitive_repository_file(kept), "{kept}");
    }
}

/// The known credential files are withheld however a tool or a filesystem
/// spells them: any case, a backup copy, a Windows trailing dot, and a
/// store at its known place (kubeadm's `/etc/kubernetes/admin.conf`).
#[test]
fn known_credential_files_are_withheld_in_every_spelling() {
    let names = CREDENTIAL_FILE_NAMES
        .iter()
        .map(|name| (*name).to_owned())
        .chain(
            CREDENTIAL_FILE_EXTENSIONS
                .iter()
                .map(|extension| format!("store{extension}")),
        )
        .chain([".env", ".env.production", "app.env", "id_ed25519_github"].map(str::to_owned));
    for name in names {
        for spelled in [
            name.clone(),
            name.to_ascii_uppercase(),
            format!("{name}.bak"),
            format!("{name}~"),
            format!("{name}."),
        ] {
            assert!(
                is_sensitive_path(&Path::new("/w/project").join(&spelled)),
                "withheld: {spelled}"
            );
        }
    }
    for name in KUBEADM_CONFIGS {
        assert!(is_sensitive_path(
            &Path::new("/w/etc/kubernetes").join(name)
        ));
        assert!(!is_sensitive_path(&Path::new("/w/app").join(name)));
    }
}

/// Only the known list withholds a file. A shell's history and rc files, a
/// file whose name merely holds a credential word, public keys and
/// certificates, and the source and docs about credentials read like any
/// file: what they hold of a known secret format is redacted.
#[test]
fn a_file_outside_the_known_list_is_readable() {
    for readable in [
        "/w/home/.zsh_history",
        "/w/home/.bash_history",
        "/w/home/.zshrc",
        "/w/home/.bashrc",
        "/w/home/.profile",
        "/w/kube/kubeconfig-prod-cluster.yaml",
        "/w/app/app-secrets.yaml",
        "/w/app/secrets.toml",
        "/w/gcp-credentials.json",
        "/w/certs/server.crt",
        "/w/keys/release.asc",
        "/w/keys/id_rsa.pub",
        "/w/pkg/kubeconfig.go",
        "/w/src/credentials.ts",
        "/w/docs/secrets.md",
        "/w/infra/variables.tf",
    ] {
        assert!(
            !is_sensitive_path(Path::new(readable)),
            "readable: {readable}"
        );
    }
}

/// A refused `.git/` path points at the commit a search or read inside the
/// checkout already states, which is what reading `.git/HEAD` was for.
#[test]
fn a_refused_git_directory_points_at_the_stated_commit() {
    let message = withheld_message("repo/.git", Path::new("/w/repo/.git/HEAD"));
    assert!(message.contains("commitSha"), "{message}");
    let credential = withheld_message("x", Path::new("/w/.ssh/id_rsa"));
    assert!(!credential.contains("commitSha"), "{credential}");
}
