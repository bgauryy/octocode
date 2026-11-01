//! A saved page whose response is no longer retained re-executes its rows
//! and answers only when the sources reproduce the content its earlier pages
//! were cut from; a lost handle never executes.

use crate::support::{Run as _, Workspace};
use octocode_contract_types::ErrorCode;
use serde_json::{Value, json};
use std::path::Path;

const STORAGE: [(&str, &str); 1] = [("OCTOCODE_STORAGE_MODE", "persistent")];

fn env() -> Vec<(&'static str, String)> {
    STORAGE
        .iter()
        .map(|(key, value)| (*key, (*value).to_owned()))
        .collect()
}

/// Lines with multi-byte text, so every page boundary is Unicode-checked.
fn source() -> String {
    (0..120)
        .map(|index| {
            format!(
                "needle ünïcødé 😀 {index} {}\n",
                "long value ".repeat(index % 7)
            )
        })
        .collect()
}

/// Make every entry under `dir` look unused for longer than any lifetime.
fn age(dir: &Path) {
    for entry in std::fs::read_dir(dir).expect("store").flatten() {
        let file = std::fs::File::options()
            .write(true)
            .open(entry.path())
            .expect("entry");
        file.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1))
            .expect("age entry");
    }
}

/// Evict every saved response: what a store under memory pressure does.
fn evict(dir: &Path) {
    for entry in std::fs::read_dir(dir).expect("store").flatten() {
        std::fs::remove_file(entry.path()).expect("evict entry");
    }
}

/// Every hit (line and value) of a page, in order.
fn hits(output: &Value) -> Vec<(u64, String)> {
    let mut hits = Vec::new();
    for row in output["results"].as_array().into_iter().flatten() {
        for file in row["data"]["files"].as_array().into_iter().flatten() {
            for hit in file["matches"].as_array().into_iter().flatten() {
                hits.push((
                    hit["line"].as_u64().expect("line"),
                    hit["value"].as_str().unwrap_or_default().to_owned(),
                ));
            }
        }
    }
    hits
}

/// Walk every response page from `start`; before each later page, `before`
/// runs (it may age the stores or restart the runtime).
async fn walk(workspace: &Workspace, start: Value, mut before: impl FnMut()) -> Vec<(u64, String)> {
    let mut runtime = workspace.runtime(&env());
    let first = runtime
        .run("first".into(), "localSearch".into(), start)
        .await
        .expect("first page")
        .structured_content;
    let mut seen = hits(&first);
    let mut next = first.pointer("/responsePagination/next/input").cloned();
    let mut pages = 1;
    while let Some(input) = next {
        before();
        runtime.close().await;
        runtime = workspace.runtime(&env());
        pages += 1;
        let output = runtime
            .run(format!("page-{pages}"), "localSearch".into(), input)
            .await
            .unwrap_or_else(|error| panic!("page {pages}: {error:?}"))
            .structured_content;
        assert_eq!(output["responsePagination"]["page"], pages, "{output}");
        assert_eq!(output["responsePagination"]["replayed"], true, "{output}");
        seen.extend(hits(&output));
        next = output.pointer("/responsePagination/next/input").cloned();
    }
    runtime.close().await;
    assert!(pages > 2, "the fixture spans several pages");
    seen
}

#[tokio::test]
async fn an_unretained_replay_reexecutes_and_every_page_is_read_exactly_once() {
    let workspace = Workspace::new();
    let file = workspace.write("saved.txt", source());
    let pages = workspace.home.join("tmp/response-pages");
    let start = |chars: usize| json!({"queries":[{"path":file,"matchString":["needle"]}],"responseChars":chars});
    for chars in [600, 3000] {
        let retained = walk(&workspace, start(chars), || {}).await;
        let reexecuted = walk(&workspace, start(chars), || evict(&pages)).await;
        let lines: Vec<u64> = reexecuted.iter().map(|(line, _)| *line).collect();
        assert_eq!(
            lines,
            (1..=120).collect::<Vec<_>>(),
            "every hit exactly once at {chars}"
        );
        assert_eq!(
            reexecuted, retained,
            "a re-executed page is the page the replay held"
        );
    }
}

#[tokio::test]
async fn a_changed_source_or_a_lost_handle_never_answers_a_saved_page() {
    let workspace = Workspace::new();
    let file = workspace.write("saved.txt", source());
    let runtime = workspace.runtime(&env());
    let first = runtime
        .run(
            "first".into(),
            "localSearch".into(),
            json!({"queries":[{"path":file,"matchString":["needle"]}],"responseChars":600}),
        )
        .await
        .expect("first page")
        .structured_content;
    let next = first["responsePagination"]["next"]["input"].clone();
    runtime.close().await;
    evict(&workspace.home.join("tmp/response-pages"));
    std::fs::write(
        &file,
        source().replace("needle ünïcødé 😀 7 ", "needle changed 7 "),
    )
    .expect("edit source");
    let runtime = workspace.runtime(&env());
    let error = runtime
        .run("changed".into(), "localSearch".into(), next.clone())
        .await
        .expect_err("changed source");
    assert_eq!(error.code, ErrorCode::StaleSnapshot);
    runtime.close().await;
    std::fs::write(&file, source()).expect("restore source");
    // Unread for longer than the idle lifetime: the handle itself expired.
    age(&workspace.home.join("tmp/continuations"));
    let runtime = workspace.runtime(&env());
    let error = runtime
        .run("lost".into(), "localSearch".into(), next)
        .await
        .expect_err("lost handle");
    assert_eq!(error.code, ErrorCode::ResponseSnapshotExpired);
    assert!(error.message.contains("No source or provider was called"));
    runtime.close().await;
}
