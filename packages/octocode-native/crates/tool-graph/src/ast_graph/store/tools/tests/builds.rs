//! Explicit builds: each publishes a fresh snapshot of the same graph, scoped to its workspace.
use super::*;
use octocode_test_support::json_fixture;

#[test]
fn builds_pages_and_freshness_are_workspace_scoped() {
    let dir = Fixture::new();
    dir.write(
        "app/a.ts",
        "export function first() { return 1; }\nexport function final() { return first(); }\n",
    );
    dir.write("other/a.ts", "export const other = 2;\n");
    let built = call(
        &dir.0,
        ToolId::GraphBuild,
        json_fixture(r#"{"path": "app"}"#),
    );
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
    let again = call(
        &dir.0,
        ToolId::GraphBuild,
        json_fixture(r#"{"path": "app"}"#),
    );
    assert_eq!(built["id"], again["id"], "{again}");
    let rebuilt = call(
        &dir.0,
        ToolId::GraphBuild,
        json_fixture(r#"{"path": "app", "refresh": true}"#),
    );
    assert_ne!(built["id"], rebuilt["id"], "{rebuilt}");
    assert!(rebuilt.get("reused").is_none(), "{rebuilt}");
    for key in ["root", "counts", "linkage", "scan"] {
        assert!(!built[key].is_null(), "{key} missing: {built}");
        assert_eq!(built[key], rebuilt[key], "{key} differs between builds");
    }
    // The call-resolution breakdown is the snapshot's detail: `stats`, the
    // receipt's related call, states it.
    assert!(built.get("calls").is_none(), "{built}");
    let stats = &built["related"][0]["input"]["queries"][0];
    assert_eq!(stats["operation"], "stats", "{built}");
    let stated = call(&dir.0, ToolId::GraphQuery, stats.clone());
    assert!(stated["calls"]["sites"].is_u64(), "{stated}");
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
        json_fixture(
            r#"{
                "operation": "callers",
                "path": "app",
                "target": "app/a.ts#first"
            }"#,
        ),
    );
    assert_eq!(
        callers["confidence"], "high",
        "the core contract supplies the default"
    );
    assert_eq!(callers["rows"][0]["id"], "app/a.ts#final");
    call(
        &dir.0,
        ToolId::GraphBuild,
        json_fixture(r#"{"path": "other"}"#),
    );
    let mut row = json_fixture(
        r#"{
            "operation": "find",
            "path": "app",
            "target": "f",
            "limit": 1
        }"#,
    );
    let mut keys = BTreeSet::new();
    loop {
        let page = call(&dir.0, ToolId::GraphQuery, row);
        // The latest snapshot of the scope serves it; another scope's build
        // and the older snapshot do not take its place.
        assert_eq!(page["graph"]["id"], rebuilt["id"]);
        assert!(page.get("freshness").is_none(), "{page}");
        for item in page["rows"].as_array().expect("results") {
            assert!(keys.insert(item.to_string()), "duplicate row");
        }
        match page.pointer("/next/nextPage/input/queries/0") {
            Some(next) => row = next.clone(),
            None => {
                assert_eq!(keys.len() as u64, page["pagination"]["total"]);
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

/// Simultaneous `refresh` builds are serialized by the build lock, each
/// extracts and publishes its own snapshot, and retention applies to all of
/// them: never more than `keep` remain, each remaining one is complete and
/// readable. Without `refresh` the builds of the unchanged scope that follow
/// all answer with the one current snapshot.
#[test]
fn simultaneous_builds_each_publish_a_complete_snapshot_under_retention() {
    const BUILDS: usize = 3;
    const KEEP: usize = 2;
    let dir = Fixture::new();
    dir.write("a.ts", "export const a = 1;");
    let simultaneous = |row: Value| {
        std::thread::scope(|scope| {
            let tasks = (0..BUILDS)
                .map(|_| scope.spawn(|| call(&dir.0, ToolId::GraphBuild, row.clone())))
                .collect::<Vec<_>>();
            tasks
                .into_iter()
                .map(|task| task.join().expect("build"))
                .collect::<Vec<_>>()
        })
    };
    let ids_of = |outputs: &[Value]| {
        outputs
            .iter()
            .map(|out| out["id"].as_str().expect("id").to_owned())
            .collect::<BTreeSet<_>>()
    };
    let outputs = simultaneous(json!({"keep": KEEP, "refresh": true}));
    let ids = ids_of(&outputs);
    assert_eq!(
        ids.len(),
        BUILDS,
        "every refresh publishes its own id: {outputs:?}"
    );
    let home = dir.0.join(".octocode/graph");
    let unchanged = simultaneous(json!({"keep": KEEP}));
    let latest = std::fs::read_to_string(home.join(LATEST_FILE)).expect("latest");
    assert_eq!(
        ids_of(&unchanged),
        BTreeSet::from([latest]),
        "an unchanged scope has one current snapshot: {unchanged:?}"
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
    impl octocode_runtime_base::execution::cancel::CancellationCheck for Spent {
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
    let (workspace, root, _) = scope("app", &paths, None, &|_, _| None).expect("scope");
    let query: GraphBuildQuery =
        serde_json::from_value(json_fixture(r#"{"path": "app", "refresh": true}"#)).expect("query");
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
    octocode_runtime_base::test_support::assert_row_contract(ToolId::GraphBuild, &data);
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
        let hints = built["warnings"].to_string();
        assert!(hints.contains("were not searched"), "{path}: {built}");
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
        octocode_runtime_base::contracts::validate_built_query(
            ToolId::GraphBuild.as_str(),
            query.clone(),
        )
        .unwrap_or_else(|error| panic!("{path}: the lead is not runnable: {error:?}"));
        let lifted = call(&dir.0, ToolId::GraphBuild, query.clone());
        assert!(
            lifted["counts"]["nodes"].as_u64().unwrap_or(0) > 0,
            "{path}: the lead builds the skipped files: {lifted}"
        );
    }
    // A scope with code says nothing about skipped paths.
    let plain = call(
        &git.0,
        ToolId::GraphBuild,
        json_fixture(r#"{"path": "plain"}"#),
    );
    assert!(plain.get("hints").is_none(), "{plain}");
}

/// A build names the files of its scope that no graph grammar parses, as
/// astSearch names the ones no AST grammar does: every one of them when the
/// build holds no file, and beside parsed files the ones of no known type by
/// name and the rest as a count per type. Script-bearing markup is read by
/// the build, so it is not among them.
#[test]
fn a_build_names_the_files_no_graph_grammar_parses() {
    let dir = Fixture::new();
    dir.write("docs/a.md", "# a\n");
    dir.write("docs/b.md", "# b\n");
    dir.write("app/a.ts", "export function a() { return 1; }\n");
    dir.write("app/notes.md", "# notes\n");
    dir.write("app/tool.unknowntype", "fn main() {}\n");
    dir.write("app/page.vue", "<script>export default {}</script>\n");
    dir.write("src/c.ts", "export const c = 1;\n");

    // A build that holds no file is empty and states what it read: a fresh
    // build and an unchanged scope's reuse alike.
    let request = octocode_runtime_base::contracts::validate_built_query(
        ToolId::GraphBuild.as_str(),
        json_fixture(r#"{"path": "docs"}"#),
    )
    .expect("input contract");
    for build in ["fresh", "reused"] {
        let result = execute(ToolId::GraphBuild, &request, &dir.0, &context()).expect("execute");
        assert_eq!(result.status, Some("empty"), "{build}: {}", result.data);
        assert_eq!(
            result.data["scan"]["filesScanned"], 0,
            "{build}: {}",
            result.data
        );
    }
    let empty = call(
        &dir.0,
        ToolId::GraphBuild,
        json_fixture(r#"{"path": "docs"}"#),
    );
    assert_eq!(empty["counts"]["nodes"], 0, "{empty}");
    let hints = empty["warnings"].to_string();
    assert!(
        hints.contains("2 files skipped: no graph grammar for their names (docs/a.md, docs/b.md)"),
        "{empty}"
    );
    assert!(hints.contains("parserGlobs"), "{empty}");
    // Every file the warning counts is its own `unparsed` entry, by type.
    // Recorded absolute, spelled from the workspace as the runtime responds;
    // the walk finished, so the list is whole (no `sampled`).
    let skipped = |row: &Value| {
        let mut skipped = row["skippedPaths"].clone();
        octocode_runtime_base::response::paths::shown_paths(
            true,
            &mut skipped,
            &workspace_policy(&dir.0),
            &Value::Null,
        );
        skipped
    };
    assert_eq!(
        skipped(&empty),
        json_fixture(
            r#"{
                "entries": [
                    {
                        "path": "docs/a.md",
                        "cause": "unparsed",
                        "reason": "markdown"
                    },
                    {
                        "path": "docs/b.md",
                        "cause": "unparsed",
                        "reason": "markdown"
                    }
                ]
            }"#
        ),
        "{empty}"
    );
    assert!(!hints.contains("behind skipped paths"), "{empty}");
    assert!(
        !hints.contains("searched"),
        "a build searches nothing: {empty}"
    );

    let partly = call(
        &dir.0,
        ToolId::GraphBuild,
        json_fixture(r#"{"path": "app"}"#),
    );
    assert!(
        partly["counts"]["nodes"].as_u64().unwrap_or(0) > 0,
        "{partly}"
    );
    let hints = partly["warnings"].to_string();
    assert!(
        hints.contains("1 file skipped: no graph grammar for its name (app/tool.unknowntype)"),
        "{partly}"
    );
    assert!(hints.contains("markdown 1"), "{partly}");
    // Beside parsed files the warning counts the file of no known type: it
    // is the one entry; the markdown file stays a count.
    assert_eq!(
        skipped(&partly),
        json_fixture(
            r#"{
                "entries": [{"path": "app/tool.unknowntype", "cause": "unparsed"}]
            }"#
        ),
        "{partly}"
    );
    assert!(!hints.contains("page.vue"), "{partly}");
    assert!(!hints.contains("a.ts"), "{partly}");

    // The file `parserGlobs` maps to a grammar is parsed, not named.
    let mapped = call(
        &dir.0,
        ToolId::GraphBuild,
        json_fixture(r#"{"path": "app", "parserGlobs": {"rust": ["*.unknowntype"]}}"#),
    );
    assert!(
        !mapped["warnings"].to_string().contains("tool.unknowntype"),
        "{mapped}"
    );

    // A scope every file of which is parsed says nothing.
    let plain = call(
        &dir.0,
        ToolId::GraphBuild,
        json_fixture(r#"{"path": "src"}"#),
    );
    assert!(plain.get("warnings").is_none(), "{plain}");
    assert!(plain.get("skippedPaths").is_none(), "{plain}");

    // The graph store the builds above wrote is Octocode's own, never the
    // scope's: a walk that takes dot entries names none of its files.
    assert!(dir.0.join(".octocode/graph").is_dir());
    let whole = call(
        &dir.0,
        ToolId::GraphBuild,
        json_fixture(r#"{"path": ".", "includeHidden": true}"#),
    );
    let hints = whole["warnings"].to_string();
    assert!(hints.contains("tool.unknowntype"), "{whole}");
    assert!(!hints.contains(".octocode"), "{whole}");
    assert!(
        !hints.contains("bin "),
        "the store's files are counted: {whole}"
    );
}

/// A snapshot built by another analysis version is refused by path and by
/// ID, on the row both interfaces serve; its lead is the build that replaces
/// it, and the rebuilt snapshot answers.
#[test]
fn a_snapshot_of_another_analysis_version_leads_to_its_rebuild() {
    let dir = Fixture::new();
    dir.write(
        "app/a.ts",
        "export function first() { return 1; }\nexport function final() { return first(); }\n",
    );
    let built = call(
        &dir.0,
        ToolId::GraphBuild,
        json_fixture(r#"{"path": "app"}"#),
    );
    let id = built["id"].as_str().expect("id");
    let snapshot = dir.0.join(".octocode/graph").join(id);
    let mut manifest = read_manifest(&snapshot).expect("manifest");
    manifest["analysisVersion"] = json!(crate::ast_graph::ANALYSIS_VERSION - 1);
    std::fs::write(
        snapshot.join(MANIFEST_FILE),
        serde_json::to_vec(&manifest).expect("json"),
    )
    .expect("write");

    let by_path = call(
        &dir.0,
        ToolId::GraphQuery,
        json_fixture(
            r#"{
                "operation": "callers",
                "path": "app",
                "target": "app/a.ts#first"
            }"#,
        ),
    );
    let by_id = call(
        &dir.0,
        ToolId::GraphQuery,
        json!({"operation":"callers","path":"app","graph":id,"target":"app/a.ts#first"}),
    );
    assert_eq!(by_path, by_id);
    assert_eq!(by_path["errorCode"], "snapshotIncompatible", "{by_path}");
    assert!(by_path.get("rows").is_none(), "{by_path}");
    assert!(by_path.get("graph").is_none(), "{by_path}");
    let lead = &by_path["related"][0];
    assert_eq!(lead["tool"], "graphBuild", "{by_path}");
    let rebuilt = call(
        &dir.0,
        ToolId::GraphBuild,
        lead["input"]["queries"][0].clone(),
    );
    assert_ne!(rebuilt["id"], built["id"], "{rebuilt}");
    let callers = call(
        &dir.0,
        ToolId::GraphQuery,
        json_fixture(
            r#"{
                "operation": "callers",
                "path": "app",
                "target": "app/a.ts#first"
            }"#,
        ),
    );
    assert_eq!(callers["graph"]["id"], rebuilt["id"], "{callers}");
    assert_eq!(callers["rows"][0]["id"], "app/a.ts#final", "{callers}");
}
