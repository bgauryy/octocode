use super::*;

pub(super) fn assert_clean_equivalent(dir: &Path) {
    let incremental = ingest_root(dir, &dir.join("app"), None, false);
    assert_eq!(
        incremental.class,
        ExitClass::Success,
        "{}",
        incremental.value
    );
    let clean = ingest_root(dir, &dir.join("app"), None, true);
    assert_eq!(clean.class, ExitClass::Success, "{}", clean.value);
    let load = |output: &GraphOutput| {
        let bytes =
            read_graph_file(&PathBuf::from(output.value["dir"].as_str().unwrap()).join(GRAPH_FILE))
                .unwrap();
        format::decode(&bytes).unwrap().0
    };
    let a = load(&incremental);
    let b = load(&clean);
    assert_eq!(a.strings, b.strings);
    assert_eq!(a.nodes, b.nodes);
    assert_eq!(a.edges, b.edges);
    assert_eq!(a.witnesses, b.witnesses);
    assert_eq!(a.unresolved, b.unresolved);
    assert_eq!(a.declarations, b.declarations);
    assert_eq!(a.diagnostics, b.diagnostics);
    assert_eq!(a.digests, b.digests);
    assert_eq!(a.sources.descriptors, b.sources.descriptors);
    assert_eq!(a.sources, b.sources);
    assert_eq!(incremental.value["calls"], clean.value["calls"]);
    assert_eq!(
        incremental.value["argumentCoverage"],
        clean.value["argumentCoverage"]
    );
    assert_eq!(incremental.value["linkage"], clean.value["linkage"]);
}

#[test]
fn incremental_extraction_relinks_add_edit_delete_rename_and_negative_dependencies() {
    let dir = git_app(&[
        (
            "main.ts",
            "import { work } from './missing'; export function run() { work(); }\n",
        ),
        (
            "stable.py",
            "def stable_incremental_fixture():\n    return 42\n",
        ),
    ]);
    let root = dir.0.join("app");
    ingest_root(&dir.0, &root, None, false);
    std::fs::write(
        root.join("missing.ts"),
        "export function work() { return 1; }\n",
    )
    .unwrap();
    let added = ingest_root(&dir.0, &root, None, false);
    assert!(
        added.value["extraction"]["reusedFiles"].as_u64().unwrap() >= 1,
        "{}",
        added.value
    );
    assert_eq!(
        ask(&dir.0, "callees", Some("main.ts#run"), |_| {}).value["pagination"]["totalItems"],
        1
    );
    assert_clean_equivalent(&dir.0);
    std::fs::write(
        root.join("missing.ts"),
        "export function work() { return changed(); }\nfunction changed(){ return 2; }\n",
    )
    .unwrap();
    assert_clean_equivalent(&dir.0);
    std::fs::rename(root.join("missing.ts"), root.join("renamed.ts")).unwrap();
    assert_clean_equivalent(&dir.0);
    std::fs::remove_file(root.join("renamed.ts")).unwrap();
    assert_clean_equivalent(&dir.0);
    assert_eq!(
        ask(&dir.0, "callees", Some("main.ts#run"), |_| {}).value["pagination"]["totalItems"],
        0
    );
}

#[test]
fn unchanged_extraction_is_rebound_when_resolution_configuration_changes() {
    let dir = git_app(&[
        (
            "main.ts",
            "import { work } from '@lib'; export function run() { work(); }\n",
        ),
        ("left.ts", "export function work() { return 1; }\n"),
        ("right.ts", "export function work() { return 2; }\n"),
        (
            "tsconfig.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@lib":["left.ts"]}}}"#,
        ),
    ]);
    let root = dir.0.join("app");
    ingest_root(&dir.0, &root, None, false);
    std::fs::write(
        root.join("tsconfig.json"),
        r#"{"compilerOptions":{"baseUrl":".","paths":{"@lib":["right.ts"]}}}"#,
    )
    .unwrap();
    assert_clean_equivalent(&dir.0);
    let out = ask(&dir.0, "callees", Some("main.ts#run"), |_| {});
    assert_eq!(
        out.value["rows"][0]["id"], "right.ts#work",
        "{}",
        out.value
    );
}
