//! Patch walks: explicit response page sizes and the default walk reach every patch once.

use super::{mount_file_batches, mount_pr, raw_patch, rest_file};
use crate::support::{self, Run as _};
use serde_json::{Value, json};
use std::time::Duration;
use support::Workspace;
use wiremock::MockServer;

/// A whole-PR patch walk that asks for a larger response page gets patch
/// windows sized to it, and the windows still tile every patch exactly (no
/// gap, no repeat). Every hop has the page budget of the first window and
/// runs unchanged.
#[tokio::test]
async fn explicit_response_page_sizes_patch_walk_windows() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 2).await;
    let patch = |tag: &str| {
        format!(
            "@@ -1,9000 +1,9000 @@\n{}",
            (0..6_000)
                .map(|i| format!("+{tag} line {i}\n"))
                .collect::<String>()
        )
    };
    let patches = [patch("a"), patch("b")];
    mount_file_batches(
        &server,
        vec![vec![
            rest_file("src/a.rs", Some(&patches[0]), 6_000, 0),
            rest_file("src/b.rs", Some(&patches[1]), 6_000, 0),
        ]],
        Duration::ZERO,
    )
    .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    for budget in [None, Some(50_000)] {
        let mut input = json!({"queries":[{"operation":"pullRequest","owner":"a","repo":"b","number":9,"sections":["patches"]}]});
        if let Some(budget) = budget {
            input["responseChars"] = json!(budget);
        }
        let mut read = std::collections::BTreeMap::<String, String>::new();
        for hop in 0..100 {
            let outcome = runtime
                .run(format!("patch-{hop}"), "ghGetHistoryItem".into(), input)
                .await
                .expect("patch window");
            let content = &outcome.structured_content;
            let data = &content["results"][0]["data"];
            for file in data["pullRequests"][0]["files"]
                .as_array()
                .into_iter()
                .flatten()
            {
                let Some(patch) = file["patch"].as_str() else {
                    continue;
                };
                let text = read
                    .entry(file["path"].as_str().expect("path").to_owned())
                    .or_default();
                assert_eq!(
                    file["patchPagination"]["offset"].as_u64().unwrap_or(0) as usize,
                    text.chars().count()
                );
                text.push_str(patch);
            }
            let next = content
                .pointer("/responsePagination/next")
                .or_else(|| data.pointer("/next/continuePatch"))
                .or_else(|| data.pointer("/next/nextFilePage"));
            let Some(next) = next else { break };
            assert!(next["input"]["cursor"].is_string());
            input = next["input"].clone();
        }
        for (path, patch) in ["src/a.rs", "src/b.rs"].iter().zip(&patches) {
            assert_eq!(
                read.get(*path).map(|view| raw_patch(view)).as_ref(),
                Some(patch),
                "{path}"
            );
        }
    }
    runtime.close().await;
}

/// A 115-file pull request (one 100-file provider batch, then 15 files)
/// read with the default `sections:["patches"]`: following only the
/// returned continuations (`responsePagination.next` first, then
/// `next.continuePatch` / `next.nextFilePage`) every call is a
/// contract-valid success and the walk delivers every patch exactly once.
/// The first window of a file page that does not fit one response also
/// lists the page's `fileSummary`: that list must ride inside the page
/// budget, or the row splits into a part with an empty `files` array.
#[tokio::test]
async fn default_large_pr_patch_walk_reaches_every_patch_once() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 115).await;
    let file = |index: usize| {
        let dir = [
            "packages/react-devtools-timeline/src/content-views/utils",
            "packages/react-devtools-shared/src/devtools/views/Profiler",
            "packages/react-devtools-shell/src/app/InspectableElements",
        ][index % 3];
        let name = format!("{dir}/ComponentMeasuresView{index}.js");
        let lines = 6 + (index * 37) % 140;
        let body = (0..lines)
            .map(|line| {
                format!("-  const value{line} = \"timeline\" + computeLane({index}, {line});\n")
            })
            .collect::<String>();
        let patch = format!(
            "@@ -1,{lines} +0,0 @@ module.exports = {{\n{}",
            body.trim_end()
        );
        rest_file(&name, Some(&patch), 0, lines as u64)
    };
    let files = (0..115).map(file).collect::<Vec<_>>();
    let expected = files
        .iter()
        .map(|file| {
            (
                file["filename"].as_str().expect("name").to_owned(),
                file["patch"].as_str().expect("patch").to_owned(),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    mount_file_batches(
        &server,
        vec![files[..100].to_vec(), files[100..].to_vec()],
        Duration::ZERO,
    )
    .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let mut envelope = json!({"queries": [{"operation": "pullRequest", "owner": "a",
        "repo": "b", "number": 9, "sections": ["patches"]}]});
    let mut read = std::collections::BTreeMap::<String, String>::new();
    let mut calls = 0;
    loop {
        calls += 1;
        assert!(calls <= 80, "walk did not finish");
        let outcome = runtime
            .run(
                format!("walk-{calls}"),
                "ghGetHistoryItem".into(),
                envelope.clone(),
            )
            .await
            .unwrap_or_else(|error| panic!("call {calls} failed: {error:?}\n{envelope}"));
        let content = &outcome.structured_content;
        assert_eq!(
            content["results"][0]["status"]
                .as_str()
                .unwrap_or("success"),
            "success",
            "{content}"
        );
        // A default patch window fits its response page: the row never
        // splits into response parts.
        assert!(
            content.get("responsePagination").is_none(),
            "call {calls}: {}",
            content["responsePagination"]
        );
        let data = &content["results"][0]["data"];
        for file in data["pullRequests"][0]["files"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let path = file["path"].as_str().expect("path").to_owned();
            let text = read.entry(path).or_default();
            let offset = file["patchPagination"]["offset"].as_u64().unwrap_or(0);
            assert_eq!(
                offset as usize,
                text.chars().count(),
                "call {calls}: {file}"
            );
            text.push_str(file["patch"].as_str().unwrap_or(""));
        }
        let next = content["responsePagination"]["next"]["input"]
            .as_object()
            .or_else(|| data["next"]["continuePatch"]["input"].as_object())
            .or_else(|| data["next"]["nextFilePage"]["input"].as_object());
        match next {
            Some(next) => envelope = Value::Object(next.clone()),
            None => break,
        }
    }
    assert_eq!(read.len(), 115, "patches reached: {}", read.len());
    for (path, patch) in &expected {
        assert_eq!(
            read.get(path).map(|view| raw_patch(view)).as_ref(),
            Some(patch),
            "{path}"
        );
    }
    runtime.close().await;
}
