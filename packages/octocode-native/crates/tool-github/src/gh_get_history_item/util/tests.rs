use octocode_test_support::json_fixture;

/// A comment without an edit time (GraphQL discussion comments) carries
/// no `updatedAt`, so the latest comment time falls back to createdAt.
#[test]
fn comments_without_an_edit_time_carry_no_empty_updated_at() {
    let rows = super::map_comments(vec![json_fixture(
        r#"{
                "id": 1,
                "user": {"login": "dev"},
                "body": "b",
                "created_at": "2026-06-03T00:00:00Z",
                "updated_at": null
            }"#,
    )]);
    assert!(rows[0].get("updatedAt").is_none(), "{}", rows[0]);
    assert_eq!(rows[0]["createdAt"], "2026-06-03T00:00:00Z");
}

/// A GitHub App or bot account's comment is marked `bot` (a `Bot` user,
/// or a login ending in `[bot]`), and every comment keeps GitHub's
/// order: the timeline is evidence.
#[test]
fn bot_comments_are_marked_in_github_order() {
    let comment = |id: u64, login: &str, kind: &str| {
        serde_json::json!({"id": id, "user": {"login": login, "type": kind},
                "body": "b", "created_at": "2026-06-03T00:00:00Z"})
    };
    let rows = super::map_comments(vec![
        comment(1, "changeset-bot[bot]", "Bot"),
        comment(2, "ada", "User"),
        comment(3, "pkg-pr-new", "Bot"),
        comment(4, "bob", "User"),
        comment(5, "renovate[bot]", "User"),
    ]);
    let order = rows
        .iter()
        .map(|row| (row["id"].as_str().unwrap_or(""), row.get("bot").is_some()))
        .collect::<Vec<_>>();
    assert_eq!(
        order,
        [
            ("1", true),
            ("2", false),
            ("3", true),
            ("4", false),
            ("5", true)
        ]
    );
    assert_eq!(rows[0]["bot"], true);
}
