//! A saved page whose response is gone and cannot be retained again serves
//! the rest of the response from that page on: the pages before it are not
//! sent twice.

use crate::support::{Run as _, Workspace};
use serde_json::{Value, json};
use std::path::Path;

fn lines(output: &Value) -> Vec<u64> {
    output["results"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|row| row["data"]["files"].as_array().into_iter().flatten())
        .flat_map(|file| file["matches"].as_array().into_iter().flatten())
        .filter_map(|hit| hit["line"].as_u64())
        .collect()
}

fn source() -> String {
    (0..120)
        .map(|index| format!("needle {index} {}\n", "long value ".repeat(index % 7)))
        .collect()
}

fn evict(dir: &Path) {
    for entry in std::fs::read_dir(dir).expect("store").flatten() {
        std::fs::remove_file(entry.path()).expect("evict entry");
    }
}

#[cfg(unix)]
#[tokio::test]
async fn a_page_that_cannot_be_retained_again_serves_from_its_own_page_to_the_end() {
    use std::os::unix::fs::PermissionsExt;
    let workspace = Workspace::new();
    let file = workspace.write("saved.txt", source());
    let store = workspace.home.join("tmp/response-pages");
    let env = [("OCTOCODE_STORAGE_MODE", "persistent".to_owned())];
    let runtime = workspace.runtime(&env);
    let first = runtime
        .run(
            "first".into(),
            "localSearch".into(),
            json!({"queries":[{"path":file,"matchString":["needle"]}],"responseChars":600}),
        )
        .await
        .expect("first page")
        .structured_content
        .into_value();
    let seen = lines(&first);
    let next = first
        .pointer("/responsePagination/next/input")
        .cloned()
        .expect("a second page");
    runtime.close().await;
    evict(&store);
    std::fs::set_permissions(&store, std::fs::Permissions::from_mode(0o500)).expect("read-only");
    let runtime = workspace.runtime(&env);
    let tail = runtime
        .run("later".into(), "localSearch".into(), next)
        .await
        .expect("the page is served")
        .structured_content
        .into_value();
    runtime.close().await;
    std::fs::set_permissions(&store, std::fs::Permissions::from_mode(0o700)).expect("restore");
    let tail_lines = lines(&tail);
    let start = seen.last().copied().expect("a first page") + 1;
    assert_eq!(
        tail_lines,
        (start..=120).collect::<Vec<_>>(),
        "no earlier hit again, none lost: {tail}"
    );
    assert!(tail.pointer("/responsePagination/next").is_none(), "{tail}");
    assert!(
        tail["warnings"].as_array().is_some_and(|w| !w.is_empty()),
        "{tail}"
    );
}
