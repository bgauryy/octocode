use super::*;
use octocode_test_support::json_fixture;

#[test]
fn fresh_symbol_filter_leads_drop_all_saved_progress_and_keep_scope() {
    let query: AstSearchQuerySymbols = serde_json::from_value(json_fixture(
        r#"{
                "operation": "symbols",
                "path": "src",
                "page": 3,
                "snapshot": "previous",
                "scanOffset": 17,
                "symbolFilter": {"terms": ["Widget"], "mode": "exact"},
                "symbolKinds": ["function"],
                "includeHidden": true,
                "exclude": ["vendor/**"]
            }"#,
    ))
    .expect("saved symbols query");
    let kinds = KindsLeftOut {
        names: vec!["Widget".into()],
        count: 1,
        kinds: vec!["class".into()],
    };
    let leads = [Suppressed::lead(&query), kinds.lead(&query)];
    for lead in &leads {
        assert_eq!(lead["kind"], "related");
        octocode_runtime_base::contracts::validate("astSearch", lead["input"].clone())
            .expect("authored fresh request");
        let row = &lead["input"]["queries"][0];
        for field in octocode_runtime_base::contracts::state_fields(ToolId::AstSearch) {
            assert!(row.get(field).is_none(), "{field}: {lead}");
        }
        assert_eq!(row["path"], "src");
        assert_eq!(row["exclude"], json_fixture(r#"["vendor/**"]"#));
        assert_eq!(row["includeHidden"], true);
        assert_eq!(row["symbolFilter"]["terms"], json_fixture(r#"["Widget"]"#));
    }
    assert_eq!(
        leads[0]["input"]["queries"][0]["symbolFilter"]["mode"],
        "substring"
    );
    assert_eq!(
        leads[1]["input"]["queries"][0]["symbolKinds"],
        json_fixture(r#"["function", "class"]"#)
    );
}
