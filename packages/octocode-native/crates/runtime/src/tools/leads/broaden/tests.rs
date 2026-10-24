use super::*;

#[test]
fn broadening_drops_one_filter_group_or_keyword_and_keeps_public_scope() {
    let row = json!({"operation":"search","owner":"octo","keywords":["cache","redis"],
        "filters":{"language":"Rust"},"sort":{"by":"stars","order":"desc"},"page":3});
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
    crate::contracts::validate("ghSearchRepo", lead["input"].clone()).expect("authored input");
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
fn classification_context_survives_a_fresh_broader_search() {
    let classification =
        json!({"predicates":["Does this implement cache eviction?"],"context":"Trace expiration."});
    let row = json!({"owner":"octo","repo":"demo","keywords":["cache","redis"],"classification":classification,"page":2});
    let lead = broaden(
        ToolId::GhSearchCode,
        &row,
        &Broaden {
            strip: &[],
            groups: &[],
            split_lone_keyword: false,
        },
        5,
    )
    .expect("broader searches");
    crate::contracts::validate("ghSearchCode", lead["input"].clone()).expect("authored input");
    for variant in lead["input"]["queries"].as_array().unwrap() {
        assert_eq!(variant["classification"], classification);
        assert!(variant.get("page").is_none());
    }
}

#[test]
fn broadening_never_emits_a_request_with_no_search_target() {
    let row = json!({"operation":"search","ecosystem":"npm","keywords":["cache"]});
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
