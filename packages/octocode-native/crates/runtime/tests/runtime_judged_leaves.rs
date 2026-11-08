//! A localFetch page cut from a source judged whole is not judged again as
//! an isolated fragment (B165 D1), and a secret in that source stays
//! redacted in every range.
use crate::support::{Run as _, Workspace};
use serde_json::{Value, json};

const SOURCE: &str = "use serde::Serialize;\n\n#[derive(Serialize)]\nstruct SavingWalk<'a> {\n    pending: &'a [Ref],\n    token: Option<&'a str>,\n    read: &'a [Value],\n    import: &'a Option<String>,\n}\n\nfn main() {}\n";

async fn content(
    runtime: &octocode_native::runtime::ToolRuntime,
    path: &std::path::Path,
    lines: (usize, usize),
) -> String {
    let output: Value = runtime
        .run(
            "judged".into(),
            "localFetch".into(),
            json!({"queries":[{"path": path, "ranges":[{"startLine": lines.0, "endLine": lines.1}]}]}),
        )
        .await
        .expect("read")
        .structured_content
        .into_value();
    output["results"][0]["data"]["content"]
        .as_str()
        .unwrap_or_else(|| panic!("no content: {output}"))
        .to_owned()
}

#[tokio::test]
async fn ranged_reads_keep_the_whole_source_judgement() {
    let workspace = Workspace::new();
    let path = workspace.write("walk.rs", SOURCE);
    let runtime = workspace.runtime(&[]);
    for lines in [(5, 8), (1, 11), (6, 6)] {
        let text = content(&runtime, &path, lines).await;
        assert!(
            text.contains("token: Option<&'a str>,"),
            "{lines:?}: {text}"
        );
        assert!(!text.contains("[REDACTED"), "{lines:?}: {text}");
    }
    runtime.close().await;
}

/// The first string under a `cursor` key anywhere in `value`.
fn cursor_in(value: &Value) -> Option<String> {
    match value {
        Value::Object(map) => map
            .get("cursor")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| map.values().find_map(cursor_in)),
        Value::Array(items) => items.iter().find_map(cursor_in),
        _ => None,
    }
}

/// The `nextPage` continuation anywhere in `value`.
fn next_page(value: &Value) -> Option<&Value> {
    match value {
        Value::Object(map) => map
            .get("nextPage")
            .or_else(|| map.values().find_map(next_page)),
        Value::Array(items) => items.iter().find_map(next_page),
        _ => None,
    }
}

/// Every page of a whole-file read, in order, following `nextPage`.
async fn whole_read_pages(
    runtime: &octocode_native::runtime::ToolRuntime,
    path: &std::path::Path,
    limit: usize,
) -> Vec<String> {
    let mut pages = Vec::new();
    let mut input = json!({"queries":[{"path": path, "limit": limit}]});
    loop {
        let output: Value = runtime
            .run("judged".into(), "localFetch".into(), input)
            .await
            .expect("read")
            .structured_content
            .into_value();
        pages.push(
            output["results"][0]["data"]["content"]
                .as_str()
                .unwrap_or_else(|| panic!("no content: {output}"))
                .to_owned(),
        );
        let Some(cursor) = next_page(&output).and_then(cursor_in) else {
            return pages;
        };
        input = json!({ "cursor": cursor });
    }
}

/// Every page of a whole-file read is cut from the same sealed source as the
/// first, so none is judged again as a fragment (B165 round 8, M1).
#[tokio::test]
async fn every_page_of_a_whole_read_keeps_the_whole_source_judgement() {
    let workspace = Workspace::new();
    let path = workspace.write("walk.rs", SOURCE);
    let runtime = workspace.runtime(&[]);
    let pages = whole_read_pages(&runtime, &path, 5).await;
    assert!(pages.len() >= 2, "{pages:?}");
    assert!(
        pages.concat().contains("token: Option<&'a str>,"),
        "{pages:?}"
    );
    for page in &pages {
        assert!(!page.contains("[REDACTED"), "{page}");
    }
    runtime.close().await;
}

#[tokio::test]
async fn a_secret_in_the_source_stays_redacted_in_every_page_of_a_whole_read() {
    let workspace = Workspace::new();
    let planted = SOURCE.replace(
        "fn main() {}",
        "const PASSWORD: &str = \"hunter2-Xq9!zP4#wL7\";",
    );
    let path = workspace.write("walk.rs", planted);
    let runtime = workspace.runtime(&[]);
    for page in whole_read_pages(&runtime, &path, 5).await {
        assert!(!page.contains("hunter2-Xq9"), "{page}");
    }
    runtime.close().await;
}

#[tokio::test]
async fn a_secret_in_the_source_stays_redacted_in_a_ranged_read() {
    let workspace = Workspace::new();
    let token = format!("ghp_{}", "a1B2".repeat(9));
    let planted = SOURCE.replace(
        "fn main() {}",
        &format!(
            "const TOKEN: &str = \"{token}\";\nconst PASSWORD: &str = \"hunter2-Xq9!zP4#wL7\";"
        ),
    );
    let path = workspace.write("walk.rs", planted);
    let runtime = workspace.runtime(&[]);
    for lines in [(11, 12), (5, 12), (12, 12)] {
        let text = content(&runtime, &path, lines).await;
        assert!(!text.contains(&token), "{lines:?}: {text}");
        assert!(!text.contains("hunter2-Xq9"), "{lines:?}: {text}");
    }
    runtime.close().await;
}
