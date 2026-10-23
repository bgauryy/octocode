use super::*;

/// The banner states this call's count; how to follow `next` lives once
/// in the instructions.
#[test]
fn the_incomplete_banner_states_only_the_count() {
    let mut structured = Map::new();
    lead_with_incomplete_banner(&mut structured, 2, 2);
    assert_eq!(
        structured["warnings"],
        serde_json::json!(["incomplete — 2 of 2 rows continue on later response pages"])
    );
}
