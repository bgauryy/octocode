//! Tests of the read lead a listing ranks first.
use super::*;

fn lead(paths: &[&str], include: &[&str]) -> Option<String> {
    let query: GhStructureQueryTree = serde_json::from_value(json!({"operation":"tree",
        "owner": "o", "repo": "r", "include": include
    }))
    .expect("query");
    let entries = paths
        .iter()
        .map(|path| TreeEntry {
            path: (*path).to_owned(),
            kind: EntryKind::File,
            size: Some(1),
        })
        .collect::<Vec<_>>();
    entry_read(&query, &entries, "s").map(|read| {
        read["input"]["queries"][0]["path"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    })
}

/// The read lead lands on source: the file a bare-word `include`
/// names over its test, and a code entry over README and other files.
#[test]
fn the_read_lead_prefers_source_over_tests_and_docs() {
    assert_eq!(
        lead(
            &["__tests__/X-test.js", "client/X.js", "client/XFB.js"],
            &["X"]
        )
        .as_deref(),
        Some("client/X.js")
    );
    assert_eq!(
        lead(&["README.md", "benches/b.rs", "src/lib.rs"], &[]).as_deref(),
        Some("src/lib.rs")
    );
    // Without source, a manifest before a README.
    assert_eq!(
        lead(&["README.md", "Cargo.toml"], &[]).as_deref(),
        Some("Cargo.toml")
    );
    // A root with loose scripts reads its manifest, not a script (zod's
    // `mm.mjs`); a nested README does not outrank source.
    assert_eq!(
        lead(&["README.md", "AGENTS.md", "mm.mjs", "package.json"], &[]).as_deref(),
        Some("package.json")
    );
    assert_eq!(
        lead(&["README.md", "download-latest.sh"], &[]).as_deref(),
        Some("README.md")
    );
    assert_eq!(
        lead(&["examples/a/README.md", "src/util.ts"], &[]).as_deref(),
        Some("src/util.ts")
    );
    // A large listing never leads to a file no tier names.
    let data = (0..8).map(|n| format!("data/{n}.csv")).collect::<Vec<_>>();
    let data = data.iter().map(String::as_str).collect::<Vec<_>>();
    assert_eq!(lead(&data, &[]), None);
    assert_eq!(lead(&data[..2], &[]).as_deref(), Some("data/0.csv"));
}
