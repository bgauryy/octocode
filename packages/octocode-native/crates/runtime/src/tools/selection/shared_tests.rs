use super::*;
use octocode_contract_types::{SelectionScope, SelectionUnit};

fn outcome(unjudged: Vec<[u32; 2]>) -> Outcome {
    let mut plan = plan::Plan::new(
        "binding".into(),
        3,
        SelectionStatus::Partial,
        SelectionScope::SelectedView,
        SelectionUnit::Hits,
        None,
    );
    plan.selected = vec![0];
    plan.unjudged = unjudged;
    plan.finish()
}

#[test]
fn related_items_are_decided_hidden_and_apart_from_unknown_ones() {
    let mut plan = plan::Plan::new(
        "binding".into(),
        3,
        SelectionStatus::Partial,
        SelectionScope::SelectedView,
        SelectionUnit::Chunks,
        None,
    );
    plan.selected = vec![0];
    plan.related = vec![2];
    let outcome = plan.finish();
    let receipt = &outcome.receipt;
    assert_eq!(receipt.related_items.map(u64::from), Some(1));
    assert_eq!((receipt.selected_items, receipt.unknown_items), (1, 1));
    assert_eq!(outcome.related, vec![2]);
    assert_eq!(outcome.shown(3), vec![0]);
    assert!(outcome.leaves_out());
    // A page with no related ordinal omits the count.
    assert_eq!(
        super::shared_tests::outcome(vec![]).receipt.related_items,
        None
    );
}
