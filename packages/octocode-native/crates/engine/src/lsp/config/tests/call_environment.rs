//! Discovery and start read the CALL's environment, never the process's: a
//! host that serves many callers from one process (the warm server) finds
//! each caller's language servers on that caller's `PATH`.
use super::*;
use crate::lsp::config::{LspDiscoveryOptions, default_server_for_file, server_command_resolves};
use std::collections::BTreeMap;

#[cfg(unix)]
fn executable(dir: &std::path::Path, name: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(dir).expect("dir");
    let path = dir.join(name);
    std::fs::write(&path, "#!/bin/sh\nexit 0\n").expect("script");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("mode");
    path
}

/// A server on the caller's `PATH` only is found for that caller, and its
/// spawn environment is that caller's `PATH`; the same discovery under a
/// `PATH` without it (as the process's) finds none. The call is
/// `path_a`'s in the first half, `path_b`'s in the second.
#[cfg(unix)]
#[test]
fn a_server_is_found_on_the_callers_path_not_the_processs() {
    let tmp = TmpDir::new();
    let root = tmp.join("ws");
    std::fs::create_dir_all(&root).expect("root");
    std::fs::write(root.join("a.py"), "x = 1\n").expect("source");
    let (a, b) = (tmp.join("bin-a"), tmp.join("bin-b"));
    std::fs::create_dir_all(&a).expect("a");
    let _server = executable(&b, "pyright-langserver");
    let options = |path: &std::path::Path| LspDiscoveryOptions {
        env: BTreeMap::from([("PATH".to_owned(), path.to_string_lossy().into_owned())]),
        ..LspDiscoveryOptions::default()
    };
    let file = root.join("a.py").to_string_lossy().into_owned();
    let workspace = root.to_string_lossy().into_owned();

    let found = default_server_for_file(&file, &workspace, &options(&b)).expect("found on B");
    assert!(found.command.ends_with("pyright-langserver"), "{found:?}");
    assert_eq!(
        found.env.as_ref().and_then(|env| env.get("PATH")),
        Some(&b.to_string_lossy().into_owned()),
        "the server is started under the caller's PATH"
    );
    assert!(server_command_resolves(&found));

    let elsewhere = default_server_for_file(&file, &workspace, &options(&a));
    assert!(
        elsewhere.is_none_or(|config| !config.command.starts_with(&*b.to_string_lossy())),
        "B's server is not found for a caller whose PATH is A"
    );
}
