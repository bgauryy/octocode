//! Every local page channel, walked generically ([`support::walk`]): each page
//! satisfies the output contract and offers only contract-valid leads, every
//! page lead is followed verbatim, no call repeats, and every row arrives
//! exactly once with the union equal to the unpaged rows.

use crate::support::{self, Workspace};
use serde_json::{Value, json};

fn fixture(workspace: &Workspace) {
    for index in 0..60 {
        workspace.write(
            &format!("src/mod{index}.rs"),
            format!("pub fn helper{index}() -> u32 {{\n    {index}\n}}\n\npub fn caller{index}() -> u32 {{\n    helper{index}()\n}}\n"),
        );
    }
}

/// The first result row's data, without its leads.
fn data(output: &Value) -> Value {
    let mut data = output["results"][0]["data"].clone();
    if let Some(map) = data.as_object_mut() {
        map.remove("next");
        map.remove("related");
    }
    data
}

/// `path:line` of every match of a localSearch page.
fn search_rows(output: &Value) -> Vec<String> {
    data(output)["files"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|file| {
            let path = file["path"].as_str().expect("file path").to_owned();
            file["matches"]
                .as_array()
                .into_iter()
                .flatten()
                .map(move |hit| format!("{path}:{}", hit["line"]))
        })
        .collect()
}

/// `dir/name` of every file a structureSearch page lists (`{dir, files:
/// ["name (size, ...)"]}` groups).
fn listed_rows(output: &Value) -> Vec<String> {
    data(output)["files"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|group| {
            let dir = group["dir"].as_str().expect("group dir").to_owned();
            group["files"]
                .as_array()
                .into_iter()
                .flatten()
                .map(move |entry| {
                    let entry = entry.as_str().expect("file entry");
                    let name = entry.split(" (").next().expect("file name");
                    format!("{dir}/{name}")
                })
        })
        .collect()
}

#[tokio::test]
async fn local_page_channels_walk_exactly_once() {
    let workspace = Workspace::new();
    fixture(&workspace);
    let runtime = workspace.runtime(&[]);
    type Rows = fn(&Value) -> Vec<String>;
    let cases: [(&str, Value, Rows); 2] = [
        (
            "localSearch",
            json!({"queries":[{"path":"src","matchString":["helper"]}],"responseChars":1000}),
            search_rows,
        ),
        (
            "structureSearch",
            json!({"queries":[{"operation":"files","path":"src"}],"responseChars":600}),
            listed_rows,
        ),
    ];
    for (tool, seed, rows) in cases {
        let mut unpaged = seed.clone();
        unpaged
            .as_object_mut()
            .expect("seed")
            .remove("responseChars");
        let whole = support::run(&runtime, tool, unpaged).await;
        let mut expected = rows(&whole.structured_content);
        expected.sort();
        assert!(expected.len() >= 60, "{tool}: {}", whole.structured_content);
        let walk = support::walk(&runtime, tool, support::envelope(seed), rows).await;
        assert!(walk.queries.len() >= 3, "{tool} pages: {:?}", walk.queries);
        let mut walked = walk.rows;
        walked.sort();
        assert_eq!(walked, expected, "{tool}");
    }
    runtime.close().await;
}
