use super::*;

/// A runtime given a pool runs its servers there and leaves them to the
/// pool's owner on close; one built alone owns and stops its own.
#[tokio::test]
async fn a_runtime_on_a_shared_pool_leaves_it_to_its_owner() {
    let home = tempfile::tempdir().expect("home");
    let pool = Arc::new(octocode_engine::lsp::pool::LspClientPool::default());
    let shared = ToolRuntime::from_host_with_lsp_pool(
        HostOptions {
            cwd: Some(home.path().to_path_buf()),
            env: Some(BTreeMap::from([(
                "OCTOCODE_HOME".to_owned(),
                home.path().to_string_lossy().into_owned(),
            )])),
            ..HostOptions::default()
        },
        Arc::clone(&pool),
    )
    .expect("runtime");
    assert!(Arc::ptr_eq(&shared.lsp_pool, &pool));
    assert!(!shared.owns_lsp_pool);
    shared.close().await;
    assert_eq!(Arc::strong_count(&pool), 2, "the pool outlives the close");
    let alone = runtime(home.path(), &[]);
    assert!(alone.owns_lsp_pool);
    assert!(!Arc::ptr_eq(&alone.lsp_pool, &pool));
}

/// The graph workspace and the language-server invocation directory
/// come from the runtime's working directory, so a host serving several
/// directories runs each like a CLI started there.
#[test]
fn the_graph_workspace_and_lsp_invocation_dir_follow_the_runtime_cwd() {
    let home = tempfile::tempdir().expect("home");
    let repo = home.path().join("repo");
    let cwd = repo.join("sub");
    std::fs::create_dir_all(repo.join(".git")).expect("git");
    std::fs::create_dir_all(&cwd).expect("sub");
    let mut input = runtime(home.path(), &[]).input.clone();
    input.cwd = cwd.clone();
    input
        .env
        .insert("WORKSPACE_ROOT".into(), repo.to_string_lossy().into_owned());
    let runtime = ToolRuntime::new(input).expect("runtime");
    assert_ne!(Some(cwd.clone()), std::env::current_dir().ok());
    assert_eq!(runtime.graph_workspace(None), repo);
    assert_eq!(runtime.graph_workspace(Some(&cwd)), cwd);
    let discovery = runtime.lsp_execution_config().discovery(None);
    assert_eq!(discovery.invocation_dir, Some(cwd));
}

/// Catalog `lspServers` resolve against the runtime's workspace
/// (host cwd or configured workspaceRoot), not the process cwd.
#[test]
fn lsp_server_root_is_the_runtime_workspace_not_the_process_cwd() {
    let home = tempfile::tempdir().expect("home");
    let runtime = runtime(home.path(), &[]);
    assert_eq!(runtime.lsp_root(), home.path());
    assert_ne!(
        Some(runtime.lsp_root().to_path_buf()),
        std::env::current_dir().ok()
    );
}

#[test]
fn availability_is_resolved_once_per_runtime() {
    let home = tempfile::tempdir().expect("home");
    let runtime = runtime(
        home.path(),
        &[
            ("OCTOCODE_ENABLE_LOCAL", "false"),
            ("DISABLE_TOOLS", "ghSearchRepo"),
        ],
    );
    let resolved = crate::execution::availability::ToolAvailability::resolve(
        runtime.config(),
        RuntimeSurface::Mcp,
    );
    for id in ToolId::ALL {
        assert_eq!(
            runtime.is_available(id.as_str()),
            resolved.is_available(id.as_str()),
            "{id}"
        );
    }
    assert!(!runtime.is_available("localSearch"));
    assert!(!runtime.is_available("noSuchTool"));
}

#[test]
fn selection_presentation_requires_provider_and_an_available_goal_tool() {
    let home = tempfile::tempdir().expect("home");
    for (env, expected) in [
        (vec![], false),
        (vec![("OCTOCODE_CLASSIFICATION_API", "fixture-key")], true),
        (vec![("OCTOCODE_CLASSIFICATION_API", "fixture-key")], true),
        (
            vec![
                ("OCTOCODE_CLASSIFICATION_API", "fixture-key"),
                ("DISABLE_TOOLS", "clasify"),
            ],
            true,
        ),
        (
            vec![
                ("OCTOCODE_CLASSIFICATION_API", "fixture-key"),
                ("TOOLS_TO_RUN", "ghSearchHistory"),
            ],
            false,
        ),
        (
            vec![
                ("OCTOCODE_CLASSIFICATION_API", "fixture-key"),
                ("TOOLS_TO_RUN", "ghSearchCode"),
            ],
            true,
        ),
        (
            vec![
                ("OCTOCODE_CLASSIFICATION_API", "fixture-key"),
                ("TOOLS_TO_RUN", "clasify"),
            ],
            false,
        ),
        (
            vec![
                ("OCTOCODE_CLASSIFICATION_API", "   "),
                ("DISABLE_TOOLS", "clasify"),
            ],
            false,
        ),
    ] {
        let runtime = runtime(home.path(), &env);
        assert_eq!(runtime.presentation()["selection"], expected, "{env:?}");
    }
}

#[tokio::test]
async fn hidden_public_clasify_keeps_goal_reads_available() {
    let home = tempfile::tempdir().expect("home");
    let runtime = runtime(
        home.path(),
        &[
            ("OCTOCODE_CLASSIFICATION_API", "fixture-key"),
            ("TOOLS_TO_RUN", "ghSearchCode"),
        ],
    );
    assert!(!runtime.is_available("clasify"));
    assert_eq!(runtime.presentation()["selection"], true);
    assert!(!runtime.is_available("clasify"));
    assert!(runtime.dispatcher().selection_available);
}

#[tokio::test]
async fn hidden_public_clasify_without_goal_surface_has_no_semantic_route() {
    let home = tempfile::tempdir().expect("home");
    let runtime = runtime(
        home.path(),
        &[
            ("OCTOCODE_CLASSIFICATION_API", "fixture-key"),
            ("TOOLS_TO_RUN", "ghSearchHistory"),
        ],
    );
    assert_eq!(runtime.presentation()["selection"], false);
    assert!(!runtime.dispatcher().selection_available);
}

/// A file-sourced language-server setting reaches the engine exactly as
/// its environment spelling does (`lsp.cacheDir`).
#[test]
fn file_sourced_lsp_settings_reach_the_engine() {
    let home = tempfile::tempdir().expect("home");
    let mut vars = BTreeMap::new();
    vars.insert(
        "OCTOCODE_HOME".to_owned(),
        home.path().to_string_lossy().into_owned(),
    );
    let runtime = ToolRuntime::new(crate::config::ConfigInput {
        env: vars,
        cwd: home.path().to_path_buf(),
        os_home: home.path().to_path_buf(),
        global_env: FileInput::Missing {
            path: home.path().join(".env"),
        },
        project_env: FileInput::Missing {
            path: home.path().join(".octocode/.env"),
        },
        config_file: FileInput::Read {
            path: home.path().join(".octocoderc"),
            text: r#"{"lsp":{"cacheDir":"/tmp/lsp-cache"}}"#.into(),
        },
        project_config_file: FileInput::Missing {
            path: home.path().join(".octocode/.octocoderc"),
        },
        runtime_surface: RuntimeSurface::Mcp,
    })
    .expect("runtime");
    let env = &runtime.lsp_execution_config().env;
    let name = config::field::LSP_CACHE_DIR.env_name().expect("bound");
    assert_eq!(env.get(name).map(String::as_str), Some("/tmp/lsp-cache"));
}

/// Local tools reach only the GitHub checkouts (clones, materialized
/// trees) and URL captures under the cache home, never the rest of it:
/// credentials and logs stay outside every root.
#[test]
fn cache_home_exposes_only_checkout_trees() {
    let workspace = tempfile::tempdir().expect("workspace");
    let os_home = tempfile::tempdir().expect("os home");
    let home = os_home.path().join(".octocode");
    let credentials = home.join("credentials.json");
    let log = home.join("logs").join("x.log");
    let clone = crate::tools::gh_clone_repo::storage::clones_root(&home).join("o/r/main/f.rs");
    let tree =
        crate::tools::gh_structure::materialize::materialize_root(&home).join("o/r/sha/f.rs");
    let capture = crate::tools::source_capture::url::capture_root(&home).join("sha/source.txt");
    for file in [&credentials, &log, &clone, &tree, &capture] {
        std::fs::create_dir_all(file.parent().expect("parent")).expect("dir");
        std::fs::write(file, "x").expect("file");
    }
    let mut vars = BTreeMap::new();
    vars.insert(
        "OCTOCODE_HOME".to_owned(),
        home.to_string_lossy().into_owned(),
    );
    let runtime = ToolRuntime::new(crate::config::ConfigInput {
        env: vars,
        cwd: workspace.path().to_path_buf(),
        os_home: os_home.path().to_path_buf(),
        global_env: FileInput::Missing {
            path: os_home.path().join(".env"),
        },
        project_env: FileInput::Missing {
            path: workspace.path().join(".octocode/.env"),
        },
        config_file: FileInput::Missing {
            path: os_home.path().join(".octocoderc"),
        },
        project_config_file: FileInput::Missing {
            path: workspace.path().join(".octocode/.octocoderc"),
        },
        runtime_surface: RuntimeSurface::Mcp,
    })
    .expect("runtime");
    for denied in [&credentials, &log, &home] {
        assert!(
            runtime.paths.validate(denied).is_err(),
            "{}",
            denied.display()
        );
    }
    for allowed in [&clone, &tree, &capture] {
        assert!(
            runtime.paths.validate(allowed).is_ok(),
            "{}",
            allowed.display()
        );
    }
}
