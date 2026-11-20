//! An `owner` or `repo` spelt as a GitHub location runs as its owner and
//! repository names, and the note says the fields to send; a location
//! naming more than the two fields hold, or another row's names, stays a
//! refusal.
use super::*;

/// The row a GitHub `owner`/`repo` pair ran with, and its one note.
fn ran(sent: Value) -> (Value, String) {
    let ran = accepted("ghGetFileContent", sent.clone());
    let notes = normalized_input("ghGetFileContent", sent.clone()).1;
    let [note] = notes.as_slice() else {
        panic!("{sent}: one note, got {notes:?}");
    };
    (ran["queries"][0].clone(), note.clone())
}

fn row(fields: Value) -> Value {
    let mut row = json!({"path": "README.md"});
    row.as_object_mut()
        .unwrap_or_else(|| unreachable!())
        .extend(fields.as_object().cloned().unwrap_or_default());
    json!({ "queries": [row] })
}

#[test]
fn a_github_location_in_owner_or_repo_runs_as_its_names() {
    for fields in [
        json!({"owner": "sharkdp/bat"}),
        json!({"owner": "sharkdp/bat", "repo": "bat"}),
        json!({"owner": "sharkdp/bat", "repo": "sharkdp/bat"}),
        json!({"owner": "github.com/sharkdp", "repo": "bat"}),
        json!({"owner": "https://github.com/sharkdp/bat"}),
        json!({"owner": "https://www.GitHub.com/sharkdp/bat.git"}),
        json!({"owner": "github.com/sharkdp/bat/"}),
        json!({"owner": "sharkdp", "repo": "sharkdp/bat"}),
        json!({"owner": "sharkdp", "repo": "https://github.com/sharkdp/bat"}),
    ] {
        let (ran, note) = ran(row(fields.clone()));
        assert_eq!(ran["owner"], "sharkdp", "{fields}: {ran}");
        assert_eq!(ran["repo"], "bat", "{fields}: {ran}");
        assert!(note.contains("`owner: \"sharkdp\"`"), "{fields}: {note}");
    }
}

#[test]
fn a_location_with_another_reading_stays_rejected() {
    for fields in [
        // A path past the repository names more than the two fields.
        json!({"owner": "github.com/sharkdp/bat/tree/main"}),
        // The location names another repository than the row's own.
        json!({"owner": "sharkdp/bat", "repo": "fd"}),
        json!({"owner": "sharkdp", "repo": "BurntSushi/ripgrep"}),
        // Not spelt as names at all.
        json!({"owner": "a b", "repo": "bat"}),
        json!({"owner": "https://gitlab.com/sharkdp/bat"}),
    ] {
        let sent = row(fields.clone());
        assert!(
            prepare_rows(
                "ghGetFileContent",
                normalized_input("ghGetFileContent", sent).0
            )
            .map_or(true, |rows| rows.iter().any(Result::is_err)),
            "{fields} should stay rejected"
        );
    }
}
