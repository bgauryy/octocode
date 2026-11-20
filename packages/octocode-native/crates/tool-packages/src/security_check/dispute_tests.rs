//! Disagreements between databases, stated on the row.
use super::*;
use crate::security_check::record::SecurityAdvisory;
use crate::security_check::source::Source;

fn listed(pairs: &[(&str, &str)]) -> Vec<SourceResult> {
    let mut result = SourceResult::new(Source::Osv);
    result.advisories = pairs
        .iter()
        .map(|(id, alias)| SecurityAdvisory {
            id: (*id).to_owned(),
            aliases: vec![(*alias).to_owned()],
            ..SecurityAdvisory::default()
        })
        .collect();
    vec![result]
}

fn read(disputing: &[&str], unread: Vec<String>) -> Read {
    Read {
        version: Some("2.0.0".into()),
        name: "tensorflow".into(),
        disputing: disputing.iter().map(|id| (*id).to_owned()).collect(),
        warnings: unread,
    }
}

/// Many disagreements are one warning naming each advisory with the record
/// that disputes it, never one near-identical sentence per advisory
/// (LEFTOVERS P3, tensorflow 2.0.0); each record still names its own.
#[test]
fn many_disagreements_are_one_warning_naming_each() {
    let pairs = [
        ("GHSA-a", "PYSEC-1"),
        ("GHSA-b", "PYSEC-2"),
        ("GHSA-c", "PYSEC-3"),
    ];
    let mut results = listed(&pairs);
    let warnings = read(&["PYSEC-1", "PYSEC-2", "PYSEC-3"], Vec::new()).mark(&mut results);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    for (id, by) in pairs {
        assert!(
            warnings[0].contains(id) && warnings[0].contains(by),
            "{id}/{by}: {warnings:?}"
        );
    }
    assert!(
        results[0]
            .advisories
            .iter()
            .all(|row| row.disputed_by.len() == 1)
    );
    // One disagreement keeps its own sentence.
    let mut one = listed(&pairs[..1]);
    let single = read(&["PYSEC-1"], Vec::new()).mark(&mut one);
    assert_eq!(single.len(), 1);
    assert!(
        single[0].contains("the row counts GHSA-a as affecting it"),
        "{single:?}"
    );
}
