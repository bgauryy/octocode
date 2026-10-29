//! Centrality detectors: hubs, critical files and single points of failure.
use super::*;

pub(super) fn median(mut values: Vec<usize>) -> usize {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    values[values.len() / 2]
}

/// A file that only forwards: three or more re-exports, or a Rust module
/// root that mounts three or more child modules (`mod a; mod b; mod c;`,
/// `pub use` lines), with at most one declaration of its own. A `mod`
/// declaration is a module symbol, not an own declaration.
pub(super) fn is_barrel(v: &View, file: u32) -> bool {
    let t = v.t;
    let out = t.out(file);
    let reexports = out
        .iter()
        .filter(|e| e.kind == EdgeKind::Imports && t.str(e.detail).contains("reexport"))
        .count();
    let modules = out
        .iter()
        .filter(|e| {
            e.kind == EdgeKind::Contains
                && t.nodes[e.dst as usize].kind == NodeKind::File
                && t.str(e.detail) == MODULE_DETAIL
        })
        .count();
    let declarations = out
        .iter()
        .filter(|e| {
            let node = &t.nodes[e.dst as usize];
            e.kind == EdgeKind::Contains
                && node.kind == NodeKind::Symbol
                && t.str(node.detail) != MODULE_DETAIL
        })
        .count();
    (reexports >= 3 || modules >= 3) && declarations <= 1
}

pub(super) fn detect_hubs(c: &mut Ctx) {
    let v = &c.v;
    let candidates = v
        .files
        .iter()
        .copied()
        .filter(|f| v.file_level(*f))
        .collect::<Vec<_>>();
    let fan_in = |f: u32| v.inc[f as usize].len();
    let fan_out = |f: u32| v.out[f as usize].len();
    let mid_in = median(
        candidates
            .iter()
            .map(|f| fan_in(*f))
            .filter(|x| *x > 0)
            .collect(),
    );
    let mid_out = median(
        candidates
            .iter()
            .map(|f| fan_out(*f))
            .filter(|x| *x > 0)
            .collect(),
    );
    let mut pending = Vec::new();
    for file in &candidates {
        let (i, o) = (fan_in(*file), fan_out(*file));
        let balanced = (i as i64 - o as i64).unsigned_abs() as f64 * 4.0 < (i + o) as f64;
        if i > mid_in.max(2)
            && o > mid_out.max(2)
            && balanced
            && i + o >= 10
            && !is_barrel(v, *file)
        {
            pending.push((*file, i, o));
        }
    }
    for (file, i, o) in pending {
        let dependents = c.v.dependents(file);
        push(
            &mut c.findings,
            Draft {
                detector: "god-file",
                subject: c.v.key(file).to_owned(),
                id_parts: &[],
                title: format!("Hub-like file: {i} importers and {o} imports"),
                severity: 0.5 * magnitude(i * o / 4),
                confidence: 0.7,
                impact: c.v.impact(file),
                evidence: json!({"fanIn": i, "fanOut": o, "medianFanIn": mid_in, "medianFanOut": mid_out, "transitiveDependents": dependents}),
                controls: vec![
                    "Arcan hub-like thresholds (fan-in/out above median, balanced)",
                    "barrels and rust module roots excluded",
                    "edges within one rust module tree not counted",
                ],
                verify: vec![format!(
                    "octocode graph query symbols {}",
                    quote(c.v.key(file))
                )],
            },
        );
    }
}

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
                verify: vec![format!(
                    "octocode graph query dependents {} --depth 3",
                    quote(c.v.key(file))
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
                verify: vec![format!(
                    "octocode graph query walk {} --direction both --edge imports --depth 1",
                    quote(c.v.key(file))
                )],
            },
        );
    }
}
