//! The recordings themselves: nothing in them looks like a credential.
use super::store;
use serde_json::Value;
use std::path::PathBuf;

fn every_recording() -> Vec<PathBuf> {
    let providers = std::fs::read_dir(store()).expect("the store");
    let mut files = Vec::new();
    for provider in providers {
        let dir = provider.expect("a provider").path();
        if !dir.is_dir() {
            continue;
        }
        for file in std::fs::read_dir(dir).expect("a provider's recordings") {
            files.push(file.expect("a recording").path());
        }
    }
    files
}

fn strings<'a>(value: &'a Value, found: &mut Vec<&'a str>) {
    match value {
        Value::String(text) => found.push(text),
        Value::Array(items) => items.iter().for_each(|item| strings(item, found)),
        Value::Object(fields) => fields.values().for_each(|item| strings(item, found)),
        _ => {}
    }
}

/// Checked in, so nothing in them may look like a credential to the detector
/// every tool output passes through, whichever file a read would name.
#[test]
fn no_recording_holds_what_the_secret_detector_redacts() {
    let security = octocode_runtime_base::security::ContentSecurity::new();
    let files = every_recording();
    assert!(!files.is_empty(), "the store is empty");
    for file in files {
        let text = std::fs::read_to_string(&file).expect("a recording");
        let recording: Value = serde_json::from_str(&text).expect("recording JSON");
        let mut leaves = Vec::new();
        strings(&recording, &mut leaves);
        for leaf in leaves {
            if let Some(redacted) = security.sanitize_stored_leaf(leaf, None) {
                let cut = leaf
                    .char_indices()
                    .nth(120)
                    .map_or(leaf.len(), |(at, _)| at);
                panic!(
                    "{}: the detector redacts `{}`, as `{}`",
                    file.display(),
                    &leaf[..cut],
                    redacted.chars().take(120).collect::<String>()
                );
            }
        }
    }
}
