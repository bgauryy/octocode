//! `local.graphHome` moves graph snapshots out of the workspace: a build
//! writes nothing under it, a query finds the snapshot in the configured
//! home, and a directory outside the workspace (a fetched checkout) builds.
use super::*;

/// Every path under `root`, relative and sorted.
fn tree(root: &Path) -> Vec<String> {
    let mut paths = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            paths.push(
                path.strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .into_owned(),
            );
            if path.is_dir() {
                pending.push(path);
            }
        }
    }
    paths.sort();
    paths
}

fn snapshot_ids(home: &Path) -> Vec<String> {
    tree(home)
        .into_iter()
        .filter(|path| path.ends_with("manifest.json"))
        .collect()
}

#[tokio::test]
async fn graph_home_keeps_the_workspace_untouched() {
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    std::fs::write(cwd.path().join("a.ts"), "export const a = 1;\n").expect("source");
    let before = tree(cwd.path());
    let graphs = home.path().join("graphs");
    let rc = json!({"local":{"beta":true,"graphHome":graphs}});
    let tools = runtime(home.path(), cwd.path(), &rc);

    let built = call(&tools, "graphBuild", json!({"path":"."})).await;
    assert!(data(&built).get("errorCode").is_none(), "{}", data(&built));
    assert_eq!(tree(cwd.path()), before, "nothing written in the workspace");
    assert_eq!(snapshot_ids(&graphs).len(), 1, "{:?}", tree(&graphs));

    let stats = call(
        &tools,
        "graphQuery",
        json!({"operation":"stats","path":"."}),
    )
    .await;
    assert!(data(&stats).get("errorCode").is_none(), "{}", data(&stats));
    assert_eq!(
        data(&stats)["graph"]["id"],
        data(&built)["id"],
        "{}",
        data(&stats)
    );
    assert_eq!(
        tree(cwd.path()),
        before,
        "a query writes nothing there either"
    );

    // Unset, the snapshot goes to the workspace as before.
    let tools = runtime(home.path(), cwd.path(), &json!({"local":{"beta":true}}));
    let built = call(&tools, "graphBuild", json!({"path":"."})).await;
    assert!(data(&built).get("errorCode").is_none(), "{}", data(&built));
    assert_eq!(snapshot_ids(&cwd.path().join(".octocode/graph")).len(), 1);
}

/// GB-4: a materialized checkout lies outside the workspace. Without the
/// setting the build is refused and names the setting as its route; with
/// it, the checkout builds and its snapshot is queryable.
#[tokio::test]
async fn graph_home_builds_a_checkout_outside_the_workspace() {
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let checkout =
        crate::tools::gh_structure::materialize::materialize_root(home.path()).join("o/r/sha");
    std::fs::create_dir_all(&checkout).expect("checkout");
    std::fs::write(checkout.join("b.ts"), "export const b = 2;\n").expect("source");
    let path = checkout.to_string_lossy().into_owned();

    let tools = runtime(home.path(), cwd.path(), &json!({"local":{"beta":true}}));
    let refused = call(&tools, "graphBuild", json!({"path":path})).await;
    assert_eq!(
        data(&refused)["errorCode"],
        "outsideAllowedRoots",
        "{}",
        data(&refused)
    );
    assert!(
        data(&refused).to_string().contains("local.graphHome"),
        "{}",
        data(&refused)
    );

    let graphs = home.path().join("graphs");
    let rc = json!({"local":{"beta":true,"graphHome":graphs}});
    let tools = runtime(home.path(), cwd.path(), &rc);
    let built = call(&tools, "graphBuild", json!({"path":path})).await;
    assert!(data(&built).get("errorCode").is_none(), "{}", data(&built));
    let stats = call(
        &tools,
        "graphQuery",
        json!({"operation":"stats","path":path}),
    )
    .await;
    assert!(data(&stats).get("errorCode").is_none(), "{}", data(&stats));
    assert!(!cwd.path().join(".octocode").exists());
    assert!(!checkout.join(".octocode").exists(), "nor in the checkout");
}

/// A configured home inside the scanned path would be scanned by its own
/// build: refused, and nothing is written.
#[tokio::test]
async fn graph_home_inside_the_scanned_path_is_refused() {
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    std::fs::write(cwd.path().join("a.ts"), "export const a = 1;\n").expect("source");
    let graphs = cwd.path().join("graphs");
    let rc = json!({"local":{"beta":true,"graphHome":graphs}});
    let tools = runtime(home.path(), cwd.path(), &rc);
    let refused = call(&tools, "graphBuild", json!({"path":"."})).await;
    assert_eq!(
        data(&refused)["errorCode"],
        "invalidInput",
        "{}",
        data(&refused)
    );
    assert!(!graphs.exists());
    assert!(!cwd.path().join(".octocode").exists());
}

/// RV-GRAPH2 F1: the configured directory is the user's. A build writes only
/// inside its own subdirectory there and never removes what it did not
/// create, however old: a user's `.tmp-*` directory and files survive.
#[tokio::test]
async fn graph_home_keeps_what_the_user_has_there() {
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    std::fs::write(cwd.path().join("a.ts"), "export const a = 1;\n").expect("source");
    let graphs = home.path().join("userdir");
    let user_temp = graphs.join(".tmp-mywork");
    std::fs::create_dir_all(&user_temp).expect("user temp");
    std::fs::write(user_temp.join("file"), "mine").expect("user data");
    std::fs::create_dir_all(graphs.join("notes")).expect("notes");
    std::fs::write(graphs.join("notes/diary.txt"), "mine").expect("diary");
    let aged = std::time::SystemTime::now() - std::time::Duration::from_secs(2 * 3600);
    std::fs::File::open(&user_temp)
        .expect("open")
        .set_times(std::fs::FileTimes::new().set_modified(aged))
        .expect("age");
    let rc = json!({"local":{"beta":true,"graphHome":graphs}});
    let tools = runtime(home.path(), cwd.path(), &rc);

    for _ in 0..2 {
        let built = call(&tools, "graphBuild", json!({"path":"."})).await;
        assert!(data(&built).get("errorCode").is_none(), "{}", data(&built));
    }
    assert!(user_temp.join("file").exists(), "{:?}", tree(&graphs));
    assert!(graphs.join("notes/diary.txt").exists());
    let top: Vec<_> = std::fs::read_dir(&graphs)
        .expect("graphs")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name != ".tmp-mywork" && name != "notes")
        .collect();
    assert_eq!(top.len(), 1, "one Octocode-owned entry: {top:?}");
    assert_eq!(
        snapshot_ids(&graphs.join(&top[0])).len(),
        1,
        "{:?}",
        tree(&graphs)
    );
}

/// RV-GRAPH2 F3: the snapshot home is where graph tools write, not a grant
/// to read: a file under a configured home outside the workspace stays
/// outside the allowed roots for local tools, while graphBuild and
/// graphQuery work.
#[tokio::test]
async fn graph_home_is_no_read_root_for_local_tools() {
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    std::fs::write(cwd.path().join("a.ts"), "export const a = 1;\n").expect("source");
    let graphs = home.path().join("userdir");
    std::fs::create_dir_all(graphs.join("notes")).expect("notes");
    let diary = graphs.join("notes/diary.txt");
    std::fs::write(&diary, "private").expect("diary");
    let rc = json!({"local":{"beta":true,"graphHome":graphs}});
    let tools = runtime(home.path(), cwd.path(), &rc);

    let built = call(&tools, "graphBuild", json!({"path":"."})).await;
    assert!(data(&built).get("errorCode").is_none(), "{}", data(&built));
    let stats = call(
        &tools,
        "graphQuery",
        json!({"operation":"stats","path":"."}),
    )
    .await;
    assert!(data(&stats).get("errorCode").is_none(), "{}", data(&stats));
    let manifest = snapshot_ids(&graphs).pop().expect("a snapshot manifest");
    let manifest = graphs.join(manifest).to_string_lossy().into_owned();
    for path in [diary.to_string_lossy().into_owned(), manifest] {
        let fetched = call(&tools, "localFetch", json!({"path":path})).await;
        assert_eq!(
            data(&fetched)["errorCode"],
            "outsideAllowedRoots",
            "{path}: {}",
            data(&fetched)
        );
    }
}
