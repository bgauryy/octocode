use octocode_contract_types::GhSearchRepoQuery;
use serde_json::json;

#[test]
fn repository_operations_keep_their_own_sort_types() {
    for (operation, by) in [("search", "stars"), ("list", "pushedAt")] {
        let input =
            json!({"operation":operation,"owner":"octocat","sort":{"by":by,"order":"desc"}});
        let decoded: GhSearchRepoQuery =
            serde_json::from_value(input.clone()).expect("canonical operation sort");
        let encoded = serde_json::to_value(decoded).expect("serializable query");
        assert_eq!(encoded["sort"], input["sort"]);
        assert_eq!(encoded["operation"], operation);
    }
    for (operation, by) in [("search", "pushedAt"), ("list", "stars")] {
        assert!(
            serde_json::from_value::<GhSearchRepoQuery>(json!({
                "operation":operation,"owner":"octocat","sort":{"by":by,"order":"desc"}
            }))
            .is_err()
        );
    }
}
