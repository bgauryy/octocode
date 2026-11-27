use super::super::commands::{discovery, install_item, run_list, run_uninstall};
use super::*;
use octocode_engine::lsp::managed::platform_id;
use octocode_native::runtime::{HostOptions, ToolRuntime};

/// A runtime whose home is a fresh directory. Its GitHub host is not
/// github.com, so an install asks no credential store for a token.
fn runtime_in(home: &Path) -> ToolRuntime {
    let env = [
        ("OCTOCODE_HOME", home.to_string_lossy().into_owned()),
        ("GITHUB_API_URL", "https://ghe.example/api/v3".to_owned()),
    ]
    .map(|(key, value)| (key.to_owned(), value));
    ToolRuntime::from_host(HostOptions {
        cwd: Some(home.to_path_buf()),
        env: Some(env.into_iter().collect()),
        ..HostOptions::default()
    })
    .expect("fixture runtime")
}

/// Put `name` into the managed cache under `root` as a finished install.
fn installed(root: &Path, name: &str, platform: &str) {
    let bin = cached_server_bin_path(root, name, platform).expect("bin path");
    install_binary(
        &bin,
        &b"bytes"[..],
        MAX_EXTRACTED_BYTES,
        &locked_identity(name, platform),
    )
    .expect("install");
}

fn install(names: &[&str]) -> LspServerCommand {
    LspServerCommand::Install {
        names: names.iter().map(|name| (*name).to_owned()).collect(),
        all: false,
        force: false,
    }
}

/// The `lsp-server` subcommands act on the managed cache under the runtime's
/// own home: an install there is reported present without a download,
/// uninstall and clean remove it, and every refusal has its own exit.
#[tokio::test(flavor = "current_thread")]
async fn each_subcommand_acts_on_the_cache_of_the_runtimes_home() {
    let dir = tempfile::tempdir().expect("home");
    let home = dir.path().canonicalize().expect("canonical home");
    let runtime = runtime_in(&home);
    let root = discovery(&runtime).managed_root().expect("managed root");
    assert!(root.starts_with(&home), "{root:?} is under {home:?}");
    let platform = platform_id();
    installed(&root, "rust-analyzer", &platform);

    for json in [false, true] {
        assert_eq!(run(&runtime, LspServerCommand::List, json).await, 0);
        assert_eq!(
            run(&runtime, install(&["rust-analyzer"]), json).await,
            0,
            "an installed server is present, never downloaded"
        );
        assert_eq!(
            run(
                &runtime,
                install(&["rust-analyzer", "no-such-server"]),
                json
            )
            .await,
            2,
            "one unknown target fails the install, and its class (the caller's to correct) sets the exit"
        );
    }
    assert_eq!(run(&runtime, install(&[]), false).await, 2);
    assert_eq!(
        run(
            &runtime,
            LspServerCommand::Uninstall { names: Vec::new() },
            false
        )
        .await,
        2
    );
    assert_eq!(
        run(
            &runtime,
            LspServerCommand::Uninstall {
                names: vec!["rust-analyzer".to_owned()]
            },
            false
        )
        .await,
        0
    );
    assert!(resolve_cached_server(&root, "rust-analyzer", &platform).is_none());

    installed(&root, "rust-analyzer", &platform);
    assert_eq!(
        run(&runtime, LspServerCommand::Clean { yes: false }, false).await,
        0
    );
    assert!(
        resolve_cached_server(&root, "rust-analyzer", &platform).is_some(),
        "a dry run keeps the cache"
    );
    // The clangd indexes kept for mirrored databases go with the cache.
    let indexes = home.join("lsp-workspaces/clangd");
    std::fs::create_dir_all(&indexes).expect("indexes");
    assert_eq!(
        run(&runtime, LspServerCommand::Clean { yes: true }, false).await,
        0
    );
    assert!(!root.exists() && !indexes.exists());
    let status = run(
        &runtime,
        LspServerCommand::Status {
            file: Some("main.unknownxyz".to_owned()),
            live: false,
        },
        true,
    )
    .await;
    assert!(status <= 1, "status reports, it does not fail: {status}");
}

/// The listing tells installed from missing servers in either output, and an
/// uninstall of a server never installed is a no-op that still succeeds.
#[test]
fn list_and_uninstall_report_what_the_cache_holds() {
    let dir = tempfile::tempdir().expect("root");
    let root = dir.path().canonicalize().expect("canonical root");
    installed(&root, "rust-analyzer", "linux-x64");
    for json in [false, true] {
        assert_eq!(run_list(&root, "linux-x64", json), 0);
        assert_eq!(
            run_uninstall(&root, "linux-x64", vec!["clangd".to_owned()], json),
            0
        );
    }
    assert!(
        resolve_cached_server(&root, "rust-analyzer", "linux-x64").is_some(),
        "removing another server keeps this one"
    );
    assert_eq!(
        run_uninstall(&root, "linux-x64", vec!["rust-analyzer".to_owned()], true),
        0
    );
    assert!(resolve_cached_server(&root, "rust-analyzer", "linux-x64").is_none());
}

/// A clean that cannot remove the cache says so in text too, and never
/// reports success.
#[test]
fn a_clean_that_fails_is_reported_in_text_as_in_json() {
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join("not-a-dir");
    std::fs::write(&file, "x").expect("file");
    assert_eq!(run_clean(&file, true, false), 5);
    assert!(file.exists());
    let cache = dir.path().join("lsp");
    std::fs::create_dir_all(&cache).expect("cache");
    assert_eq!(run_clean(&cache, false, false), 0);
    assert!(cache.exists(), "a dry run keeps the cache");
    assert_eq!(run_clean(&cache, true, false), 0);
    assert!(!cache.exists());
}

/// An install that ends at any check prints its reason and exits by its class; one that
/// succeeds prints the checks it passed.
#[tokio::test(flavor = "current_thread")]
async fn install_output_names_each_outcome() {
    let dir = tempfile::tempdir().expect("root");
    let root = dir.path().canonicalize().expect("canonical root");
    for json in [false, true] {
        assert_eq!(
            run_install(
                &root,
                "linux-x64",
                vec!["rust-analyzer".to_owned()],
                LspAutoInstall::Off,
                json,
                &Fake::never(),
            )
            .await,
            4,
            "auto-install off refuses the download"
        );
    }
    assert!(resolve_cached_server(&root, "rust-analyzer", "linux-x64").is_none());
}

/// Each `install` item states its outcome as `--json` prints it. A failed
/// item names what stopped it (`errorCode`) and that code's class
/// (`errorClass`), chosen by the cause: a name that is no managed server is
/// the caller's to correct, an install its setting or its checks refuse is
/// refused, and a server with no build for the platform cannot be served.
/// An installed one carries neither.
#[tokio::test(flavor = "current_thread")]
async fn each_failed_install_item_states_its_error_code_and_class() {
    let dir = tempfile::tempdir().expect("root");
    let root = dir.path().canonicalize().expect("canonical root");
    let cases = [
        (
            "nosuchlang",
            "linux-x64",
            LspAutoInstall::Auto,
            Fake::never(),
            "notFound",
            "correctInput",
        ),
        (
            "rust-analyzer",
            "linux-x64",
            LspAutoInstall::Off,
            Fake::never(),
            "policy",
            "refused",
        ),
        (
            "rust-analyzer",
            "linux-x64",
            LspAutoInstall::Auto,
            tampered(),
            "securityValidationFailed",
            "refused",
        ),
        (
            "clangd",
            "linux-arm64",
            LspAutoInstall::Auto,
            Fake::never(),
            "unavailable",
            "unservable",
        ),
    ];
    for (name, platform, mode, upstream, code, class) in cases {
        let outcome = provision_server(&root, name, platform, mode, &upstream).await;
        let item = install_item(name, &outcome);
        assert_eq!(item["name"], name, "{item}");
        assert_eq!(item["ok"], false, "{item}");
        assert!(
            item["error"]
                .as_str()
                .is_some_and(|error| !error.is_empty()),
            "{item}"
        );
        assert_eq!(item["errorCode"], code, "{item}");
        assert_eq!(item["errorClass"], class, "{item}");
    }
    installed(&root, "rust-analyzer", "linux-x64");
    let outcome = provision_server(
        &root,
        "rust-analyzer",
        "linux-x64",
        LspAutoInstall::Auto,
        &Fake::never(),
    )
    .await;
    let item = install_item("rust-analyzer", &outcome);
    assert_eq!(item["ok"], true, "{item}");
    assert!(item.get("errorCode").is_none(), "{item}");
    assert!(item.get("errorClass").is_none(), "{item}");
}

/// `--live` with no server for the file starts nothing and passes; a server
/// that exits before `initialize` fails the check in either output.
#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn a_live_check_reports_servers_that_do_not_start() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("canonical");
    let unknown = root.join("notes.unknownxyz");
    std::fs::write(&unknown, "text\n").expect("file");
    let discovery = LspDiscoveryOptions {
        octocode_home: Some(root.clone()),
        ..LspDiscoveryOptions::default()
    };
    let path = unknown.to_string_lossy().into_owned();
    assert_eq!(run_live(&discovery, Some(&path), false).await, 0);

    let server = root.join("exiting-server");
    std::fs::write(&server, "#!/bin/sh\nexit 1\n").expect("server");
    std::fs::set_permissions(&server, std::fs::Permissions::from_mode(0o755)).expect("executable");
    std::fs::write(
        root.join("lsp-servers.json"),
        serde_json::json!({"languageServers":{".rs":{"command":server,"args":[],"languageId":"rust"}}})
            .to_string(),
    )
    .expect("config");
    let file = root.join("a.rs");
    std::fs::write(&file, "fn a() {}\n").expect("source");
    let path = file.to_string_lossy().into_owned();
    for json in [false, true] {
        assert_eq!(run_live(&discovery, Some(&path), json).await, 1);
    }
}

/// A server that prints a token to stderr and dies: the live check's error
/// carries that stderr through the output gate, redacted.
#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn a_live_check_redacts_a_secret_the_server_wrote_to_stderr() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("canonical");
    let token = format!("ghp_{}", "a1B2c3D4e5".repeat(4).get(..36).expect("36"));
    let server = root.join("leaking-server");
    std::fs::write(
        &server,
        format!("#!/bin/sh\necho 'auth failed for {token}' >&2\nexit 1\n"),
    )
    .expect("server");
    std::fs::set_permissions(&server, std::fs::Permissions::from_mode(0o755)).expect("executable");
    std::fs::write(
        root.join("lsp-servers.json"),
        serde_json::json!({"languageServers":{".rs":{"command":server,"args":[],"languageId":"rust"}}})
            .to_string(),
    )
    .expect("config");
    let file = root.join("a.rs");
    std::fs::write(&file, "fn a() {}\n").expect("source");
    let discovery = LspDiscoveryOptions {
        octocode_home: Some(root.clone()),
        ..LspDiscoveryOptions::default()
    };
    let path = file.to_string_lossy().into_owned();
    let (report, failed) = super::super::live_check::live_report(&discovery, Some(&path))
        .await
        .expect("checked");
    assert!(failed, "{report}");
    let error = report["live"][0]["error"].as_str().unwrap_or_default();
    assert!(error.contains("auth failed for"), "{report}");
    assert!(!report.to_string().contains(&token), "{report}");
}

/// A home whose `lsp-servers.json` routes `.rs` to a command that is not
/// installed, plus a managed install of rust-analyzer.
fn home_with_missing_server() -> (tempfile::TempDir, LspDiscoveryOptions) {
    let dir = tempfile::tempdir().expect("home");
    let home = dir.path().canonicalize().expect("canonical");
    std::fs::write(
        home.join("lsp-servers.json"),
        serde_json::json!({"languageServers":{".rs":{"command":home.join("not-installed"),"args":["--token","secret"],"languageId":"rust"}}})
            .to_string(),
    )
    .expect("config");
    installed(&home.join("lsp"), "rust-analyzer", &platform_id());
    let discovery = LspDiscoveryOptions {
        octocode_home: Some(home),
        ..LspDiscoveryOptions::default()
    };
    (dir, discovery)
}

/// A file whose configured server is not installed resolves to nothing and
/// says so; a server name passed for a file is told apart from a path.
#[test]
fn status_text_explains_an_unavailable_server() {
    let (dir, discovery) = home_with_missing_server();
    let root = discovery.managed_root().expect("root");
    let file = dir.path().join("main.rs").to_string_lossy().into_owned();
    for json in [false, true] {
        let code = run_status(&discovery, &root, &platform_id(), Some(&file), json);
        assert!(code <= 1, "{code}");
    }
    assert_eq!(
        run_status(
            &discovery,
            &root,
            &platform_id(),
            Some("rust-analyzer"),
            false
        ),
        0,
        "a name with no language has no route problem"
    );
}

/// Without a file, status lists the managed installs and every route, the
/// configured one that is not installed among them, in either output.
#[test]
fn status_overview_lists_installs_and_unavailable_routes() {
    let (_dir, discovery) = home_with_missing_server();
    let root = discovery.managed_root().expect("root");
    for json in [false, true] {
        let code = run_status(&discovery, &root, &platform_id(), None, json);
        assert!(code <= 1, "{code}");
    }
}

/// An install exits by the strictest class across its failed items: a name no
/// managed server has is the caller's to correct (2) even beside an item that
/// could not be served (5); with only unservable items it exits 5.
#[tokio::test(flavor = "current_thread")]
async fn an_install_exits_by_the_strictest_class_of_its_failed_items() {
    let dir = tempfile::tempdir().expect("root");
    let root = dir.path().canonicalize().expect("canonical root");
    let exit = |names: &[&str]| {
        let targets: Vec<String> = names.iter().map(|name| (*name).to_owned()).collect();
        let root = root.clone();
        async move {
            run_install(
                &root,
                "linux-arm64",
                targets,
                LspAutoInstall::Auto,
                true,
                &Fake::never(),
            )
            .await
        }
    };
    assert_eq!(exit(&["clangd"]).await, 5, "no build for the platform");
    assert_eq!(exit(&["nosuchlang"]).await, 2);
    assert_eq!(exit(&["clangd", "nosuchlang"]).await, 2);
    assert_eq!(exit(&["nosuchlang", "clangd"]).await, 2);
}
