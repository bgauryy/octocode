use super::*;
use crate::execution::cancel::CancellationCheck;
use crate::response::result::ToolError;
use crate::tools::ast_graph::graph::capture;
use octocode_engine::text::analysis::{ObservedSpan, SourceDescriptors};

pub(super) struct Sources {
    pub table: super::super::format::sources::SourceTable,
    indexes: BTreeMap<String, BTreeMap<(u32, u32, u32), u32>>,
}

impl Sources {
    /// The snapshot's source table, built from each file's captured source.
    /// The captured sources move out of `built` and are freed once their
    /// origins are encoded into the table: nothing after projection reads
    /// them, and they would otherwise stay decoded until publication.
    pub(super) fn capture(
        built: &mut BuiltGraph,
        cancel: &dyn CancellationCheck,
    ) -> Result<Self, ToolError> {
        let mut output = Self {
            table: super::super::format::sources::SourceTable::default(),
            indexes: BTreeMap::new(),
        };
        let mut snapshots = BTreeMap::new();
        let mut representations = BTreeMap::new();
        let mut units = BTreeMap::new();
        let mut origin_count = 0_usize;
        // Each file's catalog is checked on its own, so the checks run in
        // parallel; the first failure in file order is the one reported.
        let mut files = built.facts.values().collect::<Vec<_>>();
        let width = crate::tools::workers::workers(files.len(), None);
        crate::tools::workers::each_parallel(&mut files, width, |facts| {
            crate::response::source_evidence::validate_catalog(&facts.source.descriptors, cancel)
        })
        .into_iter()
        .collect::<Result<(), _>>()?;
        let captured = built
            .facts
            .iter_mut()
            .map(|(file, facts)| (file.as_str(), std::mem::take(&mut facts.source)))
            .collect::<Vec<_>>();
        for (file, source) in &captured {
            cancel.check().map_err(ToolError::cancelled)?;
            let descriptors = &source.descriptors;
            for source in &descriptors.snapshots {
                insert(&mut snapshots, &source.id, source)?;
            }
            for representation in &descriptors.representations {
                insert(&mut representations, &representation.id, representation)?;
            }
            for unit in &descriptors.units {
                insert(&mut units, &unit.id, unit)?;
            }
            let mut index = BTreeMap::new();
            for span in source.origins.keys() {
                cancel.check().map_err(ToolError::cancelled)?;
                let id = u32::try_from(origin_count)
                    .ok()
                    .filter(|id| *id != NONE)
                    .ok_or_else(|| {
                        ToolError::new(
                            octocode_contract_types::ErrorCode::ScopeTooBroad,
                            "graph has too many source origins",
                        )
                    })?;
                index.insert(*span, id);
                origin_count += 1;
            }
            output.indexes.insert((*file).to_owned(), index);
        }
        let descriptors = SourceDescriptors {
            snapshots: snapshots.into_values().cloned().collect(),
            representations: representations.into_values().cloned().collect(),
            units: units.into_values().cloned().collect(),
        };
        output.table = super::super::format::sources::SourceTable::from_origins(
            descriptors,
            captured
                .iter()
                .flat_map(|(_, source)| source.origins.values()),
            cancel,
        )
        .map_err(ToolError::source)?;
        Ok(output)
    }

    pub(super) fn origin(&self, file: &str, span: Option<ObservedSpan>) -> u32 {
        span.and_then(|span| self.indexes.get(file)?.get(&capture::key(span)).copied())
            .unwrap_or(NONE)
    }

    pub(super) fn digest(&self, origin: u32) -> Option<&str> {
        self.table.digest(origin)
    }
}

fn insert<'a, T: PartialEq>(
    map: &mut BTreeMap<&'a str, &'a T>,
    id: &'a str,
    value: &'a T,
) -> Result<(), ToolError> {
    if map
        .insert(id, value)
        .is_some_and(|previous| previous != value)
    {
        return Err(ToolError::new(
            octocode_contract_types::ErrorCode::ExecutionFailed,
            "graph source descriptors reuse an identity for different records",
        ));
    }
    Ok(())
}
