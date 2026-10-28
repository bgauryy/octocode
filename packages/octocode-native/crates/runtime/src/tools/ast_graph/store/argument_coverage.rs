//! Snapshot-scoped extraction populations, independent from call linkage.
use super::*;
use octocode_engine::graph::{GraphArgumentGroupObservation, GraphArgumentObservation};
use serde_json::Map;

#[derive(Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Counts {
    sites: u64,
    groups: u64,
    slots: u64,
    direct_references: u64,
    non_reference_values: u64,
    unsupported_slots: u64,
    unsupported_groups: u64,
}

pub(super) fn coverage(
    built: &super::super::types::BuiltGraph,
    tables: &format::GraphTables,
) -> Value {
    let mut languages = BTreeMap::<&str, Vec<&str>>::new();
    for node in tables
        .nodes
        .iter()
        .filter(|node| node.kind == format::NodeKind::File)
    {
        languages
            .entry(tables.str(node.detail))
            .or_default()
            .push(tables.str(node.key));
    }
    let mut by_language = Map::new();
    for (language, files) in languages {
        let mut capabilities = octocode_engine::signatures::languages::select(language)
            .map(octocode_engine::signatures::languages::LanguageEntry::argument_capabilities);
        let capability = capabilities.next().filter(|first| {
            capabilities.all(|next| {
                next.supported == first.supported
                    && next.forms == first.forms
                    && next.unsupported_constructs == first.unsupported_constructs
            })
        });
        let Some(capability) = capability else {
            by_language.insert(language.into(), json!({"extraction":"notAssessed"}));
            continue;
        };
        if !capability.supported {
            by_language.insert(language.into(), json!({"extraction":"unsupported"}));
            continue;
        }
        let mut value = json!({"extraction":"supported","referenceForms":capability.forms});
        if files.iter().all(|file| {
            built
                .facts
                .get(*file)
                .is_some_and(|facts| facts.argument_scan_complete)
        }) {
            let mut counts = Counts::default();
            for facts in files.iter().filter_map(|file| built.facts.get(*file)) {
                for site in &facts.argument_sites {
                    counts.sites += 1;
                    for group in &site.groups {
                        counts.groups += 1;
                        match &group.observation {
                            GraphArgumentGroupObservation::Unsupported { .. } => {
                                counts.unsupported_groups += 1;
                            }
                            GraphArgumentGroupObservation::Slots { slots } => {
                                for slot in slots {
                                    counts.slots += 1;
                                    match slot.observation {
                                        GraphArgumentObservation::Reference { .. } => {
                                            counts.direct_references += 1;
                                        }
                                        GraphArgumentObservation::NonReference { .. } => {
                                            counts.non_reference_values += 1;
                                        }
                                        GraphArgumentObservation::Unsupported { .. } => {
                                            counts.unsupported_slots += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            value["counts"] = json!(counts);
        }
        by_language.insert(language.into(), value);
    }
    json!({"scope":"snapshot","population":"syntax-argument-slots","byLanguage":by_language})
}
