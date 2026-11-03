//! What a rate-limit wait says to the call's host while it lasts
//! ([`octocode_io::budget::pause_reporting`]): which limit, how many seconds
//! are left, and why the request is not sent yet.
use super::key_state::Block;
use super::{Group, ceil_secs};
use std::time::Duration;

/// A request held by a block: an exhausted bucket (its reset, or the
/// `retry-after` GitHub sent) or a secondary-limit cooldown.
pub(super) fn blocked(block: &Block, remaining: Duration) -> String {
    let seconds = ceil_secs(remaining);
    let resource = &block.resource;
    if block.reset.is_some() {
        format!(
            "GitHub {resource} limit reached: waiting {seconds}s for the quota to reset or the retry-after to pass"
        )
    } else {
        format!("GitHub secondary rate limit: waiting {seconds}s for the cooldown to end")
    }
}

/// A request spaced after the group's last start by the reported quota.
pub(super) fn paced(group: Group, remaining: Duration) -> String {
    format!(
        "GitHub {} requests are paced to the reported quota: waiting {}s for the next slot",
        group.name(),
        ceil_secs(remaining)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(reset: Option<u64>) -> Block {
        Block {
            until_ms: 0,
            resource: "code_search".into(),
            remaining: reset.map(|_| 0),
            reset,
            circuit: None,
        }
    }

    /// Each reason reads differently, and whole seconds round up.
    #[test]
    fn a_wait_names_its_limit_seconds_and_reason() {
        let left = Duration::from_millis(51_200);
        let quota = blocked(&block(Some(1)), left);
        assert!(
            quota.contains("code_search") && quota.contains("52s"),
            "{quota}"
        );
        assert!(quota.contains("quota to reset"), "{quota}");
        let cooldown = blocked(&block(None), left);
        assert!(
            cooldown.contains("secondary") && cooldown.contains("52s"),
            "{cooldown}"
        );
        let slot = paced(Group::CodeSearch, left);
        assert!(
            slot.contains("code_search") && slot.contains("52s"),
            "{slot}"
        );
    }
}
