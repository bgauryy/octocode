//! Source-observed argument references. Passing a value never implies invocation.

use serde::{Deserialize, Serialize};

use super::{ByteRange, GraphParseAssessment, GraphRange, ObservedSpan};

pub const ARGUMENT_DERIVATION: &str = "syntax-argument-reference:1";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum GraphArgumentForm {
    Identifier,
    Qualified,
    Member,
    AddressOf,
    Borrow,
    MethodReference,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum GraphArgumentValueKind {
    Literal,
    CallResult,
    Closure,
    ValueExpression,
}

/// Only `ScopedLookup` permits a name-based candidate lookup. An observed
/// binding that is not represented in the declaration inventory blocks it.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum GraphArgumentBinding {
    Declaration { declaration_id: String },
    Import { import_id: String },
    ScopedLookup { implicit_members: bool },
    Unrepresented { source_range: ObservedSpan },
    NotAssessed,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum GraphArgumentObservation {
    Reference {
        target_name: String,
        form: GraphArgumentForm,
        selection_range: GraphRange,
        selection_byte_range: ObservedSpan,
        binding: GraphArgumentBinding,
    },
    NonReference {
        value_kind: GraphArgumentValueKind,
    },
    Unsupported {
        construct: String,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GraphArgumentSlot {
    pub id: String,
    pub index: u32,
    pub label: Option<String>,
    pub range: GraphRange,
    pub source_range: ObservedSpan,
    pub observation: GraphArgumentObservation,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum GraphArgumentGroupObservation {
    Slots { slots: Vec<GraphArgumentSlot> },
    Unsupported { construct: String },
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GraphArgumentGroup {
    pub index: u32,
    pub range: GraphRange,
    pub source_range: ObservedSpan,
    pub observation: GraphArgumentGroupObservation,
}

/// A recipient expression has its own occurrence even when its name and
/// relationship to a named call fact cannot be established.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GraphFactArgumentSite {
    pub id: String,
    pub caller: String,
    pub caller_id: Option<String>,
    pub call_id: Option<String>,
    pub recipient_name: Option<String>,
    pub range: GraphRange,
    pub source_range: ObservedSpan,
    pub parse_assessment: GraphParseAssessment,
    pub groups: Vec<GraphArgumentGroup>,
}

impl GraphFactArgumentSite {
    pub fn observed_spans(&self) -> impl Iterator<Item = ObservedSpan> + '_ {
        std::iter::once(self.source_range).chain(self.groups.iter().flat_map(|group| {
            let slots = match &group.observation {
                GraphArgumentGroupObservation::Slots { slots } => slots.as_slice(),
                _ => &[],
            };
            std::iter::once(group.source_range).chain(slots.iter().flat_map(|slot| {
                let (selection, binding) = match &slot.observation {
                    GraphArgumentObservation::Reference {
                        selection_byte_range,
                        binding,
                        ..
                    } => (
                        Some(*selection_byte_range),
                        match binding {
                            GraphArgumentBinding::Unrepresented { source_range } => {
                                Some(*source_range)
                            }
                            _ => None,
                        },
                    ),
                    _ => (None, None),
                };
                [Some(slot.source_range), selection, binding]
                    .into_iter()
                    .flatten()
            }))
        }))
    }

    pub(crate) fn set_unit(&mut self, unit: crate::text::analysis::UnitHandle) {
        self.source_range.unit = unit;
        for group in &mut self.groups {
            group.source_range.unit = unit;
            if let GraphArgumentGroupObservation::Slots { slots } = &mut group.observation {
                for slot in slots {
                    slot.source_range.unit = unit;
                    if let GraphArgumentObservation::Reference {
                        selection_byte_range,
                        ..
                    } = &mut slot.observation
                    {
                        selection_byte_range.unit = unit;
                    }
                }
            }
        }
    }
    pub(crate) fn heap_bytes(&self) -> usize {
        let optional = |value: &Option<String>| value.as_ref().map_or(0, String::capacity);
        std::mem::size_of::<Self>()
            + self.id.capacity()
            + self.caller.capacity()
            + optional(&self.caller_id)
            + optional(&self.call_id)
            + optional(&self.recipient_name)
            + self.groups.capacity() * std::mem::size_of::<GraphArgumentGroup>()
            + self
                .groups
                .iter()
                .map(|group| match &group.observation {
                    GraphArgumentGroupObservation::Unsupported { construct } => {
                        construct.capacity()
                    }
                    GraphArgumentGroupObservation::Slots { slots } => {
                        slots.capacity() * std::mem::size_of::<GraphArgumentSlot>()
                            + slots
                                .iter()
                                .map(|slot| {
                                    slot.id.capacity()
                                        + optional(&slot.label)
                                        + match &slot.observation {
                                            GraphArgumentObservation::Reference {
                                                target_name,
                                                binding,
                                                ..
                                            } => {
                                                target_name.capacity()
                                                    + match binding {
                                                        GraphArgumentBinding::Declaration {
                                                            declaration_id,
                                                        } => declaration_id.capacity(),
                                                        GraphArgumentBinding::Import {
                                                            import_id,
                                                        } => import_id.capacity(),
                                                        _ => 0,
                                                    }
                                            }
                                            GraphArgumentObservation::Unsupported { construct } => {
                                                construct.capacity()
                                            }
                                            GraphArgumentObservation::NonReference { .. } => 0,
                                        }
                                })
                                .sum::<usize>()
                    }
                })
                .sum::<usize>()
    }
}

/// An occurrence identifies the exact decoded input and derivation, not a
/// canonical binding. Roles distinguish equal spans serving different purposes.
pub(crate) fn argument_occurrence_id(
    file: &str,
    digest: &str,
    range: &ByteRange,
    role: &str,
) -> String {
    format!(
        "{ARGUMENT_DERIVATION}:{file}@{digest}:{}:{}:{role}",
        range.start_byte, range.end_byte
    )
}
