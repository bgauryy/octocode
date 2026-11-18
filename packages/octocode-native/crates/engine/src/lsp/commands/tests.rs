use super::*;

#[test]
fn rejected_shell_matches_common_wrappers_by_file_name() {
    for shell in ["sh", "/bin/bash", "powershell.exe", "pwsh"] {
        assert!(is_rejected_shell(shell), "{shell}");
    }
    assert!(!is_rejected_shell("rust-analyzer"));
}

#[test]
fn has_path_separator_detects_unix_and_windows_paths() {
    assert!(has_path_separator("bin/server"));
    assert!(has_path_separator("bin\\server"));
    assert!(!has_path_separator("server"));
}
