use super::*;
use serde_json::json;

/// Every `(tool, collection)` core's `continuationChannels` registry
/// declares, read from the embedded contract.
fn declared() -> Vec<(ToolId, String)> {
    let contract = crate::contracts::parsed_contract().expect("embedded contract");
    let registry = &contract["continuationChannels"];
    ToolId::ALL
        .iter()
        .flat_map(|tool| {
            registry[tool.as_str()]["collections"]
                .as_array()
                .expect("every tool declares its collections")
                .iter()
                .map(|name| (*tool, name.as_str().expect("name").to_owned()))
        })
        .collect()
}

#[test]
fn every_declared_collection_pages_by_its_pagination_next_page() {
    let declared = declared();
    assert!(!declared.is_empty(), "the registry declares collections");
    for (tool, collection) in declared {
        assert!(
            declares(tool, &collection),
            "{} {collection}",
            tool.as_str()
        );
        let mut pagination = json!({"page": 1, "hasMore": false});
        let call = Continuation::continued(tool, json!({"page": 2}));
        set_next_page(&mut pagination, tool, &collection, Some(call));
        assert_eq!(pagination["hasMore"], true, "{pagination}");
        let next = pagination["next"].as_object().expect("next");
        assert_eq!(next.keys().collect::<Vec<_>>(), [NEXT_PAGE], "{pagination}");
        assert_eq!(next[NEXT_PAGE]["kind"], "continue", "{pagination}");
        assert_eq!(next[NEXT_PAGE]["tool"], tool.as_str(), "{pagination}");
        assert_eq!(
            next[NEXT_PAGE]["input"]["queries"][0]["page"], 2,
            "{pagination}"
        );
    }
}

#[test]
fn the_last_page_of_a_collection_has_no_next() {
    let (tool, collection) = declared().remove(0);
    let mut pagination = json!({"page": 2, "hasMore": true, "next": {NEXT_PAGE: {}}});
    set_next_page(&mut pagination, tool, &collection, None);
    assert_eq!(pagination, json!({"page": 2, "hasMore": false}));
}

#[test]
fn only_registry_collections_are_collections() {
    for (tool, collection) in [
        (ToolId::GhCloneRepo, "files"),
        (ToolId::LspSearch, NEXT_PAGE),
        (ToolId::LocalFetch, "importers"),
    ] {
        assert!(
            !declares(tool, collection),
            "{} {collection}",
            tool.as_str()
        );
    }
}
