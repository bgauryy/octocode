use super::*;

/// GitHub routing sees exactly the contract's `github` family.
#[test]
fn the_github_identity_covers_exactly_the_github_family() {
    for id in ToolId::ALL {
        assert_eq!(id.github().is_some(), id.is_github(), "{}", id.as_str());
    }
}

fn contract_tools() -> &'static [serde_json::Value] {
    crate::contracts::parsed_contract().expect("contract")["tools"]
        .as_array()
        .expect("tools[]")
}

#[test]
fn numbered_names_repeat_their_kind() {
    let names = (0..3)
        .map(|index| numbered("readHits", index))
        .collect::<Vec<_>>();
    assert_eq!(names, ["readHits", "readHits2", "readHits3"]);
}

#[test]
fn names_round_trip_and_are_unique() {
    let mut seen = std::collections::HashSet::new();
    for id in ToolId::ALL {
        assert!(seen.insert(id.as_str()), "duplicate name {}", id.as_str());
        assert_eq!(ToolId::from_name(id.as_str()), Some(id));
        assert_eq!(id.as_str().parse::<ToolId>(), Ok(id));
    }
    assert_eq!(ToolId::from_name("nope"), None);
    assert!("nope".parse::<ToolId>().is_err());
    assert_eq!(ToolId::from_name("semanticAssess"), None);
}

#[test]
fn identity_and_policy_match_the_embedded_contract() {
    let tools = contract_tools();
    assert_eq!(tools.len(), ToolId::ALL.len());
    for (tool, id) in tools.iter().zip(ToolId::ALL) {
        assert_eq!(tool["name"], id.as_str());
        let family = match id.family() {
            ToolFamily::Local => "local",
            ToolFamily::GitHub => "github",
            ToolFamily::Remote => "remote",
        };
        assert_eq!(tool["family"], family, "{id}");
        assert_eq!(tool["beta"], id.is_beta(), "{id}");
        assert_eq!(
            tool.get("rowPageWindow") == Some(&serde_json::Value::Bool(true)),
            id.pages_by_row_window(),
            "{id}"
        );
        assert_eq!(
            tool.get("hostFacts")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|facts| facts.iter().any(|fact| fact == "lspServers")),
            id.lists_lsp_servers(),
            "{id}"
        );
        assert_eq!(tool["title"], id.title(), "{id}");
        assert_eq!(
            crate::contracts::tool_contract(id).expect("tool contract"),
            tool,
            "{id}"
        );
    }
}
