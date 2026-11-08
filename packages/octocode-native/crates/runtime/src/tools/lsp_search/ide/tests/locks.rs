//! What the rung trusts of the lock directory: a lock that is a symbolic
//! link, a directory that is one, or one others can write is refused (and
//! its token never sent); the token never shows in a formatted lock.
use super::{Bridge, Editor, TOKEN, result};
use serde_json::{Value, json};

#[test]
fn no_lock_directory_means_the_rung_does_not_apply() {
    let editor = Editor::new();
    assert!(!super::super::lock::may_apply(&editor.home));
    std::fs::create_dir(editor.home.join("ide")).expect("ide dir");
    assert!(!super::super::lock::may_apply(&editor.home));
}

/// A directory changed within its time's resolution keeps the same time, so
/// a listing read while the directory is unsettled is never kept.
#[test]
fn a_listing_of_an_unsettled_directory_is_read_again() {
    let editor = Editor::new();
    let dir = editor.home.join("ide");
    std::fs::create_dir(&dir).expect("ide dir");
    // Dated ahead, so the directory never settles, however fast the clock.
    let frozen = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
    let freeze = || {
        std::fs::File::open(&dir)
            .and_then(|handle| handle.set_modified(frozen))
            .expect("date the directory");
    };
    freeze();
    assert!(!super::super::lock::may_apply(&editor.home));
    std::fs::write(dir.join("1.json"), "{}").expect("lock file");
    freeze();
    assert!(super::super::lock::may_apply(&editor.home));
}

#[test]
fn a_lock_formatted_for_debugging_hides_its_token() {
    let lock: super::super::lock::Lock = serde_json::from_value(json!({
        "version": 1, "pid": 7, "ide": "Visual Studio Code", "port": 1,
        "token": TOKEN, "workspaceFolders": []
    }))
    .expect("lock");
    let shown = format!("{lock:?} {lock:#?}");
    assert!(!shown.contains(TOKEN), "{shown}");
    assert!(shown.contains("redacted"), "{shown}");
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    async fn outline(editor: &Editor) -> Value {
        editor.without_server();
        editor.ask(json!({"operation": "documentSymbols"})).await
    }

    fn assert_refused(row: &Value, bridge: &Bridge, reason: &str) {
        assert_eq!(row["lsp"]["fellBackFrom"]["level"], "ide", "{row}");
        let said = row["lsp"]["fellBackFrom"]["reason"]
            .as_str()
            .unwrap_or_default();
        assert!(said.contains(reason), "{said} lacks {reason}");
        assert!(!said.contains(TOKEN), "the token leaked: {said}");
        assert!(bridge.methods().is_empty(), "{:?}", bridge.methods());
    }

    /// A valid lock file for this process and workspace, at `path`.
    fn write_lock(editor: &Editor, path: &std::path::Path, port: u16) {
        std::fs::write(
            path,
            json!({
                "version": 1, "pid": std::process::id(), "ide": "Visual Studio Code",
                "port": port, "token": TOKEN, "workspaceFolders": [editor.root]
            })
            .to_string(),
        )
        .expect("lock");
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).expect("mode");
    }

    /// A lock that is a symbolic link to a well-formed, private lock is
    /// refused and never opened.
    #[tokio::test(flavor = "current_thread")]
    async fn a_symlinked_lock_is_refused() {
        let editor = Editor::with_file("notes.zzz");
        let bridge = Bridge::serve(Box::new(|_| result(json!([]))));
        let real = editor.home.join("elsewhere.json");
        write_lock(&editor, &real, bridge.port);
        let dir = editor.home.join("ide");
        std::fs::create_dir(&dir).expect("ide dir");
        super::super::private_directory(&dir);
        symlink(&real, dir.join(format!("{}.json", std::process::id()))).expect("link");
        let row = outline(&editor).await;
        assert_refused(&row, &bridge, "symbolic link");
    }

    /// A lock directory that is a symbolic link is refused, whatever it holds.
    #[tokio::test(flavor = "current_thread")]
    async fn a_symlinked_lock_directory_is_refused() {
        let editor = Editor::with_file("notes.zzz");
        let bridge = Bridge::serve(Box::new(|_| result(json!([]))));
        let real = editor.home.join("real-ide");
        std::fs::create_dir(&real).expect("real dir");
        super::super::private_directory(&real);
        write_lock(
            &editor,
            &real.join(format!("{}.json", std::process::id())),
            bridge.port,
        );
        symlink(&real, editor.home.join("ide")).expect("link");
        let row = outline(&editor).await;
        assert_refused(&row, &bridge, "lock directory is a symbolic link");
    }

    /// A lock directory others can write could have its locks replaced.
    #[tokio::test(flavor = "current_thread")]
    async fn a_world_writable_lock_directory_is_refused() {
        let editor = Editor::with_file("notes.zzz");
        let bridge = Bridge::serve(Box::new(|_| result(json!([]))));
        editor.open(&bridge);
        let dir = editor.home.join("ide");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o777)).expect("mode");
        let row = outline(&editor).await;
        assert_refused(&row, &bridge, "writable by its group or others");
    }

    /// The nearest ancestor comes from the one process-table read.
    #[test]
    fn the_ancestor_chain_starts_at_the_parent_process() {
        let parent = std::os::unix::process::parent_id();
        let chain = super::super::super::lock::Hints::of_process().ancestors;
        if parent > 1 {
            assert_eq!(chain.first(), Some(&parent), "{chain:?}");
        }
    }
}
