//! Centrality detectors: critical files and single points of failure.
use super::*;

pub(super) fn detect_critical(c: &mut Ctx) {
    let v = &c.v;
    let mut ranked = v
        .files
        .iter()
        .copied()
        .filter(|f| v.file_level(*f) && !v.inc[*f as usize].is_empty())
        .collect::<Vec<_>>();
    ranked.sort_by(|a, b| {
        v.rank[*b as usize]
            .total_cmp(&v.rank[*a as usize])
            .then(a.cmp(b))
    });
    let take = (v.files.len() / 50).clamp(1, 10);
    let pending = ranked.into_iter().take(take).collect::<Vec<_>>();
    for file in pending {
        let dependents = c.v.dependents(file);
        if dependents < 5 {
            continue;
        }
        push(
            &mut c.findings,
            Draft {
                detector: "critical-file",
                subject: c.v.key(file).to_owned(),
                id_parts: &[],
                title: format!("High blast radius: {dependents} files depend on it transitively"),
                severity: 0.3,
                confidence: 0.9,
                impact: c.v.impact(file),
                evidence: json!({"pageRank": c.v.rank[file as usize], "transitiveDependents": dependents, "directImporters": c.v.inc[file as usize].len()}),
                controls: vec![
                    "informational: changes here need wide test coverage",
                    "only the top 2% of imported files by PageRank (1 to 10) are checked",
                ],
                verify: vec![step(
                    ToolId::GraphQuery,
                    json!({"operation": "dependents", "target": c.v.key(file), "depth": 3}),
                )],
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
                verify: vec![step(
                    ToolId::GraphQuery,
                    json!({"operation": "walk", "target": c.v.key(file), "direction": "both", "edges": ["imports"], "depth": 1}),
                )],
            },
        );
    }
}
