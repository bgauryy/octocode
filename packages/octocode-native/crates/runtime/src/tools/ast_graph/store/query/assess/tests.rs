use super::*;

struct Cancelled;
impl crate::execution::cancel::CancellationCheck for Cancelled {
    fn check(&self) -> Result<(), String> {
        Err("cancelled".into())
    }
}

/// The request's cancellation reaches the git run: it is reported as
/// cancelled, not as a git failure, and no fixed timeout decides it.
#[test]
fn cancellation_stops_the_git_run_as_cancelled() {
    let dir = tempfile::tempdir().unwrap();
    let error = git_changed(dir.path(), "HEAD", &Cancelled).unwrap_err();
    assert_eq!(error.value["errorCode"], json!(ErrorCode::Cancelled));
}

/// A row as its detector's constants rebuild it: each field it omits is its
/// detector's, each evidence key it omits is its detector's evidence, and
/// its controls are its detector's and its own.
fn rebuilt(row: &Value, constants: &Map<String, Value>) -> Value {
    let mut whole = row.clone();
    let Some(shared) = constants.get(row["detector"].as_str().unwrap_or_default()) else {
        return whole;
    };
    for (key, value) in shared.as_object().expect("constants") {
        match key.as_str() {
            "evidence" => {
                let evidence = whole["evidence"].as_object().cloned().unwrap_or_default();
                let mut merged = value.as_object().cloned().unwrap_or_default();
                merged.extend(evidence);
                whole["evidence"] = Value::Object(merged);
            }
            "controls" => {
                let mut controls = value.as_array().cloned().unwrap_or_default();
                controls.extend(whole["controls"].as_array().cloned().unwrap_or_default());
                whole["controls"] = Value::Array(controls);
            }
            _ => whole[key] = value.clone(),
        }
    }
    whole
}

/// The same row with its controls as a set: the rows of a detector state
/// the shared ones first.
fn comparable(mut row: Value) -> Value {
    if let Some(controls) = row["controls"].as_array_mut() {
        controls.sort_by_key(Value::to_string);
    }
    row
}

/// Findings of several detectors, each with fields every finding of its
/// detector shares and fields that differ per finding.
fn findings() -> Vec<crate::tools::ast_graph::store::detect::Finding> {
    const DETECTORS: [&str; 3] = ["one", "two", "three"];
    const SHARED: [&str; 2] = ["applied to every finding", "a second shared control"];
    let mut findings = Vec::new();
    for (rank, detector) in DETECTORS.iter().enumerate() {
        for index in 0..=rank {
            let mut controls = SHARED.to_vec();
            if index == 1 {
                controls.push("this finding's own control");
            }
            findings.push(crate::tools::ast_graph::store::detect::Finding {
                detector,
                subject: format!("src/{detector}/{index}.ts"),
                id: format!("{detector}-{index}"),
                title: format!("{detector} finding"),
                severity: 0.5,
                confidence: 0.6 + rank as f64 / 10.0,
                impact: index as f64 / 4.0,
                score: index as f64 / 8.0,
                evidence: json!({"count": index, "dynamicallyImported": false, "rank": rank}),
                controls,
                corroborated_by: Vec::new(),
                tier: 90,
            });
        }
    }
    findings
}

/// What every listed row of a detector states alike goes out once, by
/// detector, and no row repeats it; a field that differs stays on each row,
/// and a detector of one row keeps its row whole. Rows rebuilt from the
/// constants are the findings exactly.
#[test]
fn a_detector_states_what_every_row_of_it_shares_once() {
    let findings = findings();
    let original = findings
        .iter()
        .map(super::super::super::detect::Finding::to_json)
        .collect::<Vec<_>>();
    let mut rows = original.clone();
    let constants = state_detector_constants(&mut rows);
    for (row, whole) in rows.iter().zip(&original) {
        assert_eq!(
            comparable(rebuilt(row, &constants)),
            comparable(whole.clone()),
            "{constants:?}"
        );
        let detector = row["detector"].as_str().expect("detector");
        let listed = original
            .iter()
            .filter(|other| other["detector"] == detector)
            .count();
        let Some(shared) = constants.get(detector) else {
            assert_eq!(listed, 1, "{detector}: {constants:?}");
            assert_eq!(row, whole);
            continue;
        };
        assert!(listed > 1, "{detector}: {constants:?}");
        for (key, value) in shared.as_object().expect("constants") {
            match value {
                Value::Object(inner) => {
                    for inner_key in inner.keys() {
                        assert!(
                            row[key].get(inner_key).is_none(),
                            "{key}.{inner_key}: {row}"
                        );
                    }
                }
                Value::Array(items) => {
                    for item in items {
                        assert!(
                            !row[key]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .any(|own| own == item),
                            "{key} {item}: {row}"
                        );
                    }
                }
                _ => assert!(row.get(key).is_none(), "{key}: {row}"),
            }
        }
        for differing in ["id", "subject", "score", "impact"] {
            assert_eq!(row.get(differing), whole.get(differing), "{row}");
        }
        assert!(row["evidence"].get("count").is_some(), "{row}");
    }
}
