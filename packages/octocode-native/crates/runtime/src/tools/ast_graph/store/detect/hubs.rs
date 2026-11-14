//! Centrality detectors: critical files and single points of failure.
use super::*;

/// Fewest transitive runtime dependents that make a file critical.
const CRITICAL_DEPENDENTS: usize = 5;

/// Every production file with at least [`CRITICAL_DEPENDENTS`] transitive
/// runtime dependents is a finding: none is sampled away, so equal hubs are
/// reported alike and paging reaches all of them. Severity grows with the
/// dependent count; the score's impact term is the file's PageRank
/// percentile.
pub(super) fn detect_critical(c: &mut Ctx) {
    let v = &c.v;
    let candidates = v
        .files
        .iter()
        .copied()
        .filter(|f| v.file_level(*f) && !v.inc[*f as usize].is_empty())
        .collect::<Vec<_>>();
    for file in candidates {
        let (dependents, farthest) = c.v.dependents(file);
        if dependents < CRITICAL_DEPENDENTS {
            continue;
        }
        push(
            &mut c.findings,
            Draft {
                detector: "critical-file",
                subject: c.v.key(file).to_owned(),
                id_parts: &[],
                title: format!("High blast radius: {dependents} files depend on it transitively"),
                severity: 0.3 * magnitude(dependents),
                confidence: 0.9,
                impact: c.v.impact(file),
                evidence: json!({"pageRank": significant(c.v.rank[file as usize]), "transitiveDependents": dependents, "directImporters": c.v.inc[file as usize].len(), "farthestDepth": farthest.max(1)}),
                controls: vec![
                    "informational: changes here need wide test coverage",
                    "every production file with 5 or more transitive dependents is listed; severity grows with the count",
                    "transitiveDependents counts production runtime importers at every depth (test, type-only and deferred imports, support code and imports within one Rust module tree left out); farthestDepth is how many hops the farthest of them lies away",
                ],
            },
        );
    }
}

pub(super) fn detect_spof(c: &mut Ctx) {
    let points = articulation_points(&c.v);
    for (file, smaller) in points {
        if smaller < 5 || !c.v.file_level(file) {
            continue;
        }
        push(
            &mut c.findings,
            Draft {
                detector: "single-point-of-failure",
                subject: c.v.key(file).to_owned(),
                id_parts: &[],
                title: format!(
                    "Only link between two parts of the import graph ({smaller} files on the smaller side)"
                ),
                severity: 0.4 * magnitude(smaller),
                confidence: 0.6,
                impact: c.v.impact(file),
                evidence: json!({"smallerSide": smaller}),
                controls: vec!["undirected runtime import projection"],
            },
        );
    }
}
