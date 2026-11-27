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
