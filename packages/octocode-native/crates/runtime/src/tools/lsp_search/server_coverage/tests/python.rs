//! Python rows: dynamic dispatch and unresolved imports.
use super::*;

#[test]
fn python_incoming_rows_name_dynamic_dispatch_and_offer_text_search() {
    let mut row = json!({"operation":"callers","matches":[{"path":"a.py"}]});
    let q = query(
        json!({"path":"/w/a.py","operation":"callers","symbolName":"get_or_create","line":3}),
    );
    let scope = Scope::new("/w".into(), vec!["*.py".into()]);
    annotate(
        &mut row,
        &q,
        Some("python"),
        Server {
            anchor: "/w/a.py",
            root: "/w",
        },
        &scope,
    );
    // The reason is stated once, in partialReasons.
    assert_eq!(row["partialReasons"], json!([DYNAMIC_REASON]), "{row}");
    assert!(row["coverage"].get("reason").is_none(), "{row}");
    assert_eq!(row["isPartial"], true, "{row}");
    // The inference limit reads the same on every page: a notice.
    assert_eq!(row["notices"], json!([PYTHON_WARNING]), "{row}");
    assert!(
        !PYTHON_WARNING.contains("hints.findText"),
        "{PYTHON_WARNING}"
    );
    assert!(row.get("warnings").is_none(), "{row}");
    assert_eq!(
        crate::tools::test_support::checks::related_call(&row, "localSearch", |_| true)["tool"],
        "localSearch",
        "{row}"
    );
    // Outgoing questions are not affected.
    let mut definition = json!({"operation":"definition","matches":[]});
    let q = query(json!({"path":"/w/a.py","operation":"definition","symbolName":"x","line":3}));
    annotate(
        &mut definition,
        &q,
        Some("python"),
        Server {
            anchor: "/w/a.py",
            root: "/w",
        },
        &scope,
    );
    assert!(definition.get("isPartial").is_none(), "{definition}");
}

/// A Python file the answer misses that imports the name is an import
/// the server did not resolve to this declaration (`importUnresolved`),
/// not a dynamic call; a file that only calls it through a receiver is
/// the dynamic-dispatch miss. Each reason names its own files' cause.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn python_misses_name_unresolved_imports_apart_from_dynamic_dispatch() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("root");
    std::fs::write(
        root.join("accounting.py"),
        "def complete_sum(x):\n    return x\n",
    )
    .expect("anchor");
    std::fs::write(
        root.join("summary.py"),
        "from accounting import complete_sum\n\nprint(complete_sum(1))\n",
    )
    .expect("importer");
    std::fs::write(
        root.join("dynamic.py"),
        "def run(obj):\n    return obj.complete_sum(1)\n",
    )
    .expect("dynamic");
    async fn row_for(root: &std::path::Path, missed: &[&str]) -> Value {
        let anchor = root.join("accounting.py").to_string_lossy().into_owned();
        let scope = Scope::new(root.to_string_lossy().into_owned(), vec!["*.py".into()]);
        let policy = crate::tools::test_support::workspace_policy(root);
        let cancel = crate::execution::cancel::NeverCancel;
        scope
            .text_files("complete_sum", &policy, &cancel)
            .await
            .expect("scan");
        scope.answer([anchor.as_str()]);
        for file in ["summary.py", "dynamic.py"] {
            if !missed.contains(&file) {
                scope.answer([root.join(file).to_string_lossy()]);
            }
        }
        let mut row = json!({"operation":"references","matches":[{"path":"accounting.py"}]});
        let q = query(
            json!({"path":anchor,"operation":"references","symbolName":"complete_sum","line":1}),
        );
        annotate(
            &mut row,
            &q,
            Some("python"),
            Server {
                anchor: &anchor,
                root: &root.to_string_lossy(),
            },
            &scope,
        );
        row
    }
    let imported = row_for(&root, &["summary.py"]).await;
    assert_eq!(
        imported["partialReasons"],
        json!([UNRESOLVED_IMPORT_REASON]),
        "{imported}"
    );
    assert_eq!(
        imported["notices"],
        json!([UNRESOLVED_IMPORT_NOTE]),
        "{imported}"
    );
    let both = row_for(&root, &["summary.py", "dynamic.py"]).await;
    assert_eq!(
        both["partialReasons"],
        json!([UNRESOLVED_IMPORT_REASON, DYNAMIC_REASON]),
        "{both}"
    );
    let dynamic = row_for(&root, &["dynamic.py"]).await;
    assert_eq!(
        dynamic["partialReasons"],
        json!([DYNAMIC_REASON]),
        "{dynamic}"
    );
    assert_eq!(dynamic["notices"], json!([PYTHON_WARNING]), "{dynamic}");
}
