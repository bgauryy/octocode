//! The detector run context and how findings are drafted into it.
use super::*;

// ── Detectors ───────────────────────────────────────────────────────────────

/// `(timing label, detectors it produces, runner)`.
pub(super) type DetectorStep = (&'static str, &'static [&'static str], fn(&mut Ctx));

pub(super) struct Ctx<'a> {
    pub(super) v: View<'a>,
    pub(super) findings: Vec<Finding>,
    pub(super) notes: Vec<Value>,
}

/// A finding prepared while the view was borrowed: detector, subject, title,
/// severity, confidence, evidence, controls.
pub(super) type Pending = (
    &'static str,
    String,
    String,
    f64,
    f64,
    Value,
    Vec<&'static str>,
);

/// Push prepared findings at the default impact, with no extra id parts or
/// verify steps.
pub(super) fn push_pending(out: &mut Vec<Finding>, pending: Vec<Pending>) {
    for (detector, subject, title, severity, confidence, evidence, controls) in pending {
        push(
            out,
            Draft {
                detector,
                subject,
                id_parts: &[],
                title,
                severity,
                confidence,
                impact: 0.5,
                evidence,
                controls,
                verify: Vec::new(),
            },
        );
    }
}

/// A finding before its id and score are derived.
pub(super) struct Draft<'a> {
    pub(super) detector: &'static str,
    pub(super) subject: String,
    /// Parts besides the subject that make the finding's id unique.
    pub(super) id_parts: &'a [&'a str],
    pub(super) title: String,
    pub(super) severity: f64,
    pub(super) confidence: f64,
    pub(super) impact: f64,
    pub(super) evidence: Value,
    pub(super) controls: Vec<&'static str>,
    pub(super) verify: Vec<String>,
}

pub(super) fn push(out: &mut Vec<Finding>, draft: Draft<'_>) {
    let Draft {
        detector,
        subject,
        id_parts,
        title,
        severity,
        confidence,
        impact,
        evidence,
        controls,
        verify,
    } = draft;
    let mut parts = vec![subject.as_str()];
    parts.extend(id_parts);
    let id = stable_id(detector, &parts);
    out.push(Finding {
        detector,
        id,
        subject,
        title,
        severity,
        confidence,
        impact,
        score: severity * confidence * (0.5 + 0.5 * impact),
        evidence,
        controls,
        verify,
        corroborated_by: Vec::new(),
        tier: 90,
    });
}

pub(super) fn note(notes: &mut Vec<Value>, detector: &str, reason: &str) {
    notes.push(json!({"detector": detector, "skipped": reason}));
}
