//! Named argument references use an explicit lexical premise, never a callee premise.
use super::super::format::arguments::{ArgumentEvidence, Observation, StoredSpan};
use super::*;
use octocode_engine::graph::{
    GraphArgumentBinding, GraphArgumentGroupObservation, GraphArgumentObservation,
    GraphParseAssessment,
};
use octocode_engine::text::analysis::ObservedSpan;

pub(super) struct ArgumentResult {
    src: u32,
    occurrence: String,
    range: ObservedSpan,
    line: u32,
    metadata: ArgumentEvidence<ObservedSpan>,
    targets: Vec<u32>,
    detail: &'static str,
    confidence: Confidence,
    /// Set for an unextracted argument group: what it may name.
    unextracted: Option<UnextractedGroup>,
}

/// The identifiers written inside one unextracted argument group; `None`
/// when the extractor did not list them, so the group may name anything.
struct UnextractedGroup(Option<Vec<String>>);

impl ArgumentResult {
    fn apply_binding(
        &mut self,
        binding: Result<(Vec<u32>, &'static str, Confidence), &'static str>,
    ) {
        match binding {
            Ok((targets, detail, confidence)) => {
                self.targets = targets;
                self.detail = detail;
                self.confidence = confidence;
            }
            Err(reason) => self.detail = reason,
        }
    }
}

pub(super) fn link_arguments(
    linker: &Linker,
    unit: &FileLinks,
    facts: &FileFacts,
    accept: &dyn Fn(u32) -> bool,
    lexical_scope: &dyn Fn(u32) -> bool,
    output: &mut Vec<ArgumentResult>,
) {
    for site in &facts.argument_sites {
        let src = site
            .caller_id
            .as_deref()
            .and_then(|id| unit.index.decl_nodes.get(id).copied())
            .unwrap_or(unit.file_id);
        for group in &site.groups {
            let base = ArgumentEvidence {
                recipient: site.id.clone(),
                recipient_range: site.source_range,
                group: group.index,
                assessment: site.parse_assessment.clone(),
                observation: Observation::UnsupportedGroup {
                    construct: String::new(),
                },
            };
            let slots = match &group.observation {
                GraphArgumentGroupObservation::Unsupported { construct, names } => {
                    output.push(ArgumentResult {
                        src,
                        occurrence: format!("{}:group:{}", site.id, group.index),
                        range: group.source_range,
                        line: group.range.start.line + 1,
                        metadata: ArgumentEvidence {
                            observation: Observation::UnsupportedGroup {
                                construct: construct.clone(),
                            },
                            ..base
                        },
                        targets: Vec::new(),
                        detail: super::UNSUPPORTED_ARGUMENT_GROUP,
                        confidence: Confidence::Low,
                        unextracted: Some(UnextractedGroup(names.clone())),
                    });
                    continue;
                }
                GraphArgumentGroupObservation::Slots { slots } => slots,
            };
            for slot in slots {
                let mut result = ArgumentResult {
                    src,
                    occurrence: slot.id.clone(),
                    range: slot.source_range,
                    line: slot.range.start.line + 1,
                    metadata: base.clone(),
                    targets: Vec::new(),
                    detail: "scope-not-assessed",
                    confidence: Confidence::Low,
                    unextracted: None,
                };
                match &slot.observation {
                    GraphArgumentObservation::NonReference { .. } => continue,
                    GraphArgumentObservation::Unsupported { construct } => {
                        result.metadata.observation = Observation::UnsupportedSlot {
                            index: slot.index,
                            label: slot.label.clone(),
                            construct: construct.clone(),
                        };
                        result.detail = "unsupported-argument-form";
                    }
                    GraphArgumentObservation::Reference {
                        target_name,
                        form,
                        selection_byte_range,
                        binding,
                        ..
                    } => {
                        let mut blocker = match binding {
                            GraphArgumentBinding::Unrepresented { source_range } => {
                                Some(*source_range)
                            }
                            _ => None,
                        };
                        if matches!(
                            site.parse_assessment,
                            GraphParseAssessment::Recovered { .. }
                        ) {
                            result.detail = "recovered-occurrence";
                        } else {
                            match binding {
                                GraphArgumentBinding::Declaration { declaration_id } => {
                                    if let Some(target) = unit
                                        .index
                                        .decl_nodes
                                        .get(declaration_id.as_str())
                                        .copied()
                                        .filter(|id| accept(*id))
                                    {
                                        result.targets.push(target);
                                        result.detail = "lexical-declaration";
                                        result.confidence = Confidence::High;
                                    } else if let Some(range) = facts
                                        .declarations
                                        .iter()
                                        .find(|decl| decl.id == *declaration_id)
                                        .and_then(|decl| decl.source_range)
                                    {
                                        blocker = Some(range);
                                        result.detail = "binding-unrepresented";
                                    }
                                }
                                GraphArgumentBinding::Unrepresented { .. } => {
                                    result.detail = "binding-unrepresented";
                                }
                                GraphArgumentBinding::NotAssessed => {}
                                GraphArgumentBinding::Import { import_id } => {
                                    result.apply_binding(argument_bindings::imported(
                                        linker,
                                        facts,
                                        import_id,
                                        target_name,
                                        accept,
                                    ));
                                }
                                GraphArgumentBinding::ScopedLookup { implicit_members } => {
                                    result.apply_binding(argument_bindings::scoped(
                                        linker,
                                        unit,
                                        src,
                                        target_name,
                                        *implicit_members,
                                        accept,
                                        lexical_scope,
                                    ));
                                }
                            }
                        }
                        result.metadata.observation = Observation::Reference {
                            index: slot.index,
                            label: slot.label.clone(),
                            target: target_name.clone(),
                            form: *form,
                            selection: *selection_byte_range,
                            blocker,
                        };
                    }
                }
                output.push(result);
            }
        }
    }
}

/// What a file's unextracted argument groups may name, by file node: the
/// union of their identifiers, `None` once any group may name anything.
pub(crate) type UnextractedNames = Option<BTreeSet<String>>;

/// The most declared names kept for one file: past it the file may name
/// anything, as a file with an unlisted group does.
const MAX_FILE_NAMES: usize = 256;

impl Projector<'_> {
    /// Adds `group`'s names to its file. A name an import renames
    /// (`use a::f as g`, `g` written in the group) also counts as the name
    /// it renames, the one a changed declaration has.
    fn note_unextracted(&mut self, src: u32, group: UnextractedGroup) {
        let file = match self.nodes[src as usize].kind {
            NodeKind::File => src,
            _ => self.nodes[src as usize].file,
        };
        if file == NONE {
            return;
        }
        let names = group.0.map(|mut names| {
            let path = self.strings.strings[self.nodes[file as usize].key as usize].as_str();
            if let Some(facts) = self.built.facts.get(path) {
                for import in &facts.imports {
                    let renamed = import.local_name.as_deref();
                    if renamed.is_some_and(|local| names.iter().any(|name| name == local)) {
                        names.push(import.imported_name.clone());
                    }
                }
            }
            names
        });
        let entry = self
            .unextracted
            .entry(file)
            .or_insert_with(|| Some(BTreeSet::new()));
        match (entry.as_mut(), names) {
            (Some(known), Some(names)) => known.extend(names),
            _ => *entry = None,
        }
    }

    pub(super) fn push_arguments(&mut self, arguments: Vec<ArgumentResult>) {
        for result in arguments {
            if let Some(group) = result.unextracted {
                self.note_unextracted(result.src, group);
            }
            let file = &self.nodes[self.nodes[result.src as usize].file as usize];
            let path = &self.strings.strings[file.key as usize];
            let metadata = result.metadata.map_spans(|span| StoredSpan {
                range: span.range,
                origin: self.sources.origin(path, Some(span)),
            });
            let argument = self.strings.json(&metadata);
            let occurrence = self.strings.id(&result.occurrence);
            let provenance = self.provenance(result.src, Some(&result.range));
            if result.targets.is_empty() {
                let target = match &metadata.observation {
                    Observation::Reference { target, .. } => target.as_str(),
                    _ => "",
                };
                self.unresolved.push(UnresolvedRec {
                    argument,
                    provenance,
                    relation: EdgeKind::Passes,
                    src: result.src,
                    occurrence,
                    callee: self.strings.id(target),
                    line: result.line,
                    reason: self.strings.id(result.detail),
                });
            } else {
                for dst in result.targets {
                    self.push(
                        result.src,
                        dst,
                        EdgeKind::Passes,
                        result.confidence,
                        result.detail,
                        result.line,
                    );
                    if let Some(witness) = self.witnesses.last_mut() {
                        witness.argument = argument;
                        witness.occurrence = occurrence;
                        witness.provenance = provenance;
                    }
                }
            }
        }
    }
}

/// `unextracted` reduced to the names a changed declaration can have: those
/// some symbol of the snapshot is named, capped per file.
pub(super) fn declared_unextracted(
    unextracted: BTreeMap<u32, UnextractedNames>,
    tables: &GraphTables,
) -> BTreeMap<u32, UnextractedNames> {
    let declared = tables
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Symbol)
        .map(|node| super::super::manifest::spelled_name(tables.str(node.name)))
        .collect::<HashSet<_>>();
    unextracted
        .into_iter()
        .map(|(file, names)| {
            let names = names
                .map(|names| {
                    names
                        .into_iter()
                        .filter(|name| declared.contains(name.as_str()))
                        .collect::<BTreeSet<_>>()
                })
                .filter(|names| names.len() <= MAX_FILE_NAMES);
            (file, names)
        })
        .collect()
}
