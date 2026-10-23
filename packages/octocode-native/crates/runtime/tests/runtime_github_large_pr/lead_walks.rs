//! Lead walks: following patch-hop leads and overflowing hops pages by rows without loss.

use super::{mount_file_batches, mount_pr, numbered, raw_patch, rest_file};
use crate::support::{self, Run as _};
use serde_json::{Value, json};
use std::time::Duration;
use support::Workspace;
use wiremock::MockServer;

/// A 115-file, ~560k-char pull request walked by following its leads
/// only (`continuePatch`, then `nextFilePage`) at a 20k configured page reads
/// every patch byte-identical (after the gutters) with no gap and no
/// repeat, and never overflows its page. Every hop runs at the first call's
/// page (no doubled `responseLength`, 2026-10-09): 21 → 44 calls, each the
/// size of the first (the numbered view is ~10% larger than the raw
/// patches).
#[tokio::test]
async fn history_b_large_pr_lead_walk_is_lossless_at_the_first_page_size() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 115).await;
    // A few large patches and a long tail of small ones (~560k chars).
    let size = |i: usize| match i {
        0 => 70_000,
        7 => 48_000,
        19 => 33_000,
        40 => 26_000,
        77 => 21_000,
        _ => 1_200 + (i * 1_327) % 4_200,
    };
    let patches = (0..115)
        .map(|i| {
            let mut patch = format!("@@ -1,{0} +1,{0} @@ fn f{i}\n", size(i) / 40);
            let mut line = 0;
            while patch.len() < size(i) {
                line += 1;
                patch.push_str(&format!(
                    "+let value_{i}_{line} = compute({line}); // pad\n"
                ));
            }
            patch
        })
        .collect::<Vec<_>>();
    let total: usize = patches.iter().map(String::len).sum();
    assert!((540_000..600_000).contains(&total), "{total}");
    let files = patches
        .iter()
        .enumerate()
        .map(|(i, patch)| rest_file(&format!("src/m{i:03}.rs"), Some(patch), 9, 0))
        .collect::<Vec<_>>();
    mount_file_batches(
        &server,
        vec![files[..100].to_vec(), files[100..].to_vec()],
        Duration::ZERO,
    )
    .await;
    let workspace = Workspace::new();
    // The constant 20k page: 15k patch windows on every call.
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let mut envelope = json!({"queries": [{"operation": "pullRequest", "owner": "a",
        "repo": "b", "number": 9, "sections": ["patches"]}]});
    let mut read = std::collections::BTreeMap::<String, String>::new();
    let mut calls = 0;
    loop {
        calls += 1;
        assert!(calls <= 60, "walk did not finish");
        let outcome = runtime
            .run(
                format!("walk-{calls}"),
                "ghGetHistoryItem".into(),
                envelope.clone(),
            )
            .await
            .expect("patch window");
        let content = &outcome.structured_content;
        assert_ne!(
            content["responsePagination"]["hasMore"], true,
            "call {calls}: a patch window overflowed the page it asked for"
        );
        let data = &content["results"][0]["data"];
        for file in data["pullRequests"][0]["files"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let Some(patch) = file["patch"].as_str() else {
                continue;
            };
            let path = file["path"].as_str().expect("path").to_owned();
            let text = read.entry(path).or_default();
            let offset = file["patchPagination"]["offset"].as_u64().unwrap_or(0);
            assert_eq!(
                offset as usize,
                text.chars().count(),
                "gap or repeat: {file}"
            );
            text.push_str(patch);
        }
        let next = &data["next"];
        let Some(lead) = ["continuePatch", "nextFilePage"]
            .iter()
            .find_map(|name| next.get(*name))
        else {
            break;
        };
        envelope = lead["query"].clone();
    }
    for (i, patch) in patches.iter().enumerate() {
        let path = format!("src/m{i:03}.rs");
        assert_eq!(
            read.get(&path).map(|view| raw_patch(view)).as_ref(),
            Some(patch),
            "{path}"
        );
    }
    assert!(calls <= 45, "{calls} calls");
    runtime.close().await;
}

/// A patch-walk hop that outgrows its page pages its response by
/// whole rows (automatic `responseScope:"rows"`), never by text windows
/// that empty the structured `results` and bury the walk's `next.*`. File
/// page 2 lists 96 patch-less files with long names beside a few patches:
/// the first window's metadata rows and summary alone exceed the 20k page,
/// so the row splits into structured `rowPart`s. Following
/// `responsePagination.next` first, then `continuePatch`/`nextFilePage`,
/// every call keeps its rows, every patch arrives exactly once (a patch
/// identical to an earlier one of its file page names that row,
/// `samePatchAs`), and every patch-less file is listed exactly once: the
/// walk's cursor rides the last part, so no part is skipped.
#[tokio::test]
async fn overflowing_patch_hop_pages_by_rows_and_loses_nothing() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 200).await;
    let long = |i: usize| {
        format!(
            "src/generated/{}_{i:03}.ts",
            "very_long_generated_name".repeat(10)
        )
    };
    let mut second = (0..96)
        .map(|i| rest_file(&long(i), None, 0, 0))
        .collect::<Vec<_>>();
    second.extend((0..4).map(|i| {
        let patch = format!(
            "@@ -1,150 +1,150 @@ fn big{i}\n{}",
            (0..150)
                .map(|line| format!("+let changed_{i}_{line} = compute({line}); // padding\n"))
                .collect::<String>()
                .trim_end()
        );
        rest_file(&format!("src/big{i}.rs"), Some(&patch), 150, 0)
    }));
    let first = numbered(0, 100);
    let expected = first
        .iter()
        .chain(&second)
        .filter_map(|file| {
            Some((
                file["filename"].as_str()?.to_owned(),
                file["patch"].as_str()?.to_owned(),
            ))
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    mount_file_batches(&server, vec![first, second], Duration::ZERO).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let mut envelope = json!({"queries": [{"operation": "pullRequest", "owner": "a",
        "repo": "b", "number": 9, "sections": ["patches"]}]});
    let mut read = std::collections::BTreeMap::<String, String>::new();
    let mut listed = std::collections::BTreeMap::<String, usize>::new();
    let mut same = std::collections::BTreeMap::<String, String>::new();
    let (mut calls, mut split_parts) = (0, 0);
    loop {
        calls += 1;
        assert!(calls <= 40, "walk did not finish");
        let outcome = runtime
            .run(
                format!("walk-{calls}"),
                "ghGetHistoryItem".into(),
                envelope.clone(),
            )
            .await
            .unwrap_or_else(|error| panic!("call {calls} failed: {error:?}\n{envelope}"));
        let content = &outcome.structured_content;
        let pagination = &content["responsePagination"];
        if pagination.is_object() {
            assert_eq!(pagination["scope"], "rows", "call {calls}: {pagination}");
        }
        let row = &content["results"][0];
        if row.get("rowPart").is_some() {
            split_parts += 1;
        }
        let data = &row["data"];
        // A later part of a split row omits a list the split emptied there
        // (it pages another list); a whole row always shows its files.
        let empty = Vec::new();
        let files = match data["pullRequests"][0]["files"].as_array() {
            Some(files) => files,
            None if row.get("rowPart").is_some_and(|part| part["part"] != 1) => &empty,
            None => panic!("call {calls}: rows hidden from structuredContent: {content}"),
        };
        for file in files {
            let path = file["path"].as_str().expect("path").to_owned();
            if let Some(earlier) = file["samePatchAs"].as_str() {
                assert!(
                    same.insert(path, earlier.to_owned()).is_none(),
                    "call {calls}: {file}"
                );
                continue;
            }
            match file["patch"].as_str() {
                Some(patch) => {
                    let text = read.entry(path).or_default();
                    let offset = file["patchPagination"]["offset"].as_u64().unwrap_or(0);
                    assert_eq!(
                        offset as usize,
                        text.chars().count(),
                        "call {calls}: {file}"
                    );
                    text.push_str(patch);
                }
                None => *listed.entry(path).or_default() += 1,
            }
        }
        let next = pagination["next"]["query"]
            .as_object()
            .or_else(|| data["next"]["continuePatch"]["query"].as_object())
            .or_else(|| data["next"]["nextFilePage"]["query"].as_object());
        match next {
            Some(next) => envelope = Value::Object(next.clone()),
            None => break,
        }
    }
    assert!(split_parts >= 2, "the hop never overflowed its page");
    assert_eq!(
        read.len() + same.len(),
        expected.len(),
        "patches reached: {} + {} repeats",
        read.len(),
        same.len()
    );
    for (path, patch) in &expected {
        let holder = same.get(path).unwrap_or(path);
        assert_eq!(
            read.get(holder).map(|view| raw_patch(view)).as_ref(),
            Some(patch),
            "{path}"
        );
    }
    assert_eq!(listed.len(), 96, "{listed:?}");
    assert!(listed.values().all(|count| *count == 1), "{listed:?}");
    runtime.close().await;
}
