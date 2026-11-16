//! Pull-request `next` menus: the ranked patch pick only, required defaults,
//! no cursors, no entry the row already answers. The static views are named
//! in the tool description, not on the row.
use super::*;

fn request(extra: Value) -> HistoryItemRequest {
    let mut row = json!({"operation":"pullRequest", "owner":"o","repo":"r","number":5});
    if let (Some(row), Some(extra)) = (row.as_object_mut(), extra.as_object()) {
        row.extend(extra.clone());
    }
    HistoryItemRequest::from_row(row).expect("query")
}

#[test]
fn pr_next_menu_offers_no_static_section_view() {
    let query = request(json!({"offset":100,"page":2}));
    let large = json!({"body":"x","changed_files":40,
        "additions":900,"deletions":50,"comments":2,"review_comments":1});
    let menu = pr_next_menu(&query, &[], &large);
    assert_eq!(menu.as_object().map(Map::len), Some(0), "{menu}");
    let small = json!({"body":"","changed_files":1,"additions":6,"deletions":1,"comments":3});
    let menu = pr_next_menu(&query, &["src/a.rs".into()], &small);
    assert_eq!(menu.as_object().map(Map::len), Some(0), "{menu}");
}

#[test]
fn pr_next_menu_keeps_the_ranked_patch_pick_without_cursors() {
    let query = request(json!({"sections":["files"],"offset":100,"page":2}));
    let review = vec!["src/a.rs".to_owned(), "src/b.rs".to_owned()];
    let inventory = json!({"changed_files":30,"additions":900,"deletions":10});
    let menu = pr_next_menu(&query, &review, &inventory);
    let read = &menu["readSelectedPatches"]["input"]["queries"][0];
    assert_eq!(read["sections"], json!(["patches"]));
    assert_eq!(read["include"], json!(["src/a.rs", "src/b.rs"]));
    for key in ["offset", "page"] {
        assert!(read.get(key).is_none(), "{key} leaked: {read}");
    }
    assert_eq!(menu.as_object().map(Map::len), Some(1), "{menu}");
    // A pick covering every changed file is the every-patch read, which the
    // description names: nothing is offered.
    let two = json!({"body":"","changed_files":2,"additions":900,"deletions":10});
    let menu = pr_next_menu(&query, &review, &two);
    assert_eq!(menu.as_object().map(Map::len), Some(0), "{menu}");
}
