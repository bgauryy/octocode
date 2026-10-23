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
    let walk = |page: Option<u64>, char_length: Option<u64>| {
        let runtime = &runtime;
        async move {
            let mut first = json!({"operation": "pullRequest", "owner": "a", "repo": "b",
                "number": 9, "mainGoal": "g", "reasoning": "r",
                "sections": ["patches"], "extras": ["hints"]});
            if let Some(length) = char_length {
                first["length"] = json!(length);
            }
            let with_page = |row: Value| {
                let mut envelope = json!({"queries": [row]});
                if let Some(page) = page {
                    envelope["responseLength"] = json!(page);
                }
                envelope
            };
            let mut hops = std::collections::VecDeque::from([with_page(first)]);
            let mut read = std::collections::BTreeMap::<String, String>::new();
            let mut calls = 0;
            while let Some(envelope) = hops.pop_front() {
                calls += 1;
                assert!(calls <= 60, "walk did not finish");
                let outcome = runtime
                    .run(format!("walk-{calls}"), "ghGetHistoryItem".into(), envelope)
                    .await
                    .expect("patch window");
                let content = &outcome.structured_content;
                // An explicit page reports its (single) response page.
                assert_ne!(
                    content["responsePagination"]["hasMore"], true,
                    "a patch window overflowed its page: {content}"
                );
                let data = &content["results"][0]["data"];
                // A length above the page is clamped and says so on the
                // call that asked; the windows below still tile every patch.
                let clamped = data["warnings"].as_array().is_some_and(|text| {
                    text.iter()
                        .any(|t| t.as_str().is_some_and(|t| t.starts_with("length")))
                });
                assert_eq!(
                    clamped,
                    char_length.is_some() && calls == 1,
                    "call {calls}: {data}"
                );
                for file in data["pullRequests"][0]["files"]
                    .as_array()
                    .into_iter()
                    .flatten()
                {
                    let path = file["path"].as_str().expect("path").to_owned();
                    let text = read.entry(path).or_default();
                    let offset = file["patchPagination"]["offset"].as_u64().unwrap_or(0);
                    assert_eq!(offset as usize, text.chars().count(), "{file}");
                    text.push_str(file["patch"].as_str().unwrap_or(""));
                }
                // One check at the merge commit, on the first window only.
                assert_eq!(
                    !crate::support::checks::hint(data, "readAtMerge").is_null(),
                    calls == 1,
                    "call {calls}: {data}"
                );
                // Each hop is a complete one-row input: the same file page
                // at the stream cursor, with the page budget of the first
                // window.
                if let Some(next) = data["next"]["continuePatch"]["query"]["queries"][0].as_object()
                {
                    assert_eq!(
                        data["next"]["continuePatch"]["query"]["queries"]
                            .as_array()
                            .map(Vec::len),
                        Some(1),
                        "{data}"
                    );
                    assert_eq!(next.get("sections"), Some(&json!(["patches"])), "{data}");
                    assert!(next.get("include").is_none(), "{data}");
                    // A caller's oversized length rides the hop clamped to
                    // the window it got, so the clamp is said once.
                    match char_length {
                        None => assert!(next.get("length").is_none(), "{data}"),
                        Some(asked) => assert!(
                            next["length"].as_u64().is_some_and(|length| length < asked),
                            "{data}"
                        ),
                    }
                    hops.push_back(with_page(Value::Object(next.clone())));
                }
            }
            (calls, read)
        }
    };
    let (default_calls, default_read) = walk(None, None).await;
    let (explicit_calls, explicit_read) = walk(Some(50_000), None).await;
    let (_, oversized_read) = walk(None, Some(80_000)).await;
    for (path, patch) in ["src/a.rs", "src/b.rs"].iter().zip(&patches) {
        for read in [&default_read, &explicit_read, &oversized_read] {
            assert_eq!(
                read.get(*path).map(|view| raw_patch(view)).as_ref(),
                Some(patch),
                "{path}"
            );
        }
    }
    // A larger explicit response page carries larger windows: fewer hops.
    assert!(
        explicit_calls < default_calls,
        "explicit {explicit_calls} vs default {default_calls} calls"
    );
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
        let next = content["responsePagination"]["next"]["query"]
            .as_object()
            .or_else(|| data["next"]["continuePatch"]["query"].as_object())
            .or_else(|| data["next"]["nextFilePage"]["query"].as_object());
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
