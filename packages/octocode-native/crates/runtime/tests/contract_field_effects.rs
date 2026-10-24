//! Common fields must change observable behavior consistently across tools.
use crate::support::{Workspace, call, row_data};
use serde_json::json;

#[tokio::test]
async fn matching_and_context_have_the_same_meaning_in_search_and_reads() {
    let workspace = Workspace::new();
    let path = workspace.write(
        "source.txt",
        "before\nNeedle\nafter\nseparator\nneedle\ntail\n",
    );
    let runtime = workspace.runtime(&[]);
    for (mode, expected) in [("sensitive", vec![5]), ("insensitive", vec![2, 5])] {
        for tool in ["localSearch", "localFetch"] {
            let result=call(&runtime,tool,json!({"path":path,"matchString":["needle"],"caseMode":mode,"regexEngine":"literal","contextLines":{"before":1,"after":0}})).await.expect("query executes");
            let data = row_data(&result);
            let content = if tool == "localFetch" {
                data["content"].as_str().expect("source").to_owned()
            } else {
                data["files"][0]["matches"]
                    .as_array()
                    .expect("matches")
                    .iter()
                    .map(|hit| hit["value"].as_str().expect("value"))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            let numbered = content
                .lines()
                .filter_map(|line| line.split_once('\t'))
                .filter_map(|(number, text)| Some((number.parse::<u64>().ok()?, text)))
                .collect::<std::collections::BTreeMap<_, _>>();
            let wanted = expected
                .iter()
                .flat_map(|line| [line - 1, *line])
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(
                numbered
                    .keys()
                    .copied()
                    .collect::<std::collections::BTreeSet<_>>(),
                wanted,
                "{tool} {mode}: {data}"
            );
            for hit in &expected {
                assert_eq!(numbered[hit].to_lowercase(), "needle");
            }
        }
    }
    runtime.close().await;
}

#[tokio::test]
async fn shared_walk_filters_select_the_same_files_in_text_structure_and_ast() {
    let workspace = Workspace::new();
    for path in [
        "src/keep.rs",
        "src/drop.rs",
        "test/keep.rs",
        "src/nested/keep.rs",
    ] {
        workspace.write(path, "pub fn needle() {}\n");
    }
    let runtime = workspace.runtime(&[]);
    for (tool, mut query) in [
        (
            "localSearch",
            json!({"matchString":["needle"],"view":"files"}),
        ),
        (
            "structureSearch",
            json!({"operation":"files","entryType":"file"}),
        ),
        (
            "astSearch",
            json!({"operation":"match","parser":"rust","rule":{"pattern":"pub fn needle() {}"},"view":"files"}),
        ),
    ] {
        query["path"] = json!(workspace.workspace);
        query["include"] = json!(["src/**/*.rs"]);
        query["exclude"] = json!(["**/drop.rs", "src/nested/**"]);
        let result = call(&runtime, tool, query).await.expect("filtered walk");
        let data = row_data(&result);
        let paths = data["files"]
            .as_array()
            .expect("files")
            .iter()
            .flat_map(|row| {
                if let Some(path) = row["path"].as_str() {
                    vec![path.to_owned()]
                } else {
                    row["files"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|entry| {
                            let entry = entry.as_str().expect("entry");
                            let name = entry
                                .strip_suffix(')')
                                .and_then(|value| value.rsplit_once(" ("))
                                .map_or(entry, |(name, _)| name);
                            format!("{}/{name}", row["dir"].as_str().expect("dir"))
                        })
                        .collect()
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(paths, ["src/keep.rs"], "{tool}: {data}");
    }
    runtime.close().await;
}
