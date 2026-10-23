use super::*;

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
    withheld.record(&sub.join(".git"), false);
    withheld.record(&sub.join("link"), false);
    withheld.record(&root.path().join("gone"), false);
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
