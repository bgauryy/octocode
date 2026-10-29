//! `graph query issues`: graph-algorithm detectors that rank *possible*
//! problems. Every finding is a hypothesis carrying its evidence, the
//! false-positive controls applied, and commands that verify it.
//!
//! Two rules keep findings honest:
//! - **Liveness is optimistic.** Any production edge (a low-confidence call, a
//!   dynamic or type-only import) keeps a node alive for dead-code detectors.
//! - **Violations are pessimistic.** Only runtime import edges between
//!   authored files count as evidence for cycles, hubs, and layering.
//!
//! Test code (test files, Rust `#[cfg(test)]` items and the modules they
//! mount) is never production evidence: its edges carry `test`.
//!
//! Generated, bundled, vendored, and declaration files are never subjects.
use super::super::classify::{
    ROLE_CONFIG, ROLE_DECLARATION, ROLE_NOT_AUTHORED, ROLE_TEST, ROLE_UNPARSED, role_names,
};
use super::format::{
    EdgeKind, FLAG_EXPORTED, FLAG_LOCAL_USE, FLAG_TEST, GraphTables, NONE, NodeKind,
};
use crate::tools::ast_graph::algorithms::Cycles;
use crate::tools::ast_graph::edges::{Load, RUST_MODULE, load};
use crate::tools::ast_graph::graph::{dirname, rust_module_child_dir};
use crate::tools::ast_graph::types::DiagnosticCode;
use octocode_io::shell::quote;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

mod components;
mod context;
mod cycles;
mod dead_code;
mod deps;
mod hubs;
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
pub(super) use primitives::tarjan;
use primitives::*;
use reach::*;
use view::*;

/// Every detector, in run order: a timing label, the detector names it
/// reports, and the step. The one list `--detectors` is checked against.
const STEPS: &[DetectorStep] = &[
    ("cycle", &["cycle"], detect_cycles),
    ("dir-cycle", &["dir-cycle"], detect_dir_cycles),
    (
        "reachability",
        &["unreachable-file", "test-only"],
        detect_reachability,
    ),
    (
        "unused-export",
        &["unused-export", "export-only-local"],
        detect_unused_exports,
    ),
    (
        "dependencies",
        &["undeclared-dependency", "dev-dependency-in-production"],
        detect_dependencies,
    ),
    (
        "boundary-violation",
        &["boundary-violation"],
        detect_boundaries,
    ),
    (
        "unresolved-import",
        &["unresolved-import"],
        detect_unresolved,
    ),
    ("god-file", &["god-file"], detect_hubs),
    ("critical-file", &["critical-file"], detect_critical),
    (
        "single-point-of-failure",
        &["single-point-of-failure"],
        detect_spof,
    ),
    (
        "components",
        &["unstable-dependency", "main-sequence"],
        detect_components,
    ),
    ("misplaced-file", &["misplaced-file"], detect_misplaced),
];

/// Languages whose named imports link to declarations, so an export no
/// importer names can be judged unused. Rust, Go, Java and C items are
/// reached through receivers, packages and paths the syntax does not link:
/// their exports are not judged one by one (their files still are).
const NAMED_IMPORT_LANGUAGES: &[&str] = &["typescript", "tsx", "javascript", "python"];

/// Whether an export of `file` of declaration `kind` can be judged unused.
/// Python has no export syntax, so only its functions and classes are API.
fn exports_judged(file: &str, kind: &str) -> bool {
    let language = super::tables::canonical_language(file, "");
    NAMED_IMPORT_LANGUAGES.contains(&language.as_str())
        && (language != "python" || matches!(kind, "function" | "class"))
}

const PACKAGE_SCOPED: &[&str] = &["go", "java", "scala", "csharp"];
/// Declaration kinds (of [`octocode_engine::signatures::DECLARATION_KINDS`])
/// that declare a contract, not an implementation.
const ABSTRACT_KINDS: &[&str] = &["interface", "trait", "type"];

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
    pub verify: Vec<String>,
    pub corroborated_by: Vec<&'static str>,
    /// vulture-style certainty: 100 nothing references it anywhere, 90 no
    /// graph or code reference (bare prose words go to
    /// `evidence.textMentions`), 60 the file path or every unused export is
    /// referenced in code or qualified text (`evidence.mentionedIn`), or no
    /// mention scan ran. The default view shows 90+.
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
        if !self.verify.is_empty() {
            out["verify"] = json!(self.verify);
        }
        out
    }
}

pub(crate) struct Report {
    pub findings: Vec<Finding>,
    pub summary: Value,
}

fn stable_id(detector: &str, parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
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

/// Runs the selected detectors (all when `only` is empty) and ranks them.
/// With `root`, dead-code findings are re-tiered by a repo-wide mention scan.
pub(crate) fn run(
    t: &GraphTables,
    only: &[String],
    root: Option<&std::path::Path>,
    entries: Option<&[String]>,
) -> Result<Report, String> {
    let names = || STEPS.iter().flat_map(|(_, names, _)| names.iter().copied());
    for name in only {
        if !names().any(|known| known == name) {
            return Err(format!(
                "unknown detector {name:?}; expected one of: {}",
                names().collect::<Vec<_>>().join(", ")
            ));
        }
    }
    let wants = |name: &str| only.is_empty() || only.iter().any(|o| o == name);
    let mut c = Ctx {
        v: View::new(t, entries),
        findings: Vec::new(),
        notes: Vec::new(),
    };
    let mut timings = BTreeMap::<&str, u64>::new();
    for &(label, detectors, step) in STEPS {
        if detectors.iter().any(|d| wants(d)) {
            let started = std::time::Instant::now();
            step(&mut c);
            timings.insert(label, started.elapsed().as_millis() as u64);
        }
    }
    c.findings.retain(|f| wants(f.detector));
    if let Some(root) = root {
        let started = std::time::Instant::now();
        tier_dead_code(&mut c.findings, root, &mut c.notes);
        timings.insert("mention-scan", started.elapsed().as_millis() as u64);
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
    let health_started = std::time::Instant::now();
    let health = health(&c.v);
    timings.insert("health", health_started.elapsed().as_millis() as u64);
    let summary = json!({
        "findings": merged.len(),
        "byDetector": by_detector,
        "entrypoints": {"total": c.v.entries.len(), "byRule": entries},
        "fileRoles": roles,
        "health": health,
        "detectorMs": timings,
        "notes": c.notes,
        "method": "hypotheses ranked by severity × confidence × (0.5 + 0.5 × PageRank percentile); verify before acting",
    });
    Ok(Report {
        findings: merged,
        summary,
    })
}

#[cfg(test)]
mod tests;
