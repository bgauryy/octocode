//! Component-boundary regression policy over a code-graph snapshot.
//!
//! Implements the ArchUnit-style discipline the improvement plan calls for:
//! classify files into components, evaluate named forbidden-edge rules, then
//! compare the result against a frozen baseline so CI fails only on *new*
//! error-level violations while known ones stay reviewed and reintroductions
//! are re-detected.
//!
//! Violation identity is **stable and structural** — `(rule id, from file,
//! edge kind, to file)`. It never includes evidence ranges or line numbers, so
//! a line shift never turns a known violation into a spurious new one.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use super::model::{CodeGraphSnapshot, EdgeKind, NodeId};

/// Rule severity. Only `Error`-level *new* violations block CI.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Error,
    Warn,
}

/// Maps a file path prefix to a component name. Rules are evaluated in order;
/// the first match wins, so place more specific prefixes first.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ComponentRule {
    pub prefix: String,
    pub component: String,
}

/// A named forbidden-edge rule between components. `"*"` matches any component.
/// An empty `edge_kinds` matches any edge kind.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BoundaryRule {
    pub id: String,
    pub severity: Severity,
    pub from_component: String,
    pub to_component: String,
    #[serde(default)]
    pub edge_kinds: Vec<EdgeKind>,
}

/// A concrete forbidden edge found in the graph.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "camelCase")]
pub struct BoundaryViolation {
    pub rule_id: String,
    pub severity: Severity,
    pub from_component: String,
    pub to_component: String,
    pub from_file: String,
    pub to_file: String,
    pub edge_kind: EdgeKind,
}

impl BoundaryViolation {
    /// Stable identity used for baseline comparison. Deliberately excludes
    /// severity and component labels (which can be re-derived) and any range,
    /// so freezing survives rule metadata edits and line shifts.
    pub fn fingerprint(&self) -> String {
        format!(
            "{}\u{1f}{}\u{1f}{}\u{1f}{}",
            self.rule_id,
            self.from_file,
            edge_kind_token(&self.edge_kind),
            self.to_file,
        )
    }
}

fn edge_kind_token(kind: &EdgeKind) -> String {
    match kind {
        EdgeKind::Syntactic(name) => format!("syntactic:{name}"),
        other => format!("{other:?}"),
    }
}

fn file_of(id: &NodeId) -> Option<&str> {
    id.0.strip_prefix("file:")
}

/// Classify a file path into a component using ordered prefix rules.
pub fn classify<'a>(path: &str, rules: &'a [ComponentRule]) -> Option<&'a str> {
    rules
        .iter()
        .find(|rule| path == rule.prefix || path.starts_with(&format!("{}/", rule.prefix)))
        .map(|rule| rule.component.as_str())
}

fn component_matches(pattern: &str, component: &str) -> bool {
    pattern == "*" || pattern == component
}

fn kind_matches(rule: &BoundaryRule, kind: &EdgeKind) -> bool {
    rule.edge_kinds.is_empty() || rule.edge_kinds.iter().any(|allowed| allowed == kind)
}

/// Evaluate all boundary rules against a snapshot's file-level edges.
/// Output is sorted and deterministic.
pub fn evaluate(
    snapshot: &CodeGraphSnapshot,
    components: &[ComponentRule],
    rules: &[BoundaryRule],
) -> Vec<BoundaryViolation> {
    let mut violations: BTreeSet<BoundaryViolation> = BTreeSet::new();
    for edge in snapshot.edges.values() {
        let (Some(from_file), Some(to_file)) = (file_of(&edge.from), file_of(&edge.to)) else {
            continue;
        };
        let from_component = classify(from_file, components);
        let to_component = classify(to_file, components);
        for rule in rules {
            let (Some(from_c), Some(to_c)) = (from_component, to_component) else {
                continue;
            };
            if component_matches(&rule.from_component, from_c)
                && component_matches(&rule.to_component, to_c)
                && kind_matches(rule, &edge.kind)
            {
                violations.insert(BoundaryViolation {
                    rule_id: rule.id.clone(),
                    severity: rule.severity,
                    from_component: from_c.to_owned(),
                    to_component: to_c.to_owned(),
                    from_file: from_file.to_owned(),
                    to_file: to_file.to_owned(),
                    edge_kind: edge.kind.clone(),
                });
            }
        }
    }
    violations.into_iter().collect()
}

/// The result of comparing current violations against a frozen baseline.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BaselineReport {
    /// Present now but not in the baseline — includes reintroductions.
    pub new_violations: Vec<BoundaryViolation>,
    /// Present now and in the baseline — reviewed, non-blocking.
    pub known_violations: Vec<BoundaryViolation>,
    /// In the baseline but gone now — should be pruned from the baseline.
    pub resolved_fingerprints: Vec<String>,
}

impl BaselineReport {
    /// CI gate: block only on *new* error-level violations. Known ones and
    /// warnings never block; resolved ones are informational.
    pub fn has_blocking(&self) -> bool {
        self.new_violations
            .iter()
            .any(|violation| violation.severity == Severity::Error)
    }
}

/// Compare current violations against a frozen baseline of fingerprints.
/// A baseline is never created or rewritten here — callers own that lifecycle.
pub fn compare_to_baseline(
    current: &[BoundaryViolation],
    baseline: &BTreeSet<String>,
) -> BaselineReport {
    let mut report = BaselineReport::default();
    let mut current_fingerprints: BTreeSet<String> = BTreeSet::new();
    for violation in current {
        let fingerprint = violation.fingerprint();
        current_fingerprints.insert(fingerprint.clone());
        if baseline.contains(&fingerprint) {
            report.known_violations.push(violation.clone());
        } else {
            report.new_violations.push(violation.clone());
        }
    }
    report.resolved_fingerprints = baseline
        .iter()
        .filter(|fingerprint| !current_fingerprints.contains(*fingerprint))
        .cloned()
        .collect();
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::CodeGraphBuilder;

    fn snapshot(edges: &[(&str, &str, &str, u32)]) -> CodeGraphSnapshot {
        let mut builder = CodeGraphBuilder::new("/workspace", 1);
        let mut files: BTreeSet<&str> = BTreeSet::new();
        for (from, to, _, _) in edges {
            files.insert(from);
            files.insert(to);
        }
        for file in &files {
            builder.add_file(file, "x").expect("file");
        }
        for (from, to, relation, line) in edges {
            builder
                .add_file_relation(from, to, relation, *line)
                .expect("relation");
        }
        builder.finish()
    }

    fn components() -> Vec<ComponentRule> {
        vec![
            ComponentRule {
                prefix: "brain".to_owned(),
                component: "brain".to_owned(),
            },
            ComponentRule {
                prefix: "interface".to_owned(),
                component: "interface".to_owned(),
            },
        ]
    }

    fn forbid_brain_to_interface() -> BoundaryRule {
        BoundaryRule {
            id: "brain-must-not-depend-on-interface".to_owned(),
            severity: Severity::Error,
            from_component: "brain".to_owned(),
            to_component: "interface".to_owned(),
            edge_kinds: Vec::new(),
        }
    }

    #[test]
    fn forbidden_cross_component_edge_is_a_violation() {
        let graph = snapshot(&[("brain/core.rs", "interface/cli.rs", "rust-use", 1)]);
        let violations = evaluate(&graph, &components(), &[forbid_brain_to_interface()]);

        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].from_component, "brain");
        assert_eq!(violations[0].to_component, "interface");
        // The allowed direction is not a violation.
        let reverse = snapshot(&[("interface/cli.rs", "brain/core.rs", "rust-use", 1)]);
        assert!(evaluate(&reverse, &components(), &[forbid_brain_to_interface()]).is_empty());
    }

    #[test]
    fn known_baseline_violation_does_not_block_but_new_one_does() {
        let graph = snapshot(&[
            ("brain/a.rs", "interface/x.rs", "rust-use", 1),
            ("brain/b.rs", "interface/y.rs", "rust-use", 1),
        ]);
        let violations = evaluate(&graph, &components(), &[forbid_brain_to_interface()]);
        // Freeze only the first violation.
        let baseline: BTreeSet<String> = [violations[0].fingerprint()].into_iter().collect();

        let report = compare_to_baseline(&violations, &baseline);
        assert_eq!(report.known_violations.len(), 1);
        assert_eq!(report.new_violations.len(), 1);
        assert!(
            report.has_blocking(),
            "a new error-level violation blocks CI"
        );
    }

    #[test]
    fn fully_frozen_violations_do_not_block() {
        let graph = snapshot(&[("brain/a.rs", "interface/x.rs", "rust-use", 1)]);
        let violations = evaluate(&graph, &components(), &[forbid_brain_to_interface()]);
        let baseline: BTreeSet<String> = violations
            .iter()
            .map(BoundaryViolation::fingerprint)
            .collect();

        let report = compare_to_baseline(&violations, &baseline);
        assert!(report.new_violations.is_empty());
        assert_eq!(report.known_violations.len(), 1);
        assert!(!report.has_blocking());
    }

    #[test]
    fn resolved_violation_is_reported_for_baseline_pruning() {
        let baseline: BTreeSet<String> =
            ["stale-rule\u{1f}brain/gone.rs\u{1f}Imports\u{1f}interface/x.rs".to_owned()]
                .into_iter()
                .collect();
        let report = compare_to_baseline(&[], &baseline);

        assert_eq!(report.resolved_fingerprints.len(), 1);
        assert!(report.new_violations.is_empty());
        assert!(!report.has_blocking());
    }

    #[test]
    fn violation_fingerprint_ignores_line_shifts() {
        let base = snapshot(&[("brain/a.rs", "interface/x.rs", "rust-use", 1)]);
        let shifted = snapshot(&[("brain/a.rs", "interface/x.rs", "rust-use", 42)]);
        let rules = [forbid_brain_to_interface()];
        let a = evaluate(&base, &components(), &rules);
        let b = evaluate(&shifted, &components(), &rules);

        assert_eq!(a[0].fingerprint(), b[0].fingerprint());
        // A frozen baseline from `a` still recognizes `b` as known, not new.
        let baseline: BTreeSet<String> = [a[0].fingerprint()].into_iter().collect();
        assert!(compare_to_baseline(&b, &baseline).new_violations.is_empty());
    }

    #[test]
    fn edge_kind_filter_scopes_the_rule() {
        let graph = snapshot(&[("brain/a.rs", "interface/x.rs", "dynamic-import", 1)]);
        let mut rule = forbid_brain_to_interface();
        rule.edge_kinds = vec![EdgeKind::Imports];
        // The edge is DynamicImport, so an Imports-only rule does not match.
        assert!(evaluate(&graph, &components(), &[rule]).is_empty());
    }
}
