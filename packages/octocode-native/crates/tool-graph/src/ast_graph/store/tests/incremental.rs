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
fn cached_extraction_relinks_add_edit_delete_rename_and_negative_dependencies() {
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
    assert_eq!(added.class, ExitClass::Success, "{}", added.value);
    assert_eq!(added.value["extraction"]["parsedFiles"], 1, "the new file");
    assert_eq!(added.value["extraction"]["reusedFiles"], 2);
    assert_eq!(
        ask(&dir.0, "callees", Some("app/main.ts#run"), |_| {}).value["pagination"]["total"],
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
        ask(&dir.0, "callees", Some("app/main.ts#run"), |_| {}).value["pagination"]["total"],
        0
    );
}

#[test]
fn unchanged_sources_are_rebound_when_resolution_configuration_changes() {
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
    let out = ask(&dir.0, "callees", Some("app/main.ts#run"), |_| {});
    assert_eq!(
        out.value["rows"][0]["id"], "app/right.ts#work",
        "{}",
        out.value
    );
}

/// A rebuild after an edit hands the secret scan only the strings that no
/// snapshot of the same inputs already stores, and still publishes what a
/// clean build does, with the edit's secret redacted.
#[test]
fn an_edit_rebuild_scans_only_the_strings_the_edit_brought() {
    let dir = git_app(
        &(0..6)
            .map(|index| {
                (
                    format!("m{index}.ts"),
                    format!("export function f{index}() {{ return 'text {index}'; }}\n"),
                )
            })
            .collect::<Vec<_>>()
            .iter()
            .map(|(name, text)| (name.as_str(), text.as_str()))
            .collect::<Vec<_>>(),
    );
    let root = dir.0.join("app");
    let scanned = || super::super::ingest::SCANNED_STRINGS.with(std::cell::Cell::get);
    let before = scanned();
    let first = ingest_root(&dir.0, &root, None, false);
    assert_eq!(first.class, ExitClass::Success, "{}", first.value);
    let cold = scanned() - before;
    let token = "ghp_aB3dE6gH9jK2mN5pQ8sT1vW4yZ7bC0eF3hJ6";
    std::fs::write(
        root.join("m0.ts"),
        format!("import {{ x }} from '{token}';\nexport function f0() {{ return x; }}\n"),
    )
    .unwrap();
    let before = scanned();
    let edited = ingest_root(&dir.0, &root, None, false);
    assert_eq!(edited.class, ExitClass::Success, "{}", edited.value);
    let rescanned = scanned() - before;
    assert!(rescanned > 0 && rescanned < cold, "{rescanned} of {cold}");
    let bytes = read_graph_file(
        &PathBuf::from(edited.value["dir"].as_str().expect("dir")).join(GRAPH_FILE),
    )
    .expect("graph");
    let tables = format::decode(&bytes).expect("decode").0;
    assert!(
        tables.strings.iter().all(|text| !text.contains(token)),
        "the edit's secret is redacted"
    );
    assert_clean_equivalent(&dir.0);
}
