//! `unique: "countAll"` (forced through `test_count_all` until core declares
//! it): one tally of the verified, redacted values over every file.
use super::*;
use crate::tools::local_search::tally::{UniqueValue, test_count_all};

/// Writes `files` under a fresh workspace and searches it with `fields`.
fn search_files(files: &[(&str, &str)], fields: serde_json::Value) -> LocalSearchResult {
    let dir = crate::tools::test_support::Fixture::new();
    let root = dir.0.as_path().canonicalize().expect("root");
    for (name, body) in files {
        let path = root.join(name);
        fs::create_dir_all(path.parent().expect("parent")).expect("fixture dir");
        fs::write(path, body).expect("fixture");
    }
    search_root(&root, fields)
}

fn value(value: &str, count: u32, files: u32) -> UniqueValue {
    UniqueValue {
        value: value.to_owned(),
        count,
        files,
    }
}

/// An activation event as a quoted value (a key such as `activationEvents`
/// holds `on…` inside it, never after its opening quote).
const EVENTS: &str = r#""on[A-Za-z]+(:[^"]*)?""#;

/// Activation events of three manifests: a multi-line array, a one-line
/// array, and one naming a value twice; an `on…` value outside the key path
/// is not tallied.
const MANIFESTS: [(&str, &str); 3] = [
    (
        "a/package.json",
        r#"{
  "activationEvents": [
    "onLanguage:json",
    "onCommand:a.run"
  ],
  "contributes": { "commands": [{ "command": "onCommand:fake" }] }
}
"#,
    ),
    (
        "b/package.json",
        r#"{
  "activationEvents": ["onLanguage:json", "onStartupFinished"],
  "main": "./out/onLoad.js"
}
"#,
    ),
    (
        "c/package.json",
        r#"{
  "activationEvents": [
    "onLanguage:json",
    "onLanguage:json"
  ]
}
"#,
    ),
];

/// The tally names each value once with its occurrences and files, most
/// frequent first, and is the whole page: no file rows, no pages.
#[test]
fn a_tally_counts_each_value_over_every_file() {
    let fields = serde_json::json!({
        "view": "matchOnly", "matchString": [EVENTS], "regexEngine": "rust",
        "unique": "count", "keyPath": "activationEvents",
    });
    let per_file = search_files(&MANIFESTS, fields.clone());
    assert!(per_file.unique_values.is_none(), "count stays per file");
    assert_eq!(per_file.files.len(), 3);

    let _count_all = test_count_all();
    let tally = search_files(&MANIFESTS, fields);
    assert_eq!(
        tally.unique_values,
        Some(vec![
            value(r#""onLanguage:json""#, 4, 3),
            value(r#""onCommand:a.run""#, 1, 1),
            value(r#""onStartupFinished""#, 1, 1),
        ])
    );
    assert!(tally.files.is_empty(), "{:?}", tally.files);
    assert_eq!(tally.status, SearchStatus::Success);
    assert_eq!(tally.stats.total_occurrences, Some(6));
    assert_eq!(tally.stats.matched_files, Some(3));
    assert!(tally.next.is_none(), "{:?}", tally.next);
    assert!(tally.pagination.is_none());
    assert!(tally.warnings.is_empty(), "{:?}", tally.warnings);
}

/// A secret is never matched or tallied as its bytes: the scan searches the
/// text a read serves, so a value holding one tallies as that read shows
/// it, and the row says values were redacted.
#[test]
fn a_tally_counts_redacted_values_as_a_read_shows_them() {
    let first = format!("ghp_{}", "a1B2c3D4".repeat(5));
    let second = format!("ghp_{}", "Z9y8X7w6".repeat(5));
    let bodies = [
        ("one.txt", format!("token {first}\n")),
        ("two.txt", format!("token {second}\ntoken {first}\n")),
    ];
    let files = bodies
        .iter()
        .map(|(name, body)| (*name, body.as_str()))
        .collect::<Vec<_>>();
    let fields = serde_json::json!({
        "view": "matchOnly", "matchString": ["token \\S+"],
        "regexEngine": "rust", "unique": "count",
    });
    let per_file = search_files(&files, fields.clone());
    let _count_all = test_count_all();
    let tally = search_files(&files, fields);
    let row = tally.to_row().expect("row").to_string();
    assert!(!row.contains(&first) && !row.contains(&second), "{row}");
    let values = tally.unique_values.clone().expect("a tally");
    assert!(
        values.iter().all(|value| value.value.contains("[REDACTED")),
        "{values:?}"
    );
    // The tally holds what the per-file rows show, summed.
    let mut summed = std::collections::BTreeMap::<String, (u32, Vec<&str>)>::new();
    for file in &per_file.files {
        for hit in file.matches.iter().flatten() {
            let entry = summed.entry(hit.value.clone()).or_default();
            entry.0 += hit.count.unwrap_or(1);
            if !entry.1.contains(&file.path.as_str()) {
                entry.1.push(&file.path);
            }
        }
    }
    let shown = summed
        .into_iter()
        .map(|(value, (count, files))| (value, (count, u32::try_from(files.len()).expect("files"))))
        .collect::<std::collections::BTreeMap<_, _>>();
    let tallied = values
        .iter()
        .map(|value| (value.value.clone(), (value.count, value.files)))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(tallied, shown);
    assert_eq!(values.iter().map(|value| value.count).sum::<u32>(), 3);
    assert!(
        tally
            .warnings
            .iter()
            .any(|warning| warning.starts_with(&format!("redactedMatches: {} ", values.len()))),
        "{:?}",
        tally.warnings
    );
}

/// A key path inside one-line objects keeps their lines (no false empty),
/// and a line shared with members outside the key path is disclosed.
#[test]
fn a_key_path_inside_one_line_objects_keeps_and_discloses_their_lines() {
    let manifest = r#"{
  "contributes": {
    "commands": [
      { "command": "ext.run", "title": "Run" },
      { "command": "ext.stop", "title": "Stop" }
    ]
  }
}
"#;
    let titles = search_files(
        &[("package.json", manifest)],
        serde_json::json!({
            "view": "matchOnly", "matchString": ["\"[A-Z][a-z]+\""], "regexEngine": "rust",
            "keyPath": "contributes.commands[*].title",
        }),
    );
    let shown = values(&titles);
    assert!(shown.contains("Run") && shown.contains("Stop"), "{shown}");
    assert!(
        titles.warnings.iter().any(|warning| warning.starts_with(
            "keyPath contributes.commands[*].title: 2 hit(s) on 2 line(s) in 1 file(s)"
        )),
        "{:?}",
        titles.warnings
    );
    // A one-line array whose items are all inside the key path is exact.
    let events = search_files(
        &[MANIFESTS[1]],
        serde_json::json!({
            "view": "matchOnly", "matchString": [EVENTS], "regexEngine": "rust",
            "keyPath": "activationEvents",
        }),
    );
    assert!(events.warnings.is_empty(), "{:?}", events.warnings);
    let shown = values(&events);
    assert!(!shown.contains("onLoad"), "{shown}");
}
