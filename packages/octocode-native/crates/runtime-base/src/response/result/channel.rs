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
pub fn insert_next(calls: &mut Value, channel: impl Channel, call: Continuation) {
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
mod tests;
