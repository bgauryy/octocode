use super::*;
use std::os::unix::fs::PermissionsExt;

/// A relative command with a separator resolves against the workspace
/// root when the process directory has none: the same entry works from
/// the CLI in the repository and from an MCP server started elsewhere.
#[test]
fn a_relative_command_resolves_against_the_base() {
    let root = crate::test_support::TmpDir::new();
    let bin = root.join("node_modules").join(".bin");
    std::fs::create_dir_all(&bin).expect("bin");
    let server = bin.join("fake-ls");
    std::fs::write(&server, "#!/bin/sh\n").expect("server");
    std::fs::set_permissions(&server, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let command = "./node_modules/.bin/fake-ls".to_owned();
    assert!(
        validate_lsp_server_path(command.clone(), None, &std::collections::HashMap::new()).is_err()
    );
    let resolved =
        validate_lsp_server_path(command, Some(&root), &std::collections::HashMap::new())
            .expect("rebased");
    assert_eq!(std::path::PathBuf::from(resolved), server);
}
