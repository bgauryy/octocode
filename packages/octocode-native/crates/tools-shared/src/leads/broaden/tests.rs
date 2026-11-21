use super::*;
use octocode_test_support::json_fixture;

#[test]
fn broadening_drops_one_filter_group_or_keyword_and_keeps_public_scope() {
    let row = json_fixture(
        r#"{
            "operation": "search",
            "owner": "octo",
            "keywords": ["cache", "redis"],
            "filters": {"language": "Rust"},
            "sort": {"by": "stars", "order": "desc"},
            "page": 3
        }"#,
    );
    let lead = broaden(
        ToolId::GhSearchRepo,
        &row,
        &Broaden {
            strip: &[],
            groups: &[&["filters"]],
            split_lone_keyword: false,
        },
        5,
    )
    .expect("broader searches");
    assert_eq!(lead["kind"], "related");
    octocode_runtime_base::contracts::validate("ghSearchRepo", lead["input"].clone())
        .expect("authored input");
    let variants = lead["input"]["queries"].as_array().expect("queries");
    assert_eq!(variants.len(), 3);
    for variant in variants {
        assert_eq!(variant["owner"], row["owner"]);
        assert_eq!(variant["sort"], row["sort"]);
        assert!(variant.get("page").is_none());
        let filter_removed = variant.get("filters").is_none();
        let keyword_removed = variant["keywords"].as_array().expect("keywords").len() == 1;
        assert_ne!(
            filter_removed, keyword_removed,
            "one scope change per variant: {variant}"
        );
    }
}

#[test]
fn broadening_never_emits_a_request_with_no_search_target() {
    let row = json_fixture(
        r#"{
            "operation": "search",
            "ecosystem": "npm",
            "keywords": ["cache"]
        }"#,
    );
    assert!(
        broaden(
            ToolId::ArtifactSearch,
            &row,
            &Broaden {
                strip: &[],
                groups: &[],
                split_lone_keyword: false,
            },
            5
        )
        .is_none()
    );
}

/// A broader lead another related call already runs is no second lead: the
/// row's `related` has one writer, which states each call once.
#[test]
fn a_broader_lead_a_related_call_already_holds_is_not_added_again() {
    let row = |keyword: &str| json!({"operation":"search","keywords":[keyword]});
    let wide = octocode_runtime_base::response::result::Continuation::related_input(
        ToolId::GhSearchRepo,
        json!({"queries": [row("cache"), row("redis")]}),
    )
    .build();
    let narrow = octocode_runtime_base::response::result::Continuation::related_input(
        ToolId::GhSearchRepo,
        json!({"queries": [row("redis")]}),
    )
    .build();
    let mut value = json!({"related": [wide.clone()]});
    set_broadened(&mut value, &json!({}), narrow.clone());
    set_broadened(&mut value, &json!({}), wide.clone());
    assert_eq!(value["related"], json!([wide]), "{value}");
}
