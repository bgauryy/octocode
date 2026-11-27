//! `install --rollback` failures under `--json` answer with the JSON error
//! envelope and exit by the error's class; a failed `lsp-server install` item
//! states its error code and class the same way.
use super::*;

fn rollback(workspace: &Workspace, backup: &std::path::Path) -> (Value, i32) {
    let output = workspace
        .cli()
        .args(["install", "--json", "--rollback"])
        .arg(backup)
        .output()
        .expect("run install");
    let code = output.status.code().expect("exit code");
    (json_output(&output), code)
}

#[test]
fn rollback_failures_are_json_errors_with_their_class_exit_code() {
    let workspace = Workspace::new();
    let dir = workspace.home.join("rollback");
    std::fs::create_dir_all(&dir).expect("scratch directory");

    let (missing, exit) = rollback(&workspace, &dir.join("mcp.json.bak"));
    assert_eq!(missing["errorCode"], "notFound", "{missing}");
    assert_eq!(missing["errorClass"], "correctInput", "{missing}");
    assert_eq!(exit, 2);

    let other = dir.join("mcp.txt");
    std::fs::write(&other, "{}").expect("not a backup");
    let (refused, exit) = rollback(&workspace, &other);
    assert_eq!(refused["errorCode"], "invalidInput", "{refused}");
    assert_eq!(refused["errorClass"], "correctInput", "{refused}");
    assert_eq!(exit, 2);

    // A directory named like a backup exists but cannot be read as one.
    let unreadable = dir.join("dir.json.bak");
    std::fs::create_dir(&unreadable).expect("directory named like a backup");
    let (failed, exit) = rollback(&workspace, &unreadable);
    assert_eq!(failed["errorCode"], "executionFailed", "{failed}");
    assert_eq!(failed["errorClass"], "unservable", "{failed}");
    assert_eq!(exit, 5);
}

/// A name that is no managed language server fails its `lsp-server install`
/// item with the caller's to correct: `errorCode` and `errorClass` beside the
/// item's own error.
#[test]
fn an_unknown_lsp_server_install_item_states_its_error_code_and_class() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["lsp-server", "install", "nosuchlang", "--json"])
        .output()
        .expect("run lsp-server install");
    let answer = json_output(&output);
    let item = &answer["install"][0];
    assert_eq!(item["name"], "nosuchlang", "{answer}");
    assert_eq!(item["ok"], false, "{answer}");
    assert_eq!(item["errorCode"], "notFound", "{answer}");
    assert_eq!(item["errorClass"], "correctInput", "{answer}");
}

fn install_exit(workspace: &Workspace, names: &[&str]) -> i32 {
    workspace
        .cli()
        .env("OCTOCODE_LSP_AUTO_INSTALL", "off")
        .args(["lsp-server", "install", "--json"])
        .args(names)
        .output()
        .expect("run lsp-server install")
        .status
        .code()
        .expect("exit code")
}

/// `lsp-server install` exits by the strictest class of its failed items: an
/// unknown name is the caller's to correct (2), also in a batch beside an
/// item the setting refuses (4), which alone exits 4.
#[test]
fn lsp_server_install_exits_by_the_strictest_failed_item_class() {
    let workspace = Workspace::new();
    assert_eq!(install_exit(&workspace, &["nosuchlang"]), 2);
    assert_eq!(install_exit(&workspace, &["rust-analyzer"]), 4);
    assert_eq!(
        install_exit(&workspace, &["rust-analyzer", "nosuchlang"]),
        2
    );
}
