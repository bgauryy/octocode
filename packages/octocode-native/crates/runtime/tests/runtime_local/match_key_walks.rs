//! `matchKeys` key paths page with their file's hits: whatever window a page
//! is cut to, a walk of its continuations reaches every hit and every key
//! path exactly once, and no page drops a key path to stay in its window.

use crate::support::{self, Workspace};
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// Hits per fixture file: enough that a tiny window cuts the file many times.
const HITS: usize = 150;

/// A JSON document with one hit per line under `section`: Unicode names, and
/// a long one whose key path alone outgrows a tiny window.
fn json_document(section: &str) -> (String, Vec<(usize, String)>) {
    let mut text = String::from("{\n");
    let mut keys = Vec::new();
    text.push_str(&format!("  \"{section}\": {{\n"));
    for index in 0..HITS {
        let name = if index % 7 == 0 {
            format!("{}{index}", "é".repeat(180))
        } else {
            format!("ключ{index}_λ🦀")
        };
        let comma = if index + 1 < HITS { "," } else { "" };
        text.push_str(&format!("    \"{name}\": \"needle {index}\"{comma}\n"));
        keys.push((index + 3, format!("{section}.{name}")));
    }
    text.push_str("  }\n}\n");
    (text, keys)
}

/// `path:line` of each hit and `path:line=keyPath` of each key path on a page.
fn page_rows(output: &Value) -> Vec<String> {
    assert!(
        !output.to_string().contains("matchKeys) omitted"),
        "a page dropped key paths to keep its size: {output}"
    );
    let mut rows = Vec::new();
    let files = output["results"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|row| row.pointer("/data/files").and_then(Value::as_array))
        .flatten();
    for file in files {
        let path = file["path"].as_str().expect("file path");
        for hit in file["matches"].as_array().into_iter().flatten() {
            rows.push(format!("{path}:{}", hit["line"]));
        }
        for key in file["matchKeys"].as_array().into_iter().flatten() {
            let key_path = key["keyPath"].as_str().expect("keyPath");
            rows.push(format!("{path}:{}={key_path}", key["line"]));
        }
    }
    rows
}

#[tokio::test]
async fn key_paths_are_reachable_exactly_once_at_every_window() {
    let workspace = Workspace::new();
    let mut expected = BTreeSet::new();
    for (name, section) in [("a.json", "первый"), ("b.json", "second")] {
        let (text, keys) = json_document(section);
        workspace.write(name, text);
        for (line, key_path) in keys {
            expected.insert(format!("{name}:{line}"));
            expected.insert(format!("{name}:{line}={key_path}"));
        }
    }
    let runtime = workspace.runtime(&[]);
    for window in [300, 900, 2500, 9000] {
        let seed = json!({"queries":[{"path":".","matchString":["needle"],"include":["*.json"],
            "sort":{"by":"path","order":"asc"}}],"responseChars":window});
        let walk = support::walk(&runtime, "localSearch", support::envelope(seed), page_rows).await;
        assert!(walk.queries.len() > 1, "window {window} must page");
        let walked = walk.rows.into_iter().collect::<BTreeSet<_>>();
        assert_eq!(walked, expected, "window {window}");
    }
    runtime.close().await;
}

/// A manifest's key paths read as they are (B152): `serde` and `serde_json`
/// are dependency names, not credentials, whatever the field that carries
/// them is called. A secret-shaped name inside a key path stays hidden.
#[tokio::test]
async fn key_paths_read_plainly_and_a_secret_shaped_name_in_one_is_hidden() {
    let workspace = Workspace::new();
    workspace.write(
        "Cargo.toml",
        "[dependencies]\nserde = { version = \"1\" }\nserde_json = \"1\"\n[profile.dev.package.\"serde_json\"]\nopt-level = 3\n",
    );
    let token = format!("ghp_{}", "a".repeat(36));
    workspace.write(
        "tokens.json",
        format!("{{\n  \"services\": {{\n    \"{token}\": \"needle\"\n  }}\n}}\n"),
    );
    let runtime = workspace.runtime(&[]);
    let manifest = support::run(
        &runtime,
        "localSearch",
        json!({"queries":[{"path":"Cargo.toml","matchString":["serde"]}]}),
    )
    .await;
    let rows = page_rows(&manifest.structured_content.clone().into_value());
    for expected in [
        "Cargo.toml:2=dependencies.serde",
        "Cargo.toml:3=dependencies.serde_json",
    ] {
        assert!(
            rows.iter().any(|row| row == expected),
            "{expected}: {rows:?}"
        );
    }
    let tokens = support::run(
        &runtime,
        "localSearch",
        json!({"queries":[{"path":"tokens.json","matchString":["needle"]}]}),
    )
    .await;
    let shown = tokens.structured_content.to_string();
    assert!(!shown.contains(&token), "{shown}");
    let rows = page_rows(&tokens.structured_content.clone().into_value());
    assert!(
        rows.iter()
            .any(|row| row.starts_with("tokens.json:3=services.[REDACTED")),
        "{rows:?}"
    );
    runtime.close().await;
}
