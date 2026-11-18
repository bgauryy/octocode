//! The `issues` score: severity × confidence × (0.5 + 0.5 × impact), raised
//! ×1.1 (at most 1) for each other detector on the same subject, where
//! impact is the highest PageRank percentile among the subject's files.
use super::*;

fn every_finding(root: &Path) -> Vec<Value> {
    let issues = ask(root, "issues", None, |o| {
        o.min_tier = Some(0);
        o.limit = Some(500);
    });
    crate::ast_graph::test_support::issue_rows(&issues.value)
}

/// Each row's score follows from the factors it states, and every factor
/// lies in `[0, 1]`.
#[test]
fn every_score_follows_from_the_factors_its_row_states() {
    let dir = dead_code_app();
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let rows = every_finding(root);
    assert!(!rows.is_empty());
    for row in &rows {
        let factor = |name: &str| {
            row[name]
                .as_f64()
                .unwrap_or_else(|| panic!("{name}: {row}"))
        };
        let (severity, confidence, impact) =
            (factor("severity"), factor("confidence"), factor("impact"));
        for value in [severity, confidence, impact] {
            assert!((0.0..=1.0).contains(&value), "{row}");
        }
        let others = row["corroboratedBy"].as_array().map_or(0, Vec::len);
        let expected = (severity
            * confidence
            * (0.5 + 0.5 * impact)
            * 1.1_f64.powi(i32::try_from(others).expect("count")))
        .min(1.0);
        // Each factor is stated rounded to three decimals.
        assert!(
            (factor("score") - expected).abs() <= 0.003,
            "score {expected:.4} expected: {row}"
        );
    }
}

/// Impact is one measure for every detector: a file's PageRank
/// percentile. `ping.ts` and `pong.ts` import only each other, so they
/// rank alike: the cycle they form and each one's `unreachable-file`
/// finding state the same impact.
#[test]
fn a_subjects_impact_is_its_files_page_rank_percentile_in_every_detector() {
    let dir = dead_code_app();
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let rows = every_finding(root);
    let impact = |detector: &str, subject: &str| {
        rows.iter()
            .find(|row| row["detector"] == detector && row["subject"] == format!("app/{subject}"))
            .map(|row| row["impact"].clone())
            .unwrap_or_else(|| panic!("{detector} {subject}: {rows:?}"))
    };
    let cycle = impact("cycle", "src/ping.ts");
    assert_eq!(impact("unreachable-file", "src/ping.ts"), cycle);
    assert_eq!(impact("unreachable-file", "src/pong.ts"), cycle);
}
