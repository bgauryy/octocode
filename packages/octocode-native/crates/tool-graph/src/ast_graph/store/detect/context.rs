//! The detector run context and how findings are drafted into it.
use super::*;

// ── Detectors ───────────────────────────────────────────────────────────────

/// `(timing label, detectors it produces, runner)`.
pub(super) type DetectorStep = (&'static [&'static str], fn(&mut Ctx));

pub(super) struct Ctx<'a> {
    pub(super) v: View<'a>,
    pub(super) findings: Vec<Finding>,
    pub(super) notes: Vec<Value>,
    /// Detectors that judged only file-level files ([`View::file_level`]):
    /// [`note_package_scoped`] names them once for the files they skipped.
    pub(super) file_level: Vec<&'static str>,
}

impl Ctx<'_> {
    /// `detectors` judge only file-level files; the package-scoped ones
    /// they skip are disclosed once, after every detector ran.
    pub(super) fn judges_file_level(&mut self, detectors: &[&'static str]) {
        self.file_level.extend(detectors);
    }
}

/// One note for the Go/JVM/.NET production files every file-level detector
/// the request asked for skipped: package-scoped files reference each other
/// without imports, so their file-level structure is not observable.
pub(super) fn note_package_scoped(c: &mut Ctx, wants: impl Fn(&str) -> bool) {
    let detectors = c
        .file_level
        .iter()
        .copied()
        .filter(|detector| wants(detector))
        .collect::<Vec<_>>();
    let v = &c.v;
    let scoped = v
        .files
        .iter()
        .filter(|f| v.production(**f) && v.package_scoped(**f))
        .count();
    if detectors.is_empty() || scoped == 0 {
        return;
    }
    let reason = format!(
        "{scoped} Go/JVM/.NET file(s) skipped: package-scoped files reference each other without imports"
    );
    match detectors.as_slice() {
        [detector] => note(&mut c.notes, detector, &reason),
        _ => note_detectors(&mut c.notes, &detectors, &reason),
    }
}

/// A finding prepared while the view was borrowed: detector, subject, title,
/// severity, confidence, impact, evidence, controls.
pub(super) type Pending = (
    &'static str,
    String,
    String,
    f64,
    f64,
    f64,
    Value,
    Vec<&'static str>,
);

/// Push prepared findings, with no extra id parts.
pub(super) fn push_pending(out: &mut Vec<Finding>, pending: Vec<Pending>) {
    for (detector, subject, title, severity, confidence, impact, evidence, controls) in pending {
        push(
            out,
            Draft {
                detector,
                subject,
                id_parts: &[],
                title,
                severity,
                confidence,
                impact,
                evidence,
                controls,
            },
        );
    }
}

/// A finding before its id and score are derived. Its score is
/// `severity × confidence × (0.5 + 0.5 × impact)`, where impact is the
/// highest PageRank percentile among the subject's files
/// ([`View::impact`]).
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
        corroborated_by: Vec::new(),
        tier: 90,
    });
}

pub(super) fn note(notes: &mut Vec<Value>, detector: &str, reason: &str) {
    notes.push(json!({"detector": detector, "skipped": reason}));
}

/// One note for files several detectors skip for the same reason.
pub(super) fn note_detectors(notes: &mut Vec<Value>, detectors: &[&str], reason: &str) {
    notes.push(json!({"detectors": detectors, "skipped": reason}));
}
