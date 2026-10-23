//! Pull-request `next` menus: required defaults, no cursors, no entry the row already answers, at most two entries.
use super::*;

#[test]
fn pr_next_menu_carries_required_defaults_and_drops_cursors() {
    let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
        "operation":"pullRequest","mainGoal": "test", "reasoning":"test","owner":"o","repo":"r","number":5,
        "sections":["body"],"offset":100,"page":2,
        "reasoning":"r"
    }))
    .expect("query");
    let menu = pr_next_menu(&query, &["src/a.rs".into()], &json!({}));
    let reviews = &menu["readFiles"]["query"]["queries"][0];
    // pageSize has no contract default; an omitted one stays omitted.
    assert!(reviews.get("pageSize").is_none(), "{reviews}");
    // Menu reads use the flat spelling; text is verbatim, so no `minify`.
    assert!(reviews.get("minify").is_none(), "{reviews}");
    assert_eq!(reviews["sections"], json!(["files"]));
    assert!(reviews.get("content").is_none(), "{reviews}");
    for key in ["offset", "page"] {
        assert!(reviews.get(key).is_none(), "{key} leaked: {reviews}");
    }
    assert_eq!(reviews["mainGoal"], "test");
    assert_eq!(reviews["reasoning"], "r");
    assert!(menu.get("readBody").is_none());
}

#[test]
fn pr_next_menu_omits_entries_the_row_already_answers() {
    let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
        "operation":"pullRequest","mainGoal": "test", "reasoning":"r","owner":"o","repo":"r","number":1
    }))
    .expect("query");
    let names = |menu: &Value| {
        menu.as_object()
            .map(|m| m.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default()
    };
    // Empty body, one small file, comments present: every patch reads in
    // one call, so no separate file list.
    let small = json!({"body":"","changed_files":1,"additions":6,"deletions":1,"comments":3});
    let menu = pr_next_menu(&query, &[], &small);
    assert_eq!(names(&menu), ["readPatches", "readDiscussion"]);
    assert_eq!(
        menu["readPatches"]["query"]["queries"][0]["sections"],
        json!(["patches"])
    );
    // A body rides the file list of a large diff.
    let large = json!({"body":"x","changed_files":40,
        "additions":900,"deletions":50,"comments":0,"review_comments":0});
    let menu = pr_next_menu(&query, &[], &large);
    assert_eq!(names(&menu), ["readFiles", "readDiscussion"]);
    assert_eq!(
        menu["readFiles"]["query"]["queries"][0]["sections"],
        json!(["body", "files"])
    );
    // Provably no comments: the discussion read asks for reviews only.
    assert_eq!(
        menu["readDiscussion"]["query"]["queries"][0]["sections"],
        json!(["reviews"])
    );
    // No changed files and a body: the body read stands alone.
    let empty = json!({"body":"y","changed_files":0,
        "comments":0,"review_comments":0});
    assert_eq!(
        names(&pr_next_menu(&query, &[], &empty)),
        ["readBody", "readDiscussion"]
    );
}

/// A menu holds at most [`MENU_CAP`] entries; an inventory read turns its
/// review pick into `readSelectedPatches` over several files.
#[test]
fn menus_hold_at_most_two_entries_and_inventory_reviews_many_files() {
    let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
        "operation":"pullRequest","mainGoal": "test", "reasoning":"r","owner":"o","repo":"r","number":1
    }))
    .expect("query");
    let merged = json!({"body":"x".repeat(900),"changed_files":656,"additions":282_700,
        "deletions":284_842,"comments":5,"merged_at":"2026-01-01T00:00:00Z","merge_commit_sha":"abc"});
    let menu = pr_next_menu(&query, &[], &merged);
    let keys = menu
        .as_object()
        .map(|m| m.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    assert_eq!(keys, ["readFiles", "readDiscussion"], "{menu}");
    crate::tools::test_support::checks::assert_absent(&menu, "<literal");
    let review = vec!["src/a.rs".to_owned(), "src/b.rs".to_owned()];
    let inventory = json!({"changed_files":30,"additions":900,"deletions":10});
    let inventory_read: HistoryItemRequest = HistoryItemRequest::from_row(json!({
        "operation":"pullRequest","mainGoal": "test", "reasoning":"r","owner":"o","repo":"r","number":1,
        "sections":["files"]
    }))
    .expect("inventory query");
    let menu = pr_next_menu(&inventory_read, &review, &inventory);
    assert_eq!(
        menu["readSelectedPatches"]["query"]["queries"][0]["sections"],
        json!(["patches"])
    );
    assert_eq!(
        menu["readSelectedPatches"]["query"]["queries"][0]["include"],
        json!(["src/a.rs", "src/b.rs"])
    );
    assert_eq!(menu["readSelectedPatches"]["confidence"], "high");
    assert!(menu.get("readPatches").is_some(), "{menu}");
    // The discussion read is the third candidate: it stays reachable
    // through `include`, not the menu.
    assert_eq!(menu.as_object().map(Map::len), Some(MENU_CAP), "{menu}");
    assert!(menu.get("readDiscussion").is_none(), "{menu}");
    // A pick covering every changed file is the every-patch read.
    let two = json!({"body":"","changed_files":2,"additions":900,"deletions":10});
    let menu = pr_next_menu(&inventory_read, &review, &two);
    assert_eq!(
        menu["readPatches"]["query"]["queries"][0]["sections"],
        json!(["patches"]),
        "{menu}"
    );
    assert!(
        menu["readPatches"]["query"]["queries"][0]
            .get("include")
            .is_none(),
        "{menu}"
    );
}
