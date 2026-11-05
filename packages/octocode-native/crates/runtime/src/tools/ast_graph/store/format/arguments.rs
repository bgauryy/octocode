//! Typed argument observations interned independently from topology details.
use super::{EdgeKind, GraphTables, NONE, ProvenanceRec};
use octocode_engine::graph::{GraphArgumentForm, GraphParseAssessment};
use octocode_engine::text::analysis::ByteRange;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ArgumentEvidence<S = StoredSpan> {
    pub recipient: String,
    pub recipient_range: S,
    pub group: u32,
    pub assessment: GraphParseAssessment,
    pub observation: Observation<S>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub(crate) enum Observation<S = StoredSpan> {
    Reference {
        index: u32,
        label: Option<String>,
        target: String,
        form: GraphArgumentForm,
        selection: S,
        blocker: Option<S>,
    },
    UnsupportedSlot {
        index: u32,
        label: Option<String>,
        construct: String,
    },
    UnsupportedGroup {
        construct: String,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoredSpan {
    pub range: ByteRange,
    pub origin: u32,
}

impl<S> ArgumentEvidence<S> {
    pub(crate) fn map_spans<T>(self, mut project: impl FnMut(S) -> T) -> ArgumentEvidence<T> {
        ArgumentEvidence {
            recipient: self.recipient,
            recipient_range: project(self.recipient_range),
            group: self.group,
            assessment: self.assessment,
            observation: match self.observation {
                Observation::Reference {
                    index,
                    label,
                    target,
                    form,
                    selection,
                    blocker,
                } => Observation::Reference {
                    index,
                    label,
                    target,
                    form,
                    selection: project(selection),
                    blocker: blocker.map(project),
                },
                Observation::UnsupportedSlot {
                    index,
                    label,
                    construct,
                } => Observation::UnsupportedSlot {
                    index,
                    label,
                    construct,
                },
                Observation::UnsupportedGroup { construct } => {
                    Observation::UnsupportedGroup { construct }
                }
            },
        }
    }
}

fn within(inner: &ByteRange, outer: &ByteRange) -> bool {
    inner.start_byte <= inner.end_byte
        && outer.start_byte <= inner.start_byte
        && inner.end_byte <= outer.end_byte
}

pub(super) fn validate(
    tables: &GraphTables,
    reference: u32,
    edge: EdgeKind,
    provenance: ProvenanceRec,
    reason: Option<&str>,
) -> Result<(), String> {
    if edge != EdgeKind::Passes {
        return if reference == NONE {
            Ok(())
        } else {
            Err("non-passes evidence has argument metadata".into())
        };
    }
    let evidence: ArgumentEvidence = serde_json::from_str(tables.str(reference))
        .map_err(|_| "passes evidence has invalid argument metadata")?;
    let range = ByteRange {
        start_byte: provenance.source_start,
        end_byte: provenance.source_end,
    };
    if evidence.recipient.is_empty()
        || range.end_byte == NONE
        || (tables
            .sources
            .same_representation(provenance.source_origin, evidence.recipient_range.origin)
            && !within(&range, &evidence.recipient_range.range))
    {
        return Err("passes evidence has invalid recipient or source range".into());
    }
    tables.sources.check_range(
        evidence.recipient_range.origin,
        evidence.recipient_range.range.start_byte,
        evidence.recipient_range.range.end_byte,
    )?;
    let valid = match &evidence.observation {
        Observation::Reference {
            target,
            selection,
            blocker,
            ..
        } => {
            tables.sources.check_range(
                selection.origin,
                selection.range.start_byte,
                selection.range.end_byte,
            )?;
            if let Some(blocker) = blocker {
                tables.sources.check_range(
                    blocker.origin,
                    blocker.range.start_byte,
                    blocker.range.end_byte,
                )?;
            }
            !target.is_empty()
                && (!tables
                    .sources
                    .same_representation(provenance.source_origin, selection.origin)
                    || within(&selection.range, &range))
                && blocker.as_ref().is_none_or(|r| {
                    r.range.start_byte <= r.range.end_byte && r.range.end_byte != NONE
                })
                && (reason != Some("binding-unrepresented") || blocker.is_some())
                && (blocker.is_none()
                    || matches!(
                        reason,
                        Some("binding-unrepresented" | "recovered-occurrence")
                    ))
                && !matches!(
                    reason,
                    Some("unsupported-argument-form" | "unsupported-argument-group")
                )
        }
        Observation::UnsupportedSlot { construct, .. } => {
            !construct.is_empty() && reason == Some("unsupported-argument-form")
        }
        Observation::UnsupportedGroup { construct } => {
            !construct.is_empty() && reason == Some("unsupported-argument-group")
        }
    };
    if !valid {
        return Err("passes evidence has inconsistent argument classification".into());
    }
    Ok(())
}
