//! Counts and repository language metadata do not establish qualifier validity.
use super::*;
use octocode_test_support::json_fixture;

#[tokio::test]
async fn equal_counts_do_not_prove_a_language_alias_was_ignored() {
    for languages in [&["TypeScript"][..], &["TypeScript", "HTML"][..]] {
        let server = MockServer::builder().start().await;
        let items = languages
            .iter()
            .enumerate()
            .map(|(index, language)| {
                let mut item = repo_item(&format!("r{index}"), false);
                item["language"] = json!(language);
                item
            })
            .collect();
        mount_json(&server, "/api/v3/search/repositories", 200, page(items)).await;
        let out = run(
            &server,
            json_fixture(
                r#"{
                    "operation": "search",
                    "owner": "o",
                    "filters": {"language": "ts"}
                }"#,
            ),
        )
        .await;
        let warnings = out.data["warnings"].to_string();
        assert!(
            !warnings.contains("GitHub ignored language"),
            "equal totals and {languages:?} cannot establish an ignored filter: {}",
            out.data
        );
        assert!(
            warnings.contains("may be unknown"),
            "ambiguity must remain visible for {languages:?}: {}",
            out.data
        );
    }
}
