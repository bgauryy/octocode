use super::super::super::snapshots::Pruned;
use super::pruned_warnings;

/// Files linked past the checkpoint budgets get one warning naming
/// their count and the budgets; a build that checkpointed every file
/// warns of nothing.
#[test]
fn files_past_the_checkpoint_budget_are_disclosed() {
    let mut progress = super::super::super::super::types::BuildProgress {
        max_spill_bytes: 2 << 30,
        max_memory_only_bytes: 64 << 20,
        ..Default::default()
    };
    assert_eq!(super::uncheckpointed_warning(&progress), None);
    progress.uncheckpointed_files = 7_176;
    let warning = super::uncheckpointed_warning(&progress).expect("warning");
    assert!(warning.contains("7176 file(s)"), "{warning}");
    assert!(warning.contains(&(2u64 << 30).to_string()), "{warning}");
}

/// A prune states each reason with its count; the removed IDs are the
/// receipt's `pruned` list, so no warning repeats one.
#[test]
fn a_prune_states_each_reason_once_and_names_no_id_twice() {
    let pruned = [
        Pruned {
            id: "20261109T163855Z-a".into(),
            outdated: false,
        },
        Pruned {
            id: "20261109T181915Z-a".into(),
            outdated: false,
        },
        Pruned {
            id: "20250101T000000Z-b".into(),
            outdated: true,
        },
    ];
    let warnings = pruned_warnings(&pruned, 1);
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(
        warnings[0].contains("keep:1") && warnings[0].contains('2'),
        "{warnings:?}"
    );
    assert!(
        warnings[1].contains("older graph format") && warnings[1].contains('1'),
        "{warnings:?}"
    );
    for snapshot in &pruned {
        assert!(
            warnings
                .iter()
                .all(|warning| !warning.contains(&snapshot.id)),
            "{} repeated in {warnings:?}",
            snapshot.id
        );
    }
}
