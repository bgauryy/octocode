use super::*;
use std::cell::Cell;

fn hits(indices: impl IntoIterator<Item = usize>) -> Vec<Hit> {
    indices
        .into_iter()
        .map(|index| Hit {
            index,
            at: 0,
            every: false,
            score: (0, 0),
        })
        .collect()
}

/// Enumerate complete windows directly; no sliding state or coverage table.
fn oracle(
    hits: &[Hit],
    lines: &[String],
    needles: &[String],
    declarations: &[bool],
) -> Option<(usize, usize)> {
    (0..hits.len())
        .flat_map(|start| (start..hits.len()).map(move |end| (start, end)))
        .filter(|&(start, end)| {
            hits[end].index - hits[start].index <= MAX_WINDOW_LINES
                && needles.iter().all(|needle| {
                    hits[start..=end]
                        .iter()
                        .any(|hit| lines[hit.index].contains(needle))
                })
        })
        .min_by_key(|&(start, end)| {
            (
                !declarations[start],
                hits[end].index - hits[start].index,
                start,
            )
        })
}

#[test]
fn flat_window_matches_exhaustive_unicode_coverage_and_declaration_ranking() {
    let needles: Vec<_> = ["α", "β", "γ"].map(str::to_owned).into();
    let hits = hits(0..4);
    for encoded in 0usize..8usize.pow(hits.len() as u32) {
        let lines: Vec<_> = (0..hits.len())
            .map(|index| {
                needles
                    .iter()
                    .enumerate()
                    .filter(|(bit, _)| encoded >> (index * needles.len()) & (1 << bit) != 0)
                    .map(|(_, word)| word.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect();
        for declarations in 0usize..1 << hits.len() {
            let declared: Vec<_> = (0..hits.len())
                .map(|index| declarations & (1 << index) != 0)
                .collect();
            assert_eq!(
                covering_window(&hits, &lines, &needles, |at| declared[at]),
                oracle(&hits, &lines, &needles, &declared),
                "lines={lines:?}, declarations={declared:?}"
            );
        }
    }
}

#[test]
fn line_distance_controls_windows_and_keyword_ids_can_repeat() {
    let needles: Vec<_> = ["alpha", "beta", "alpha"].map(str::to_owned).into();
    for distance in [MAX_WINDOW_LINES - 1, MAX_WINDOW_LINES, MAX_WINDOW_LINES + 1] {
        let mut lines = vec![String::new(); distance + 4];
        lines[0] = "alpha".into();
        lines[distance] = "beta".into();
        lines[distance + 2] = "alpha".into();
        lines[distance + 3] = "beta".into();
        let hits = hits([0, distance, distance + 2, distance + 3]);
        for declared in [
            [false; 4],
            [true, false, false, false],
            [false, false, true, false],
        ] {
            assert_eq!(
                covering_window(&hits, &lines, &needles, |at| declared[at]),
                oracle(&hits, &lines, &needles, &declared)
            );
        }
    }
    assert_eq!(covering_window(&[], &[], &needles, |_| false), None);
}

#[test]
fn a_winning_declaration_does_not_rescan_losing_candidates() {
    let lines: Vec<_> = (0..1_000)
        .map(|index| if index % 2 == 0 { "alpha" } else { "beta" }.to_owned())
        .collect();
    let hits = hits(0..lines.len());
    let needles = ["alpha".to_owned(), "beta".to_owned()];
    let probes = Cell::new(0);
    assert_eq!(
        covering_window(&hits, &lines, &needles, |_| {
            probes.set(probes.get() + 1);
            true
        }),
        Some((0, 1))
    );
    assert_eq!(probes.get(), 1, "later windows cannot outrank the first");
}

#[test]
fn empty_keywords_keep_the_earliest_declaration_or_first_hit() {
    let lines = vec![String::new(); 3];
    let hits = hits(0..lines.len());
    for declared in [[false; 3], [false, true, true]] {
        assert_eq!(
            covering_window(&hits, &lines, &[], |at| declared[at]),
            oracle(&hits, &lines, &[], &declared)
        );
    }
}

#[test]
fn packed_membership_crosses_word_boundaries_without_a_keyword_limit() {
    for count in [63, 64, 65, 129] {
        let needles: Vec<_> = (0..count).map(|index| format!("α{index:03}β")).collect();
        let lines: Vec<_> = (0..4)
            .map(|line| {
                needles
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| index % 3 == line || line == 3)
                    .map(|(_, needle)| needle.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect();
        let hits = hits(0..lines.len());
        for declared in [
            [false; 4],
            [true, false, false, false],
            [false, false, false, true],
        ] {
            assert_eq!(
                covering_window(&hits, &lines, &needles, |at| declared[at]),
                oracle(&hits, &lines, &needles, &declared)
            );
        }
    }
}
