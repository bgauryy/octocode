//! Values read as absent: empty optionals, and fields a form gives no
//! meaning. Each call replays one a model sent and a tool refused.
use super::*;

/// The call as normalized and the refusal it still draws, if any.
fn run(tool: &str, sent: Value) -> (Value, Vec<String>, Option<String>) {
    let (input, notes) = normalized_input(tool, sent);
    let refused = refusal(ToolId::from_name(tool).expect("tool"), &input);
    (input, notes, refused)
}

#[test]
fn empty_optional_refs_are_absent() {
    for (kind, target) in [
        ("commit", json!({"ref": "abc"})),
        ("pullRequest", json!({"number": 1})),
    ] {
        let mut row =
            json!({"owner": "a", "repo": "b", "operation": kind, "baseRef": "", "headRef": ""});
        row.as_object_mut()
            .expect("row")
            .extend(target.as_object().expect("target").clone());
        let (input, notes, refused) = run("ghGetHistoryItem", json!({ "queries": [row] }));
        assert_eq!(refused, None, "{kind}");
        assert!(input["queries"][0].get("baseRef").is_none(), "{input}");
        assert!(notes.is_empty(), "{notes:?}");
    }
}

#[test]
fn an_empty_value_a_form_requires_is_still_refused() {
    let sent =
        json!({"queries": [{"owner": "", "repo": "b", "operation": "commit", "ref": "abc"}]});
    let (input, _, refused) = run("ghGetHistoryItem", sent.clone());
    assert_eq!(input, sent);
    assert!(refused.expect("refused").contains("owner"));
}

#[test]
fn an_empty_object_is_not_read_into_a_call_that_bills() {
    let sent = json!({"queries": [{"resources": {}}]});
    assert_eq!(
        normalized_input("clasify", sent.clone()),
        (sent, Vec::new())
    );
}

#[test]
fn unique_outside_match_only_is_dropped_and_said() {
    let sent = json!({"queries": [{"path": ".", "matchString": ["x"],
        "contextLines": {"before": 8, "after": 16}, "unique": "list"}]});
    let (input, notes, refused) = run("localSearch", sent);
    assert_eq!(refused, None);
    assert!(input["queries"][0].get("unique").is_none());
    assert!(
        notes.iter().any(|note| note.contains("unique dropped")),
        "{notes:?}"
    );
    // With matchOnly it is the caller's and stays.
    let kept = json!({"queries": [{"path": ".", "matchString": ["x"], "view": "matchOnly", "unique": "list"}]});
    let (input, notes, refused) = run("localSearch", kept.clone());
    assert_eq!((input, notes, refused), (kept, Vec::new(), None));
}

#[test]
fn sort_on_a_tree_is_dropped_and_said() {
    let sent = json!({"queries": [{"operation": "tree", "path": ".", "sort": {"by": "name"}}]});
    let (input, notes, refused) = run("structureSearch", sent);
    assert_eq!(refused, None);
    assert!(input["queries"][0].get("sort").is_none());
    assert!(
        notes.iter().any(|note| note.contains("sort dropped")),
        "{notes:?}"
    );
}

#[test]
fn a_spelled_out_sort_direction_is_read_as_its_literal() {
    for (sent, meant) in [("descending", "desc"), ("Ascending", "asc")] {
        let row =
            json!({"operation": "files", "path": ".", "sort": {"by": "bytes", "order": sent}});
        let (input, _, refused) = run("structureSearch", json!({ "queries": [row] }));
        assert_eq!(refused, None, "{sent}");
        assert_eq!(input["queries"][0]["sort"]["order"], meant);
    }
}

#[test]
fn symbol_fuzzy_outside_workspace_symbol_names_the_anchor_form() {
    let sent = json!({"queries": [{"operation": "references", "symbolFuzzy": "callSmallModel"}]});
    let (_, _, refused) = run("lspSearch", sent);
    let refused = refused.expect("refused");
    assert!(
        refused.contains("symbolName") && refused.contains("path"),
        "{refused}"
    );
}
