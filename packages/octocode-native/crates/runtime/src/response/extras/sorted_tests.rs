use super::*;
use serde_json::json;

/// Rows sorted by an extra field show it by default (L6-8): the
/// comment count of a `comments:desc` search, the forks and update date
/// of a repository search sorted by them. Another sort drops them.
#[test]
fn the_field_rows_are_sorted_by_survives_pruning() {
    let issue = || json!({"data":{"issues":[{"number":1,"commentsCount":40}]}});
    let mut row = issue();
    prune_row(
        &mut row,
        ToolId::GhSearchHistory,
        &json!({"sort":"comments:desc"}),
    );
    assert_eq!(row["data"]["issues"][0]["commentsCount"], 40, "{row}");
    let mut row = issue();
    prune_row(
        &mut row,
        ToolId::GhSearchHistory,
        &json!({"sort":"created:desc"}),
    );
    assert!(
        row["data"]["issues"][0].get("commentsCount").is_none(),
        "{row}"
    );

    let repo =
        || json!({"data":{"repositories":[{"owner":"o","repo":"r","forks":9,"updatedAt":"t"}]}});
    let mut row = repo();
    prune_row(
        &mut row,
        ToolId::GhSearchRepo,
        &json!({"sort":"forks:desc"}),
    );
    assert_eq!(row["data"]["repositories"][0]["forks"], 9, "{row}");
    assert!(
        row["data"]["repositories"][0].get("updatedAt").is_none(),
        "{row}"
    );
    let mut row = repo();
    prune_row(
        &mut row,
        ToolId::GhSearchRepo,
        &json!({"sort":"updated:desc"}),
    );
    assert_eq!(row["data"]["repositories"][0]["updatedAt"], "t", "{row}");
    assert!(keeps(
        ToolId::GhSearchRepo,
        &json!({"sort":"updated:desc"}),
        "results[].data.repositories[].updatedAt"
    ));
}
