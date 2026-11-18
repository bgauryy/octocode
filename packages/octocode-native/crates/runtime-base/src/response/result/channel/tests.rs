use super::*;
use crate::id::ToolId;
use octocode_contract_types::ALL_CHANNELS;
use octocode_test_support::json_fixture;

/// Every declared channel has owners, and every owner is a tool the
/// runtime knows.
#[test]
fn every_channel_declares_owners_the_runtime_knows() {
    assert!(!ALL_CHANNELS.is_empty());
    for spec in ALL_CHANNELS {
        assert!(!spec.owners.is_empty(), "{}", spec.key);
        for owner in spec.owners {
            assert!(ToolId::from_name(owner).is_some(), "{}: {owner}", spec.key);
        }
    }
}

/// A recovery call retries the row's own tool, so every tool owns an
/// error channel.
#[test]
fn an_error_channel_is_owned_by_every_tool() {
    for tool in ToolId::ALL {
        assert!(
            owns(octocode_contract_types::ErrorChannel::Retry, tool.as_str()),
            "{}",
            tool.as_str()
        );
    }
}

#[test]
#[should_panic(expected = "does not carry")]
fn a_channel_refuses_a_tool_it_does_not_own() {
    let mut calls = serde_json::json!({});
    let call = Continuation::continued_input(
        ToolId::LocalSearch,
        json_fixture(r#"{"queries": [{"path": "src", "matchString": ["x"]}]}"#),
    );
    insert_next(
        &mut calls,
        octocode_contract_types::StructureSearchChannel::NextPage,
        call,
    );
}
