use super::*;
#[cfg(unix)]
use std::fs;

// ── validate_lsp_server_path ──────────────────────────────────────────────

/// A missing typescript-language-server names how octocode runs it, so a
/// 644 `cli.mjs` is not mistaken for the cause.
#[test]
fn a_missing_typescript_language_server_explains_the_lookup() {
    let note = lookup_note("typescript-language-server");
    assert!(note.contains("lib/cli.mjs"), "{note}");
    assert!(note.contains("under node"), "{note}");
    assert!(note.contains("644"), "{note}");
}

/// A missing built-in server names the command that installs it; a
/// managed server installs through octocode. Only a command octocode
/// does not ship a route for gets no note.
#[test]
fn a_missing_built_in_server_names_its_install_command() {
    let gopls = lookup_note("gopls");
    assert!(
        gopls.contains("go install golang.org/x/tools/gopls@latest"),
        "{gopls}"
    );
    for (command, install) in [
        ("pylsp", "pip install python-lsp-server"),
        ("csharp-ls", "dotnet tool install --global csharp-ls"),
        ("metals", "cs install metals"),
        ("jdtls", "jdtls"),
        ("clangd", "octocode lsp-server install clangd"),
        ("rust-analyzer", "octocode lsp-server install rust-analyzer"),
    ] {
        let note = lookup_note(command);
        assert!(note.contains(install), "{command}: {note}");
    }
    assert_eq!(lookup_note("my-own-language-server"), "");
}

#[test]
fn validate_lsp_server_path_rejects_empty_string() {
    assert!(
        validate_lsp_server_path(String::new(), None, &std::collections::HashMap::new()).is_err()
    );
    assert!(
        validate_lsp_server_path("   ".to_owned(), None, &std::collections::HashMap::new())
            .is_err()
    );
}

#[test]
fn validate_lsp_server_path_rejects_shell_wrappers() {
    for shell in ["sh", "bash", "zsh", "fish", "cmd", "powershell", "pwsh"] {
        let result =
            validate_lsp_server_path(shell.to_owned(), None, &std::collections::HashMap::new());
        assert!(
            result.is_err(),
            "shell '{shell}' must be rejected but was accepted"
        );
    }
}

#[test]
fn validate_lsp_server_path_rejects_nonexistent_absolute_path() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("nonexistent_server");
    let result = validate_lsp_server_path(
        path.to_string_lossy().into_owned(),
        None,
        &std::collections::HashMap::new(),
    );
    assert!(result.is_err());
}

#[cfg(unix)]
#[test]
fn validate_lsp_server_path_rejects_non_executable_file() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("nonexec");
    fs::write(&path, b"#!/bin/sh").expect("write");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("chmod");
    let result = validate_lsp_server_path(
        path.to_string_lossy().into_owned(),
        None,
        &std::collections::HashMap::new(),
    );
    assert!(result.is_err());
    assert!(
        result
            .expect_err("non-executable file must be rejected")
            .reason
            .contains("not executable")
    );
}

#[cfg(unix)]
#[test]
fn validate_lsp_server_path_accepts_executable_absolute_path() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("server_exec");
    fs::write(&path, b"#!/bin/sh\necho ok\n").expect("write");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");
    let result = validate_lsp_server_path(
        path.to_string_lossy().into_owned(),
        None,
        &std::collections::HashMap::new(),
    );
    // The command that runs is the given absolute path itself.
    let accepted = result.expect("an executable file is accepted");
    assert!(std::path::Path::new(&accepted).is_absolute(), "{accepted}");
    assert!(
        accepted.ends_with(&*path.file_name().expect("name").to_string_lossy()),
        "{accepted}"
    );
}

#[cfg(unix)]
#[test]
fn validate_lsp_server_path_preserves_a_symlinked_proxy_binary_name() {
    // A rustup-style toolchain proxy is a symlink whose target dispatches
    // on argv[0]'s basename (e.g. `rust-analyzer` -> `rustup`, which then
    // decides to run rust-analyzer only because it was invoked *as*
    // `rust-analyzer`). Canonicalizing here would resolve the symlink to
    // `.../rustup` and silently break that dispatch — the validated path
    // must keep the original (symlink) filename, not the resolved target.
    use std::os::unix::fs::{PermissionsExt, symlink};
    let dir = tempfile::tempdir().expect("dir");
    let target = dir.path().join("proxy_target");
    fs::write(&target, b"#!/bin/sh\necho ok\n").expect("write target");
    fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).expect("chmod");
    let link = dir.path().join("rust-analyzer");
    symlink(&target, &link).expect("symlink");

    let result = validate_lsp_server_path(
        link.to_string_lossy().into_owned(),
        None,
        &std::collections::HashMap::new(),
    );

    let resolved = result.expect("symlinked executable must be accepted");
    assert_eq!(
        Path::new(&resolved).file_name(),
        link.file_name(),
        "expected the symlink's own name to be preserved, got: {resolved}"
    );
}
