//! localFetch page shape, served: gap rows, compact notes, binary refusals,
//! long-line continuations.

use crate::support::{Run as _, Workspace, clasify_runtime, next_input};
use octocode_native::runtime::ToolRuntime;
use serde_json::{Value, json};

fn lines(count: usize) -> String {
    (1..=count).map(|line| format!("row {line}\n")).collect()
}

/// Every page of a localFetch walk, as served: each row's `data`.
async fn walk_pages(runtime: &ToolRuntime, start: Value) -> Vec<Value> {
    let mut pages = Vec::new();
    let mut input = start;
    loop {
        assert!(pages.len() < 100, "runaway paging");
        let output = runtime
            .run("view".into(), "localFetch".into(), input)
            .await
            .expect("localFetch")
            .structured_content
            .into_value();
        pages.push(output["results"][0]["data"].clone());
        match next_input(&output) {
            Some(next) => input = next,
            None => return pages,
        }
    }
}

fn shown_rows(page: &Value) -> Vec<&str> {
    page["content"]
        .as_str()
        .unwrap_or_default()
        .lines()
        .filter(|line| *line != "--")
        .collect()
}

#[tokio::test]
async fn gap_rows_are_never_counted_and_one_form_marks_them() {
    let workspace = Workspace::new();
    let path = workspace.write("gaps.txt", lines(30));
    let runtime = workspace.runtime(&[]);
    let ranges = json!([
        {"startLine": 1, "endLine": 3},
        {"startLine": 10, "endLine": 12},
        {"startLine": 20, "endLine": 22}
    ]);
    let whole = walk_pages(
        &runtime,
        json!({"queries":[{"path": path, "ranges": ranges}]}),
    )
    .await;
    assert_eq!(whole.len(), 1, "{whole:?}");
    let single: Vec<String> = shown_rows(&whole[0])
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(single.len(), 9, "{single:?}");
    for limit in 1..=5 {
        let pages = walk_pages(
            &runtime,
            json!({"queries":[{"path": path, "ranges": ranges, "limit": limit}]}),
        )
        .await;
        let mut joined = Vec::new();
        for page in &pages {
            let content = page["content"].as_str().expect("content");
            assert!(
                !content.contains("not requested"),
                "limit {limit}: {content}"
            );
            assert!(
                !content.starts_with("--") && !content.trim_end().ends_with("--"),
                "limit {limit}: a page does not open or close on a gap: {content:?}"
            );
            let shown = shown_rows(page);
            assert_eq!(
                shown.len() as u64,
                page["pagination"]["length"].as_u64().expect("length"),
                "limit {limit}: length counts the shown source rows: {page}"
            );
            assert_eq!(
                page["pagination"]["total"].as_u64(),
                Some(9),
                "limit {limit}: total counts source rows only: {page}"
            );
            joined.extend(shown.iter().map(ToString::to_string));
        }
        assert_eq!(joined, single, "limit {limit}");
    }
    runtime.close().await;
}

async fn compact_row(runtime: &ToolRuntime, path: &std::path::Path) -> Value {
    runtime
        .run(
            "compact".into(),
            "localFetch".into(),
            json!({"queries":[{"path": path, "view": "compact"}]}),
        )
        .await
        .expect("localFetch")
        .structured_content
        .into_value()["results"][0]["data"]
        .clone()
}

fn size_notes(data: &Value) -> usize {
    data["warnings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|warning| warning.starts_with("compact view:"))
        .count()
}

#[tokio::test]
async fn a_format_served_as_is_carries_no_compact_size_note() {
    let workspace = Workspace::new();
    let markdown = workspace.write("doc.md", "# Title\n\nSome prose here.\n");
    let code = workspace.write(
        "code.rs",
        "// a comment\n\n\n/// docs\nfn main() {\n    let value = 1;\n\n    // more\n    println!(\"{value}\");\n}\n",
    );
    let runtime = workspace.runtime(&[]);
    let md = compact_row(&runtime, &markdown).await;
    assert_eq!(md["viewFallback"]["applied"], "content", "{md}");
    assert_eq!(size_notes(&md), 0, "{md}");
    let rs = compact_row(&runtime, &code).await;
    assert_eq!(size_notes(&rs), 1, "{rs}");
    runtime.close().await;
}

/// A 1x1 PNG header: enough for the image check, binary by its NUL bytes.
fn tiny_png() -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    bytes.extend([
        0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0x1f, 0x15, 0xc4, 0x89,
    ]);
    bytes
}

fn hints_of(output: &Value) -> String {
    output["results"][0]["data"]["hints"].to_string()
}

async fn fetch(runtime: &ToolRuntime, path: &std::path::Path) -> Value {
    runtime
        .run(
            "binary".into(),
            "localFetch".into(),
            json!({"queries":[{"path": path}]}),
        )
        .await
        .expect("localFetch")
        .structured_content
        .into_value()
}

#[tokio::test]
async fn an_image_refusal_leads_to_clasify_only_when_clasify_is_configured() {
    let workspace = Workspace::new();
    let png = workspace.write("logo.png", tiny_png());
    let blob = workspace.write("blob.bin", b"\x00\x01\x02binary\x00".as_slice());
    let server = wiremock::MockServer::start().await;
    let configured = clasify_runtime(&workspace, &server, &[]);
    let output = fetch(&configured, &png).await;
    assert_eq!(
        output["results"][0]["data"]["errorCode"],
        "binaryFileUnsupported"
    );
    let hints = hints_of(&output);
    // The route is the image as a clasify source: one call, no assets map.
    assert!(
        hints.contains("clasify") && hints.contains("source") && !hints.contains("assets"),
        "{hints}"
    );
    assert!(hints.contains(&png.display().to_string()), "{hints}");
    let other = hints_of(&fetch(&configured, &blob).await);
    assert!(!other.contains("clasify"), "{other}");
    configured.close().await;
    let plain = workspace.runtime(&[]);
    let without = hints_of(&fetch(&plain, &png).await);
    assert!(!without.contains("clasify"), "{without}");
    plain.close().await;
}

const FAKE_TOKEN: &str = "ghp_aB3dE6gH9jK2mN5pQ8sT1vW4yZ7cF0hJ3kLm";

#[tokio::test]
async fn a_credential_shaped_caller_path_is_not_echoed_by_an_error_row() {
    let workspace = Workspace::new();
    workspace.write(&format!("deploy-{FAKE_TOKEN}.txt"), "x\n");
    let runtime = workspace.runtime(&[]);
    let outside = workspace
        .workspace
        .parent()
        .expect("parent")
        .join(format!("elsewhere-{FAKE_TOKEN}.txt"));
    for path in [
        workspace
            .workspace
            .join(format!("missing-{FAKE_TOKEN}.txt")),
        outside,
    ] {
        let output = fetch(&runtime, &path).await;
        let row = &output["results"][0];
        let text = row.to_string();
        assert!(!text.contains(FAKE_TOKEN), "{text}");
        assert_eq!(row["data"]["redacted"], true, "{text}");
        let leads = row["data"]["related"].to_string();
        assert!(
            !leads.contains("[REDACTED"),
            "a lead holds a placeholder glob: {leads}"
        );
    }
    runtime.close().await;
}
