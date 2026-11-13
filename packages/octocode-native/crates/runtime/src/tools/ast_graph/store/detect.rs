//! graphQuery `issues`: graph-algorithm detectors that rank *possible*
//! problems. Every finding is a hypothesis carrying its evidence, the
//! false-positive controls applied.
//!
//! Two rules keep findings honest:
//! - **Liveness is optimistic.** Any production edge (a low-confidence call, a
//!   dynamic or type-only import) keeps a node alive for dead-code detectors.
//! - **Violations are pessimistic.** Only runtime import edges between
//!   authored files count as evidence for cycles, centrality, and layering.
//!
//! Test code (test files, Rust `#[cfg(test)]` items and the modules they
//! mount) is never production evidence: its edges carry `test`.
//!
//! A Rust module tree is one unit: `use` edges between a module and the
//! modules declared below it ([`rust_lineage`]) are left out of the file
//! view and the cycle graph, so a parent with children is no cycle and no
//! single point of failure by construction.
//!
//! Generated, bundled, vendored, and declaration files are never subjects.
use super::super::classify::{
    ROLE_CONFIG, ROLE_DECLARATION, ROLE_NOT_AUTHORED, ROLE_NOT_PRODUCTION, ROLE_SUPPORT, ROLE_TEST,
    ROLE_UNPARSED, Roles, role_names,
};
use super::format::{
    EdgeKind, FLAG_EXPORTED, FLAG_LOCAL_USE, FLAG_TEST, GraphTables, NONE, NodeKind,
};
use super::tables::{MODULE_DETAIL, REEXPORTED_NAME};
use crate::tools::ast_graph::algorithms::{CycleLoad, Cycles};
use crate::tools::ast_graph::edges::{Load, RUST_MODULE, load};
use crate::tools::ast_graph::graph::rust_module_child_dir;
use crate::tools::ast_graph::paths::dirname;
use crate::tools::ast_graph::types::DiagnosticCode;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

mod components;
mod context;
mod cycles;
mod dead_code;
mod deps;
mod hubs;
mod mention_stream;
mod mention_tokens;
mod mentions;
mod primitives;
mod reach;
mod view;
use components::*;
use context::*;
pub(super) use cycles::file_graph;
use cycles::*;
use dead_code::*;
use deps::*;
use hubs::*;
use mention_tokens::*;
use mentions::*;
pub(super) use primitives::tarjan;
use primitives::*;
use reach::*;
use view::*;

/// Every detector, in run order: the detector names it reports, and the step. The one list `detectors` is checked against.
const STEPS: &[DetectorStep] = &[
    (&["cycle"], detect_cycles),
    (&["dir-cycle"], detect_dir_cycles),
    (&["unreachable-file", "test-only"], detect_reachability),
    (
        &["unused-export", "export-only-local"],
        detect_unused_exports,
    ),
    (&["undeclared-dependency"], detect_dependencies),
    (&["boundary-violation"], detect_boundaries),
    (&["unresolved-import"], detect_unresolved),
    (&["critical-file"], detect_critical),
    (&["single-point-of-failure"], detect_spof),
];

/// Every detector name, in run order: the graphQuery `detectors` choices.
pub(crate) fn detector_names() -> impl Iterator<Item = &'static str> {
    STEPS.iter().flat_map(|(names, _)| names.iter().copied())
}

/// Languages whose named imports link to declarations, so an export no
/// importer names can be judged unused. Rust, Go, Java and C items are
/// reached through receivers, packages and paths the syntax does not link:
/// their exports are not judged one by one (their files still are).
const NAMED_IMPORT_LANGUAGES: &[&str] = &["typescript", "tsx", "javascript", "python"];

/// Whether an export of `file` of declaration `kind` can be judged unused.
/// Python has no export syntax, so only its functions and classes are API.
fn exports_judged(file: &str, kind: &str) -> bool {
    let language = crate::tools::ast_graph::languages::canonical_language(file, "");
    NAMED_IMPORT_LANGUAGES.contains(&language.as_str())
        && (language != "python" || matches!(kind, "function" | "class"))
}

const PACKAGE_SCOPED: &[&str] = &["go", "java", "scala", "csharp"];

#[derive(Clone, Debug)]
pub(crate) struct Finding {
    pub detector: &'static str,
    pub subject: String,
    /// Stable across snapshots: detector + subject + identifying evidence.
    pub id: String,
    pub title: String,
    pub severity: f64,
    pub confidence: f64,
    pub impact: f64,
    pub score: f64,
    pub evidence: Value,
    pub controls: Vec<&'static str>,
    pub corroborated_by: Vec<&'static str>,
    /// vulture-style certainty: 100 nothing references it anywhere (every
    /// text file was read), 90 no graph or code reference (bare prose words
    /// go to `evidence.textMentions`), 60 the file path or every listed
    /// unused export is referenced in code or qualified text
    /// (`evidence.mentionedIn`), or no mention scan ran. An export group
    /// only partly referenced splits into one finding per tier. The default
    /// view shows 90+.
    pub tier: u8,
}

impl Finding {
    pub(crate) fn to_json(&self) -> Value {
        let round = |v: f64| (v * 1000.0).round() / 1000.0;
        let mut out = json!({
            "id": self.id,
            "detector": self.detector,
            "subject": self.subject,
            "title": self.title,
            "score": round(self.score),
            "severity": round(self.severity),
            "confidence": round(self.confidence),
            "impact": round(self.impact),
            "tier": self.tier,
            "evidence": self.evidence,
        });
        if !self.controls.is_empty() {
            out["controls"] = json!(self.controls);
        }
        if !self.corroborated_by.is_empty() {
            out["corroboratedBy"] = json!(self.corroborated_by);
        }
        out
    }
}

pub(crate) struct Report {
    pub findings: Vec<Finding>,
    pub summary: Value,
    /// What proves the report current besides its snapshot; `None` when no
    /// proof could be taken.
    proof: Option<octocode_engine::stamp::Proof>,
}

impl Report {
    /// About how many bytes it holds.
    fn weight(&self) -> usize {
        let json = |value: &Value| octocode_engine::json_size::bytes(value).unwrap_or(usize::MAX);
        json(&self.summary).saturating_add(
            self.findings
                .iter()
                .map(|finding| {
                    std::mem::size_of::<Finding>()
                        .saturating_add(finding.subject.len() + finding.id.len())
                        .saturating_add(finding.title.len())
                        .saturating_add(json(&finding.evidence))
                })
                .fold(0, usize::saturating_add),
        )
    }
}

fn stable_id(detector: &str, parts: &[&str]) -> String {
    let mut hasher = octocode_engine::digest::HashWriter::default();
    hasher.update(detector.as_bytes());
    for part in parts {
        hasher.update([0]);
        hasher.update(part.as_bytes());
    }
    hex::encode(hasher.finalize())[..12].to_owned()
}

/// Maps magnitude (members, dependents, count) into a `[0.6, 1.0]` factor.
fn magnitude(size: usize) -> f64 {
    (0.6 + 0.08 * ((1 + size) as f64).log2()).min(1.0)
}

#[cfg(test)]
thread_local! {
    /// Tests: the detector pipelines this thread has run.
    pub(crate) static RUNS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// [`run`] of the snapshot named `id`, whose tables never change, kept across
/// calls while what its mention scan read stands.
pub(crate) fn kept_run(
    id: &str,
    t: &GraphTables,
    only: &[String],
    root: Option<&std::path::Path>,
    entries: Option<&[String]>,
    stopped: &(dyn Fn() -> bool + Sync),
) -> Result<std::sync::Arc<Report>, String> {
    static REPORTS: crate::tools::page_memo::PageMemo<crate::tools::page_memo::Proven<Report>> =
        crate::tools::page_memo::PageMemo::new();
    let key = crate::digest::json_sha256(&json!([
        id,
        root.map(std::path::Path::to_string_lossy),
        only,
        entries,
    ]));
    if let Some(kept) = REPORTS.current(&key, "") {
        return Ok(kept);
    }
    let report = std::sync::Arc::new(run(t, only, root, entries, stopped)?);
    if !stopped()
        && let Some(proof) = &report.proof
    {
        let bytes = report.weight().saturating_add(proof.weight());
        REPORTS.put_proven(
            key,
            String::new(),
            bytes,
            proof.clone(),
            std::sync::Arc::clone(&report),
        );
    }
    Ok(report)
}

/// Runs the selected detectors (all when `only` is empty) and ranks them.
/// With `root`, dead-code findings are re-tiered by a repo-wide mention scan.
pub(crate) fn run(
    t: &GraphTables,
    only: &[String],
    root: Option<&std::path::Path>,
    entries: Option<&[String]>,
    stopped: &(dyn Fn() -> bool + Sync),
) -> Result<Report, String> {
    for name in only {
        if !detector_names().any(|known| known == name) {
            return Err(format!(
                "unknown detector {name:?}; expected one of: {}",
                detector_names().collect::<Vec<_>>().join(", ")
            ));
        }
    }
    #[cfg(test)]
    RUNS.with(|runs| runs.set(runs.get() + 1));
    let wants = |name: &str| only.is_empty() || only.iter().any(|o| o == name);
    let mut c = Ctx {
        v: View::new(t, entries),
        findings: Vec::new(),
        notes: Vec::new(),
    };
    for &(detectors, step) in STEPS {
        if detectors.iter().any(|d| wants(d)) {
            step(&mut c);
        }
    }
    // Liveness spreads from mentioned `unreachable-file` and `test-only`
    // subjects, so `test-only` keeps unreachable findings through the scan.
    let spread = root.is_some() && wants("test-only");
    c.findings
        .retain(|f| wants(f.detector) || (spread && f.detector == "unreachable-file"));
    let mut proof = Some(octocode_engine::stamp::Proof::default());
    if let Some(root) = root {
        proof = tier_dead_code(&c.v, &mut c.findings, &mut c.notes, root, stopped);
        spread_mentioned_liveness(&c.v, &mut c.findings);
        c.findings.retain(|f| wants(f.detector));
    } else if c
        .findings
        .iter()
        .any(|f| matches!(f.detector, "unreachable-file" | "unused-export"))
    {
        // Without the scan a tier-90 dead-code finding has had no text check.
        for finding in &mut c.findings {
            if matches!(finding.detector, "unreachable-file" | "unused-export") {
                finding.tier = finding.tier.min(60);
            }
        }
        note(
            &mut c.notes,
            "unused-export",
            "mention scan skipped: the snapshot root is not a directory, so dead-code findings are tier 60 (no text check ran)",
        );
    }

    let mut by_detector = BTreeMap::<&str, usize>::new();
    for finding in &c.findings {
        *by_detector.entry(finding.detector).or_default() += 1;
    }
    let merged = corroborate(c.findings);
    let entries =
        c.v.entries
            .values()
            .fold(BTreeMap::<&str, usize>::new(), |mut acc, rule| {
                *acc.entry(rule).or_default() += 1;
                acc
            });
    let mut roles = BTreeMap::<&str, usize>::new();
    for file in &c.v.files {
        for role in role_names(c.v.flags(*file)) {
            *roles.entry(role).or_default() += 1;
        }
    }
    let health = health(&c.v);
    // The finding count is the page's pagination.total plus summary.hiddenBelowTier.
    let summary = json!({
        "byDetector": by_detector,
        "entrypoints": {"total": c.v.entries.len(), "byRule": entries},
        "fileRoles": roles,
        "health": health,
        "notes": c.notes,
    });
    Ok(Report {
        findings: merged,
        summary,
        proof,
    })
}

#[cfg(test)]
mod mention_tests;
#[cfg(test)]
mod tests;
