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

/// A disclosure of withheld entries names the directory that holds them, from
/// the walk root, and never an entry itself (B143).
#[test]
fn a_directory_holding_withheld_files_is_not_disclosed_as_the_file() {
    use super::withheld::WithheldKind;
    use crate::tools::leads::skipped::unlifted;
    let root = tempfile::tempdir().expect("root");
    let deploy = root.path().join("deploy");
    std::fs::create_dir_all(&deploy).expect("dirs");
    let (env, key) = (deploy.join(".env"), root.path().join("id_rsa"));
    std::fs::write(&env, "A=1\n").expect("credential file");
    std::fs::write(&key, "k\n").expect("credential file");
    let mut withheld = Withheld::default();
    withheld.record(&env);
    withheld.record(&key);
    let walked = root.path().to_string_lossy().into_owned();
    let entries = unlifted(&walked, &withheld, Default::default());
    // One entry per (reason, directory), shown from the walk root; the root
    // itself only for what sits directly in it.
    let holders = |path: &str| {
        entries
            .iter()
            .filter(|entry| entry.path == path)
            .collect::<Vec<_>>()
    };
    for (directory, held) in [("deploy", ".env"), (walked.as_str(), "id_rsa")] {
        let at = holders(directory);
        assert_eq!(at.len(), 1, "{entries:?}");
        assert_eq!(at[0].count, 1);
        assert_eq!(
            at[0].reason.as_deref(),
            Some(WithheldKind::CredentialFile.reason())
        );
        assert!(
            entries.iter().all(|entry| !entry.path.ends_with(held)),
            "a withheld file is never named: {entries:?}"
        );
    }
    assert_eq!(
        entries.iter().map(|entry| entry.count).sum::<usize>(),
        withheld.total()
    );
    // Two directories holding one file each stay two entries.
    let shown = crate::tools::leads::skipped::skipped_paths(entries, false, false)
        .expect("skipped paths")
        .expect("some skipped")
        .entries;
    assert_eq!(shown.len(), 2, "{shown:?}");
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

/// Credential files keep their names' shapes as real tools write them, not
/// only the one exact name: a kubeconfig under any name a cluster tool or a
/// person gives it, cloud and registry credentials with a qualifier, SSH keys
/// with a suffix, Windows spellings. A source file that only mentions the word
/// stays readable (RV-C3eF live leak, round 12).
#[test]
fn credential_files_are_matched_by_the_names_tools_write() {
    for withheld in [
        "/w/kube/kubeconfig-prod-cluster.yaml",
        "/w/kubeconfig",
        "/w/kubeconfig.yaml",
        "/w/prod.kubeconfig",
        "/w/.kubeconfig",
        "/w/clusters/staging-kubeconfig.yml",
        "/w/KubeConfig-Dev.JSON",
        "/w/etc/kubernetes/admin.conf",
        "/w/etc/kubernetes/super-admin.conf",
        "/w/etc/kubernetes/kubelet.conf",
        "/w/etc/kubernetes/controller-manager.conf",
        "/w/etc/kubernetes/scheduler.conf",
        "/w/etc/rancher/k3s/k3s.yaml",
        "/w/etc/rancher/rke2/rke2.yaml",
        "/w/gcp-credentials.json",
        "/w/application_default_credentials.json",
        "/w/aws_credentials",
        "/w/credentials.csv",
        "/w/alice_accessKeys.csv",
        "/w/app-secrets.yaml",
        "/w/my-project-service-account.json",
        "/w/proj-firebase-adminsdk-ab12c.json",
        "/w/keys/id_rsa_github",
        "/w/keys/id_ed25519_sk",
        "/w/_netrc",
        "/w/AppData/postgresql/pgpass.conf",
        "/w/etc/npmrc",
        "/w/htpasswd",
        "/w/infra/prod.tfstate",
        "/w/infra/prod.tfstate.backup",
        "/w/infra/prod.auto.tfvars",
    ] {
        assert!(
            is_sensitive_path(Path::new(withheld)),
            "withheld: {withheld}"
        );
    }
    for readable in [
        "/w/pkg/kubeconfig.go",
        "/w/pkg/kubeconfig_test.go",
        "/w/docs/kubeconfig.md",
        "/w/src/credentials.ts",
        "/w/src/credentials_store.py",
        "/w/docs/secrets.md",
        "/w/keys/id_rsa.pub",
        "/w/keys/id_rsa_github.pub",
        "/w/src/id_rsa_loader.rs",
        "/w/app/admin.conf",
        "/w/config/k3s-install.md",
        "/w/infra/variables.tf",
        "/w/infra/terraform.tfvars.example",
    ] {
        assert!(
            !is_sensitive_path(Path::new(readable)),
            "readable: {readable}"
        );
    }
}

/// Round 13 (RV-SEC round 12 review): the shapes that still bypassed the
/// name rules are withheld; the ordinary files a substring match withheld are
/// readable again; and a name is judged as Windows opens it (a trailing dot or
/// space dropped). Every case round 12 pinned stays as it was.
#[test]
fn credential_name_rules_close_bypasses_and_free_ordinary_files() {
    for withheld in [
        // Terraform loads JSON variable files; `terraform state` writes
        // timestamped backups.
        "/w/tf/prod.tfvars.json",
        "/w/tf/prod.auto.tfvars.json",
        "/w/tf/terraform.tfstate.1700000000.backup",
        "/w/tf/terraform.tfstate.backup",
        // Google's OAuth client downloads.
        "/w/app/client_secret.json",
        "/w/app/client_secret_1234.apps.googleusercontent.com.json",
        // Editor and rotation copies of a withheld name.
        "/w/kube/kubeconfig.yaml.swp",
        "/w/kube/.kubeconfig.yaml.swp",
        "/w/kube/kubeconfig.yaml.tmp",
        "/w/kube/kubeconfig.yaml.1",
        "/w/keys/.id_rsa.swp",
        "/w/kube/#kubeconfig.yaml#",
        // Windows opens these as `kubeconfig.yaml` and `.env`.
        "/w/win/kubeconfig.yaml.",
        "/w/win/kubeconfig.yaml ",
        "/w/win/.env.",
        // Words on a token boundary, as before.
        "/w/app/app-secrets.yaml",
        "/w/app/secrets.toml",
        "/w/gcp-credentials.json",
        "/w/aws_credentials",
        "/w/my-project-service-account.json",
        "/w/i18n/id_rsa",
        // `sealed`/`external` describe only a secrets manifest: an
        // "external" kubeconfig or credentials file is a real one (N13-1).
        "/w/kube/kubeconfig-external.yaml",
        "/w/app/credentials-external.json",
        "/w/app/sealed-credentials.json",
        // `lang/` is a programming language in SDK layouts, not a
        // translation folder (N13-2).
        "/w/lang/go/credentials.json",
        "/w/langs/python/secrets.yaml",
    ] {
        assert!(
            is_sensitive_path(Path::new(withheld)),
            "withheld: {withheld}"
        );
    }
    for readable in [
        // A Kubernetes ServiceAccount manifest holds no secret.
        "/w/helm/templates/service-account.yaml",
        "/w/docs/service_account.txt",
        // Sealed secrets are encrypted; ExternalSecrets hold references.
        "/w/k8s/sealed-secrets.yaml",
        "/w/k8s/external-secrets.yaml",
        // Descriptions, not the credential.
        "/w/schemas/credentials.schema.json",
        "/w/i18n/credentials.json",
        "/w/locales/en/secrets.json",
        "/w/.github/workflows/secrets.yml",
        // A word inside another word (AWS SDK models).
        "/w/aws-sdk/secretsmanager-2017-10-17.normal.json",
        "/w/src/credentialsprovider.xml",
        // Still readable from round 12.
        "/w/pkg/kubeconfig.go",
        "/w/src/credentials.ts",
        "/w/keys/id_rsa.pub",
    ] {
        assert!(
            !is_sensitive_path(Path::new(readable)),
            "readable: {readable}"
        );
    }
}
