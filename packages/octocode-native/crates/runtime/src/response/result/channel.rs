//! The `next` keys a tool's rows may carry, from the generated channel enums
//! (core `continuationChannels`): a continue call is inserted under a typed
//! channel, and the channel's declared owners must include the call's tool.
use super::Continuation;
use octocode_contract_types::Channel;
use serde_json::Value;

/// Whether `channel` declares `tool` as an owner of its calls.
fn owns(channel: impl Channel, tool: &str) -> bool {
    channel.owners().contains(&tool)
}

/// `call` under `channel` in a producer's private `calls` bundle
/// ([`super::attach_calls`] publishes it under `next`). Debug and test
/// builds panic when the channel does not declare the call's tool.
#[track_caller]
pub(crate) fn insert_next(calls: &mut Value, channel: impl Channel, call: Continuation) {
    debug_assert!(
        owns(channel, call.tool().as_str()),
        "next.{} does not carry {} calls (owners: {:?}), built at {}",
        channel.key(),
        call.tool().as_str(),
        channel.owners(),
        std::panic::Location::caller(),
    );
    calls[channel.key()] = call.build();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::ToolId;
    use octocode_contract_types::ALL_CHANNELS;

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
            serde_json::json!({"queries": [{"path": "src", "matchString": ["x"]}]}),
        );
        insert_next(
            &mut calls,
            octocode_contract_types::StructureSearchChannel::NextPage,
            call,
        );
    }
}
