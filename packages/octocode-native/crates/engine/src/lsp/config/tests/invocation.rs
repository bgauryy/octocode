use super::*;

#[test]
fn keeps_unknown_server_commands_unchanged() {
    assert_eq!(
        resolve_known_server_command("definitely-not-an-octocode-server"),
        "definitely-not-an-octocode-server"
    );
}

#[cfg(unix)]
#[test]
fn wraps_non_executable_typescript_cli_with_current_node() {
    let root = TmpDir::new();
    let cli = root
        .join("node_modules")
        .join("typescript-language-server")
        .join("lib")
        .join("cli.mjs");
    std::fs::create_dir_all(cli.parent().unwrap_or(&root))
        .expect("create temporary typescript-language-server dir");
    std::fs::write(&cli, "#!/usr/bin/env node\n").expect("write temporary cli");

    let Some(root_str) = root.to_str() else {
        panic!("temporary root is not utf-8");
    };
    let Some(cli_str) = cli.to_str() else {
        panic!("temporary cli path is not utf-8");
    };

    let (command, args, _) =
        resolve_server_invocation(cli_str, vec!["--stdio".to_owned()], root_str, false, None);
    assert_eq!(Some(command), current_node_command());
    assert_eq!(args.len(), 2);
    assert_eq!(
        PathBuf::from(&args[0]),
        std::fs::canonicalize(&cli).expect("canonicalize temporary cli")
    );
    assert_eq!(args[1], "--stdio");
}

#[cfg(unix)]
#[test]
fn resolves_bundled_typescript_server_from_current_dir_for_external_workspace() {
    let workspace_root = TmpDir::new();
    let package_root = TmpDir::new();
    let cli = package_root
        .join("node_modules")
        .join("typescript-language-server")
        .join("lib")
        .join("cli.mjs");
    std::fs::create_dir_all(&workspace_root).expect("create external workspace");
    std::fs::create_dir_all(cli.parent().unwrap_or(&package_root))
        .expect("create bundled typescript-language-server dir");
    std::fs::write(&cli, "#!/usr/bin/env node\n").expect("write temporary cli");

    let Some(workspace_str) = workspace_root.to_str() else {
        panic!("temporary workspace path is not utf-8");
    };
    let (command, args, source) = resolve_server_invocation_with_environment(
        "typescript-language-server",
        vec!["--stdio".to_owned()],
        workspace_str,
        &crate::lsp::config::invocation::TypeScriptServerSearch {
            managed_start: None,
            current_dir: Some(&package_root),
            workspace_trusted: false,
        },
        false,
    );

    assert_eq!(Some(command), current_node_command());
    assert_eq!(
        PathBuf::from(&args[0]),
        std::fs::canonicalize(&cli).expect("canonicalize temporary cli")
    );
    assert_eq!(args[1], "--stdio");
    assert_eq!(source, crate::lsp::types::ServerSource::Workspace);
}

/// A scanned checkout's own `node_modules` cannot supply the
/// TypeScript server unless the workspace is trusted (opt-in, or the
/// user's own invocation directory); octocode's install tree and the
/// invocation directory are searched first.
#[cfg(unix)]
#[test]
fn workspace_typescript_server_requires_a_trusted_workspace() {
    let base = TmpDir::new();
    let write_cli = |root: &std::path::Path| {
        let cli = root
            .join("node_modules")
            .join("typescript-language-server")
            .join("lib")
            .join("cli.mjs");
        std::fs::create_dir_all(cli.parent().unwrap_or(root)).expect("cli dir");
        std::fs::write(&cli, "#!/usr/bin/env node\n").expect("cli");
        std::fs::canonicalize(&cli).expect("canonical cli")
    };
    let checkout = base.join("clone");
    let checkout_cli = write_cli(&checkout);
    let elsewhere = base.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).expect("elsewhere");
    let checkout_str = checkout.to_string_lossy().into_owned();
    let resolve = |search: &crate::lsp::config::invocation::TypeScriptServerSearch<'_>| {
        crate::lsp::config::invocation::resolve_typescript_server_cli(
            "typescript-language-server",
            &checkout_str,
            search,
            false,
        )
    };

    // Untrusted checkout, nothing else: no server (never the checkout's).
    let untrusted = crate::lsp::config::invocation::TypeScriptServerSearch {
        managed_start: None,
        current_dir: Some(&elsewhere),
        workspace_trusted: false,
    };
    assert_eq!(resolve(&untrusted), None);

    // Trusted (opt-in or invocation root): the workspace's server.
    let trusted = crate::lsp::config::invocation::TypeScriptServerSearch {
        workspace_trusted: true,
        ..untrusted
    };
    assert_eq!(
        resolve(&trusted),
        Some((
            checkout_cli.clone(),
            crate::lsp::types::ServerSource::Workspace
        ))
    );

    // octocode's own install tree wins over the workspace.
    let install = base.join("octocode-install");
    let managed_cli = write_cli(&install);
    let exe = install.join("bin").join("octocode");
    let managed = crate::lsp::config::invocation::TypeScriptServerSearch {
        managed_start: Some(&exe),
        ..trusted
    };
    assert_eq!(
        resolve(&managed),
        Some((managed_cli, crate::lsp::types::ServerSource::Bundled))
    );

    // On PATH: the command runs as-is.
    assert_eq!(
        crate::lsp::config::invocation::resolve_typescript_server_cli(
            "typescript-language-server",
            &checkout_str,
            &trusted,
            true
        ),
        None
    );

    // The invocation root: the workspace itself or a directory inside it.
    assert!(
        crate::lsp::config::invocation::workspace_is_invocation_root(
            &checkout_str,
            Some(&checkout)
        )
    );
    assert!(
        crate::lsp::config::invocation::workspace_is_invocation_root(
            &checkout.join("node_modules").to_string_lossy(),
            Some(&checkout)
        )
    );
    assert!(
        !crate::lsp::config::invocation::workspace_is_invocation_root(
            &checkout_str,
            Some(&elsewhere)
        )
    );
    assert!(
        !crate::lsp::config::invocation::workspace_is_invocation_root(
            &checkout_str,
            Some(std::path::Path::new("/"))
        )
    );
    if let Some(home) = std::env::home_dir() {
        assert!(
            !crate::lsp::config::invocation::workspace_is_invocation_root(
                &home.join(".octocode/clones/x").to_string_lossy(),
                Some(&home)
            ),
            "a home-directory invocation does not trust every checkout under it"
        );
    }

    // The search trusts and looks up from the directory the host runs
    // the call for, never from the process's own directory: a server
    // answering for several directories decides per call.
    assert_ne!(std::env::current_dir().ok(), Some(checkout.clone()));
    let from = |dir: Option<&std::path::Path>| {
        let search = crate::lsp::config::invocation::TypeScriptServerSearch::new(
            None,
            &checkout_str,
            false,
            dir,
        );
        (
            search.workspace_trusted,
            search.current_dir.map(PathBuf::from),
        )
    };
    assert_eq!(from(Some(&checkout)), (true, Some(checkout.clone())));
    assert_eq!(from(Some(&elsewhere)), (false, Some(elsewhere.clone())));
    assert_eq!(from(None), (false, None));
    assert_eq!(
        resolve(
            &crate::lsp::config::invocation::TypeScriptServerSearch::new(
                None,
                &checkout_str,
                false,
                Some(&checkout)
            )
        ),
        Some((
            checkout_cli.clone(),
            crate::lsp::types::ServerSource::Workspace
        )),
        "invoked in the checkout, its server is the user's own"
    );
    assert_eq!(
        resolve(
            &crate::lsp::config::invocation::TypeScriptServerSearch::new(
                None,
                &checkout_str,
                false,
                Some(&elsewhere)
            )
        ),
        None,
        "invoked elsewhere, the checkout's server is never trusted"
    );
}

#[cfg(unix)]
#[test]
fn absolute_non_executable_non_node_command_is_unavailable() {
    let root = TmpDir::new();
    let command = root.join("server");
    std::fs::create_dir_all(&root).expect("create temporary command dir");
    std::fs::write(&command, "not executable\n").expect("write temporary command");

    let Some(command_str) = command.to_str() else {
        panic!("temporary command path is not utf-8");
    };
    assert!(!command_resolves_to_executable(command_str));
    assert!(!is_command_available(command_str).expect("check command availability"));
}
