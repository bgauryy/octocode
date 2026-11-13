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

/// A holder's walk reads on from its `next`, never from its `pagination`.
#[test]
fn every_declared_collection_pages_by_a_call_in_its_holders_next() {
    let declared = declared();
    assert!(!declared.is_empty(), "the registry declares collections");
    for (tool, collection) in declared {
        assert!(
            declares(tool, &collection),
            "{} {collection}",
            tool.as_str()
        );
        let mut holder = json!({"pagination": {"offset": 0, "hasMore": false}});
        let row = crate::tools::test_support::accepted_row(tool, &json!({}));
        let expected = Continuation::continued(tool, row.clone()).build();
        let call = Continuation::continued(tool, row);
        set_next_page(&mut holder, tool, &collection, Some(call));
        assert_eq!(holder["pagination"]["hasMore"], true, "{holder}");
        assert!(holder["pagination"].get("next").is_none(), "{holder}");
        let walk = walk_name(tool, &collection);
        let next = holder["next"].as_object().expect("next");
        assert_eq!(next.keys().collect::<Vec<_>>(), [walk], "{holder}");
        assert_eq!(next[walk], expected, "{holder}");
    }
}

/// A collection the rows name as a linear channel walks by its own name.
#[test]
fn a_section_walks_by_its_own_name() {
    assert_eq!(walk_name(ToolId::GhGetHistoryItem, "commits"), "commits");
    assert_eq!(walk_name(ToolId::AstSearch, "files"), NEXT_PAGE);
}

#[test]
fn the_last_page_of_a_collection_has_no_next() {
    let (tool, collection) = declared().remove(0);
    let walk = walk_name(tool, &collection);
    let mut holder = json!({"pagination": {"offset": 5, "hasMore": true}, "next": {walk: {}}});
    set_next_page(&mut holder, tool, &collection, None);
    assert_eq!(
        holder,
        json!({"pagination": {"offset": 5, "hasMore": false}})
    );
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
