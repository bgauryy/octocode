//! Explicit builds: each publishes a fresh snapshot of the same graph, scoped to its workspace.
use super::*;

#[test]
fn builds_pages_and_freshness_are_workspace_scoped() {
    let dir = Fixture::new();
    dir.write(
        "app/a.ts",
        "export function first() { return 1; }\nexport function final() { return first(); }\n",
    );
    dir.write("other/a.ts", "export const other = 2;\n");
    let built = call(&dir.0, ToolId::GraphBuild, json!({"path":"app"}));
    // The snapshot lives in the workspace; its receipt names the scope relative to it.
    assert!(
        dir.0
            .join(".octocode/graph")
            .join(built["id"].as_str().expect("id"))
            .is_dir(),
        "{built}"
    );
    assert_eq!(built["root"], "app", "{built}");
    // An explicit rebuild of unchanged sources publishes a new snapshot with the
    // same graph content; the receipt carries no reuse flag.
    let rebuilt = call(&dir.0, ToolId::GraphBuild, json!({"path":"app"}));
    assert_ne!(built["id"], rebuilt["id"], "{rebuilt}");
    assert!(rebuilt.get("reused").is_none(), "{rebuilt}");
    for key in ["root", "counts", "calls", "linkage", "scan"] {
        assert!(!built[key].is_null(), "{key} missing: {built}");
        assert_eq!(built[key], rebuilt[key], "{key} differs between builds");
    }
    assert!(
        dir.0
            .join(".octocode/graph")
            .join(rebuilt["id"].as_str().expect("id"))
            .is_dir(),
        "{rebuilt}"
    );
    let stats = call(
        &dir.0,
        ToolId::GraphQuery,
        built["related"][0]["input"]["queries"][0].clone(),
    );
    assert_eq!(stats["graph"]["id"], built["id"]);
    let callers = call(
        &dir.0,
        ToolId::GraphQuery,
        json!({"operation":"callers", "path":"app", "target":"a.ts#first"}),
    );
    assert_eq!(
        callers["confidence"], "high",
        "the core contract supplies the default"
    );
    assert_eq!(callers["rows"][0]["id"], "a.ts#final");
    call(&dir.0, ToolId::GraphBuild, json!({"path":"other"}));
    let mut row = json!({"operation":"find", "path":"app", "target":"f", "limit":1});
    let mut keys = BTreeSet::new();
    loop {
        let page = call(&dir.0, ToolId::GraphQuery, row);
        // The latest snapshot of the scope serves it; another scope's build
        // and the older snapshot do not take its place.
        assert_eq!(page["graph"]["id"], rebuilt["id"]);
        assert_eq!(page["freshness"]["fresh"], true);
        for item in page["rows"].as_array().expect("results") {
            assert!(keys.insert(item.to_string()), "duplicate row");
        }
        match page.pointer("/next/nextPage/input/queries/0") {
            Some(next) => row = next.clone(),
            None => {
                assert_eq!(keys.len() as u64, page["pagination"]["totalItems"]);
                break;
            }
        }
    }
    assert!(keys.len() >= 2);
    dir.write("app/a.ts", "export function first() { return 3; }\n");
    let stale = call(
        &dir.0,
        ToolId::GraphQuery,
        json!({"operation":"stats", "path":"app", "graph":built["id"]}),
    );
    assert_eq!(stale["freshness"]["fresh"], false);
    let wrong = call(
        &dir.0,
        ToolId::GraphQuery,
        json!({"operation":"stats", "path":"other", "graph":built["id"]}),
    );
    assert_eq!(wrong["errorCode"], "invalidInput");
}

/// Simultaneous builds are serialized by the build lock, each extracts and
/// publishes its own snapshot, and retention applies to all of them: never
/// more than `keep` remain, each remaining one is complete and readable.
#[test]
fn simultaneous_builds_each_publish_a_complete_snapshot_under_retention() {
    const BUILDS: usize = 3;
    const KEEP: usize = 2;
    let dir = Fixture::new();
    dir.write("a.ts", "export const a = 1;");
    let outputs = std::thread::scope(|scope| {
        let tasks = (0..BUILDS)
            .map(|_| scope.spawn(|| call(&dir.0, ToolId::GraphBuild, json!({"keep": KEEP}))))
            .collect::<Vec<_>>();
        tasks
            .into_iter()
            .map(|task| task.join().expect("build"))
            .collect::<Vec<_>>()
    });
    let ids = outputs
        .iter()
        .map(|out| out["id"].as_str().expect("id").to_owned())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        ids.len(),
        BUILDS,
        "every build publishes its own id: {outputs:?}"
    );
    for out in &outputs {
        assert!(out.get("errorCode").is_none(), "{out}");
        assert!(out.get("reused").is_none(), "{out}");
        assert_eq!(out["counts"], outputs[0]["counts"]);
        assert_eq!(out["linkage"], outputs[0]["linkage"]);
    }
    let retained = list_snapshots(&dir.0.join(".octocode/graph"));
    assert_eq!(retained.len(), KEEP, "{retained:?}");
    for id in &retained {
        assert!(ids.contains(id), "{id} is not one of the builds: {ids:?}");
        let stats = call(
            &dir.0,
            ToolId::GraphQuery,
            json!({"operation":"stats", "graph": id}),
        );
        assert!(stats.get("errorCode").is_none(), "{stats}");
        assert_eq!(stats["graph"]["id"], json!(id), "{stats}");
        assert_eq!(stats["counts"], outputs[0]["counts"], "{stats}");
    }
}
