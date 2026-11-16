//! A server route is resolved once per state of what it reads: every lead and
//! `lspSearch` request asks, and `which` with the config reads is most of a
//! warm request's discovery.
use super::write_lsp_config;
use crate::lsp::config::catalog::resolve_route;
use crate::lsp::config::invocation::rustup::{RustupView, view_env, with_view};
use crate::lsp::config::route::kept_route;
use crate::lsp::config::{LspDiscoveryOptions, default_server_for_directory};
use crate::lsp::module_path::install_anchor;
use crate::stamp::settle_written_so_far;
use crate::test_support::TmpDir;
use std::path::PathBuf;

struct Machine {
    root: TmpDir,
    workspace: PathBuf,
    options: LspDiscoveryOptions,
    view: RustupView,
}

/// A workspace, an octocode home and an empty `PATH` directory, all settled.
fn machine() -> Machine {
    let root = TmpDir::new();
    let workspace = root.join("project");
    std::fs::create_dir_all(&workspace).expect("workspace");
    std::fs::create_dir_all(root.join("bin")).expect("bin");
    let view = RustupView {
        path_var: Some(root.join("bin").into_os_string()),
        home: Some(root.join("rustup-home")),
        ..RustupView::default()
    };
    let options = LspDiscoveryOptions {
        octocode_home: Some(root.join(".octocode")),
        env: view_env(&view),
        ..LspDiscoveryOptions::default()
    };
    settle_written_so_far();
    Machine {
        root,
        workspace,
        options,
        view,
    }
}

impl Machine {
    /// The command discovery gives a `file` in the workspace, and how many
    /// routes it resolved rather than served kept: `default_server_for_file`
    /// with its resolver counted, the one seam (`kept_route` takes it).
    fn route(&self, file: &str) -> (Option<String>, usize) {
        let extension = format!(".{}", file.rsplit('.').next().expect("extension"));
        let root = self.workspace.to_string_lossy().into_owned();
        let anchor = install_anchor();
        let resolved = std::cell::Cell::new(0);
        let route = with_view(self.view.clone(), || {
            kept_route(
                &extension,
                &root,
                &self.options,
                anchor.as_deref(),
                true,
                || {
                    resolved.set(resolved.get() + 1);
                    resolve_route(&extension, &root, &self.options, anchor.as_deref(), true)
                },
            )
        });
        let command = route.map(|route| route.finished(&self.options).command);
        (command, resolved.get())
    }

    fn config(&self, command: &str) {
        write_lsp_config(
            &self.root,
            &format!(
                r#"{{"languageServers":{{".go":{{"command":"{command}","languageId":"go"}}}}}}"#
            ),
        );
        settle_written_so_far();
    }
}

/// A kept route is served without resolving it again (no `which`, no read).
#[test]
fn a_kept_route_is_served_without_resolving() {
    let machine = machine();
    let first = machine.route("main.go");
    assert_eq!(first.1, 1);
    for _ in 0..3 {
        assert_eq!(machine.route("main.go"), (first.0.clone(), 0));
    }
    // Another file of the same language shares the route.
    assert_eq!(machine.route("other.go"), (first.0, 0));
}

/// A changed `PATH` value, a program installed into a `PATH` directory, and
/// an `lsp-servers.json` written, edited or removed each resolve again.
#[test]
fn what_resolution_reads_resolves_again() {
    let mut machine = machine();
    machine.route("main.go");
    assert_eq!(machine.route("main.go").1, 0);

    machine.view.path_var = Some(machine.root.join("other-bin").into_os_string());
    assert_eq!(machine.route("main.go").1, 1, "PATH value");
    assert_eq!(machine.route("main.go").1, 0);

    std::fs::create_dir_all(machine.root.join("other-bin")).expect("dir");
    settle_written_so_far();
    assert_eq!(machine.route("main.go").1, 1, "PATH directory created");
    machine.root.write("other-bin/gopls", "#!/bin/sh\n");
    settle_written_so_far();
    assert_eq!(machine.route("main.go").1, 1, "program installed on PATH");
    assert_eq!(machine.route("main.go").1, 0);

    machine.config("first-server");
    assert_eq!(
        machine.route("main.go"),
        (Some("first-server".to_owned()), 1),
        "config written"
    );
    assert_eq!(machine.route("main.go").1, 0);
    machine.config("second-server");
    assert_eq!(
        machine.route("main.go"),
        (Some("second-server".to_owned()), 1),
        "config edited"
    );
    std::fs::remove_file(machine.root.join(".octocode/lsp-servers.json")).expect("removed");
    settle_written_so_far();
    let (command, resolved) = machine.route("main.go");
    assert_eq!(resolved, 1, "config removed");
    assert_ne!(command.as_deref(), Some("second-server"));
}

/// A program installed in a `PATH` directory after the one `which` found the
/// server in cannot take over, so the route stays kept; one installed before
/// it resolves again.
#[test]
fn only_path_directories_before_the_found_server_count() {
    let mut machine = machine();
    for dir in ["early", "found", "late"] {
        std::fs::create_dir_all(machine.root.join(dir)).expect("dir");
    }
    machine.root.write("found/gopls", "#!/bin/sh\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            machine.root.join("found/gopls"),
            std::fs::Permissions::from_mode(0o755),
        )
        .expect("chmod");
    }
    let joined = std::env::join_paths(["early", "found", "late"].map(|dir| machine.root.join(dir)))
        .expect("PATH");
    machine.view.path_var = Some(joined);
    settle_written_so_far();
    machine.route("main.go");
    assert_eq!(machine.route("main.go").1, 0);
    machine.root.write("late/gopls", "#!/bin/sh\n");
    settle_written_so_far();
    assert_eq!(machine.route("main.go").1, 0, "after the found directory");
    machine.root.write("early/gopls", "#!/bin/sh\n");
    settle_written_so_far();
    assert_eq!(machine.route("main.go").1, 1, "before the found directory");
}

/// A file that is on `PATH` but not executable is no server; `chmod +x` makes
/// it one without changing its directory, so the file is stamped too.
#[cfg(unix)]
#[test]
fn making_a_path_program_executable_resolves_again() {
    use std::os::unix::fs::PermissionsExt;
    let machine = machine();
    let program = machine.root.write("bin/gopls", "#!/bin/sh\n");
    settle_written_so_far();
    assert_eq!(machine.route("main.go").1, 1);
    assert_eq!(machine.route("main.go").1, 0, "the miss is kept");
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    settle_written_so_far();
    assert_eq!(machine.route("main.go").1, 1, "chmod +x");
    assert_eq!(machine.route("main.go").1, 0);
}

/// A user entry is stamped by the server it names, not by its extension: a
/// `.vue` entry for the TypeScript server resolves again when a
/// `node_modules` appears where that server is looked for.
#[test]
fn a_user_entry_for_the_typescript_server_follows_node_modules() {
    let machine = machine();
    write_lsp_config(
        &machine.root,
        r#"{"languageServers":{".vue":{"command":"typescript-language-server","languageId":"vue"}}}"#,
    );
    settle_written_so_far();
    assert_eq!(machine.route("App.vue").1, 1);
    assert_eq!(machine.route("App.vue").1, 0);
    std::fs::create_dir_all(machine.workspace.join("node_modules")).expect("node_modules");
    settle_written_so_far();
    assert_eq!(machine.route("App.vue").1, 1, "node_modules created");
    assert_eq!(machine.route("App.vue").1, 0);
}

/// "No server" is kept too, until `PATH` changes.
#[test]
fn no_route_is_kept_until_path_changes() {
    let mut machine = machine();
    assert_eq!(machine.route("main.rb"), (None, 1));
    assert_eq!(machine.route("main.rb"), (None, 0));
    machine.view.path_var = Some(machine.root.join("other-bin").into_os_string());
    assert_eq!(machine.route("main.rb"), (None, 1));
    assert_eq!(machine.route("main.rb"), (None, 0));
}

/// A toolchain file that would select another rust-analyzer resolves the
/// Rust route again.
#[test]
fn a_toolchain_file_resolves_the_rust_route_again() {
    let machine = machine();
    machine.route("lib.rs");
    assert_eq!(machine.route("lib.rs").1, 0);
    machine
        .root
        .write("project/rust-toolchain.toml", "[toolchain]\n");
    settle_written_so_far();
    assert_eq!(machine.route("lib.rs").1, 1);
}

/// A new project marker at a root-only query's workspace picks the language,
/// and with it the route.
#[test]
fn a_new_marker_changes_a_root_route() {
    let machine = machine();
    let root_route = || {
        with_view(machine.view.clone(), || {
            let workspace = machine.workspace.to_string_lossy();
            default_server_for_directory(&workspace, &workspace, &machine.options)
        })
        .and_then(|config| config.language_id)
    };
    machine.root.write("project/go.mod", "module x\n");
    settle_written_so_far();
    assert_eq!(root_route().as_deref(), Some("go"));
    assert_eq!(root_route().as_deref(), Some("go"));
    machine
        .root
        .write("project/Cargo.toml", "[workspace]\nmembers = []\n");
    settle_written_so_far();
    assert_eq!(root_route().as_deref(), Some("rust"));
}
