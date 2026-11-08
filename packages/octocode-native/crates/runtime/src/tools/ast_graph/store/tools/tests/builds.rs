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
    // A build of unchanged sources answers with the current snapshot; a
    // refresh publishes a new one with the same graph content. Neither
    // receipt carries a reuse flag.
    let again = call(&dir.0, ToolId::GraphBuild, json!({"path":"app"}));
    assert_eq!(built["id"], again["id"], "{again}");
    let rebuilt = call(
        &dir.0,
        ToolId::GraphBuild,
        json!({"path":"app","refresh":true}),
    );
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

/// A paused build is a partial row with its progress, whose
/// `next.expandScan` is the same request without `refresh`, which would
/// discard what it resumes.
#[test]
fn a_paused_build_resumes_through_expand_scan_without_refresh() {
    struct Spent(std::time::Instant);
    impl crate::execution::cancel::CancellationCheck for Spent {
        fn check(&self) -> Result<(), String> {
            Ok(())
        }
        fn deadline(&self) -> Option<std::time::Instant> {
            Some(self.0)
        }
    }
    let dir = Fixture::new();
    dir.write("app/a.ts", "export const a = () => 1;\n");
    let paths = workspace_policy(&dir.0);
    let (workspace, root, _) = scope("app", &paths, None).expect("scope");
    let query: GraphBuildQuery =
        serde_json::from_value(json!({"path": "app", "refresh": true})).expect("query");
    let options = IngestOptions {
        workspace: Some(workspace),
        ..IngestOptions::of_query(&query, root)
    };
    let mut paused = ingest(
        &options,
        &paths,
        &ContentSecurity::new(),
        &Spent(std::time::Instant::now()),
        None,
    );
    resume_paused(&mut paused, &query);
    let data = result(paused).data;
    crate::tools::test_support::assert_row_contract(ToolId::GraphBuild, &data);
    assert_eq!(data["isPartial"], true, "{data}");
    let row = &data["next"]["expandScan"]["input"]["queries"][0];
    assert_eq!(row["path"], "app", "{data}");
    assert!(row.get("refresh").is_none(), "{data}");
    // The resumed call builds the scope.
    let built = call(&dir.0, ToolId::GraphBuild, row.clone());
    assert!(built["id"].is_string(), "{built}");
}

/// A build whose source files all sit behind ignore rules or the default
/// excludes names why it holds no file, and leads to the same graphBuild
/// with the flag that reaches them; running that lead builds them.
#[test]
fn a_build_of_only_skipped_files_says_why_it_is_empty() {
    // In a git work tree a default-pruned name is pruned only where git
    // ignores it, so the default excludes alone show outside one.
    let git = Fixture::new();
    std::fs::create_dir_all(git.0.join(".git")).expect("git");
    git.write("scratch/.gitignore", "*\n");
    git.write("scratch/a.ts", "export function a() { return 1; }\n");
    git.write("plain/c.ts", "export const c = 1;\n");
    let loose = Fixture::new();
    loose.write("vendored/node_modules/x/b.ts", "export const b = 1;\n");
    loose.write("vendored/readme.md", "# not code\n");
    for (dir, path, flag, value) in [
        (&git, "scratch", "includeIgnored", json!(true)),
        (&loose, "vendored", "defaultExcludes", json!(false)),
    ] {
        let built = call(&dir.0, ToolId::GraphBuild, json!({"path": path}));
        assert_eq!(built["counts"]["nodes"], 0, "{built}");
        let hints = built["hints"].to_string();
        assert!(hints.contains("behind skipped paths"), "{path}: {built}");
        assert!(
            !hints.contains("has no includeIgnored"),
            "{path}: graphBuild has the flag now: {built}"
        );
        let lead = built["related"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|call| call["tool"] == "graphBuild")
            .unwrap_or_else(|| panic!("{path}: no lead: {built}"));
        let query = &lead["input"]["queries"][0];
        assert_eq!(query[flag], value, "{path}: {lead}");
        assert_eq!(query["path"], path, "{path}: {lead}");
        crate::contracts::validate_built_query(ToolId::GraphBuild.as_str(), query.clone())
            .unwrap_or_else(|error| panic!("{path}: the lead is not runnable: {error:?}"));
        let lifted = call(&dir.0, ToolId::GraphBuild, query.clone());
        assert!(
            lifted["counts"]["nodes"].as_u64().unwrap_or(0) > 0,
            "{path}: the lead builds the skipped files: {lifted}"
        );
    }
    // A scope with code says nothing about skipped paths.
    let plain = call(&git.0, ToolId::GraphBuild, json!({"path": "plain"}));
    assert!(plain.get("hints").is_none(), "{plain}");
}

/// A build that lost a race with an edit names the cause as transient and
/// leads to the same graphBuild, runnable as written; it is no tool failure.
#[test]
fn a_build_that_lost_a_race_with_an_edit_leads_to_the_same_build() {
    let query: GraphBuildQuery =
        serde_json::from_value(json!({"path": "app", "keep": 2})).expect("graphBuild query");
    let mut out =
        GraphOutput::changed_during_build("source \"a.ts\" changed during graph analysis");
    retry_after_change(&mut out, &query);
    let hints = out.value["hints"].to_string();
    assert!(hints.contains("run the same graphBuild again"), "{hints}");
    assert!(!hints.contains("tool failure") || hints.contains("not a tool failure"));
    let lead = &out.value["related"][0];
    assert_eq!(lead["tool"], "graphBuild", "{lead}");
    assert_eq!(lead["input"]["queries"][0]["path"], "app", "{lead}");
    assert_eq!(lead["input"]["queries"][0]["keep"], 2, "{lead}");
    // A build that succeeded, or failed otherwise, is left alone.
    let mut other = GraphOutput::error(ErrorCode::ExecutionFailed, "other");
    retry_after_change(&mut other, &query);
    assert!(other.value.get("related").is_none());
}
