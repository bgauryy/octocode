use super::*;
use serde_json::json;

/// `fields:["license"]` keeps the details-class `license` a default row
/// drops: the pick is the caller's request, not an opt-in extra.
#[test]
fn a_picked_extra_key_survives_pruning() {
    let path = ToolId::GhSearchRepo
        .extra_paths()
        .iter()
        .flat_map(|(_, paths)| paths.iter().copied())
        .find(|path| path.ends_with(".license"))
        .expect("license is an extra path");
    let picked_query = json!({"keywords":["x"],"fields":["license"]});
    assert!(picked(&picked_query, path));
    assert!(!picked(&json!({"keywords":["x"]}), path));
    let mut row = json!({"data":{"repositories":[{"owner":"o","repo":"r","license":"mit"}]}});
    prune_row(&mut row, ToolId::GhSearchRepo, &picked_query);
    assert_eq!(row["data"]["repositories"][0]["license"], "mit", "{row}");
    prune_row(&mut row, ToolId::GhSearchRepo, &json!({"keywords":["x"]}));
    assert!(
        row["data"]["repositories"][0].get("license").is_none(),
        "{row}"
    );
}
