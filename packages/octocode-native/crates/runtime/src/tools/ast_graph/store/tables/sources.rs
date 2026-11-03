use super::*;
use crate::execution::cancel::CancellationCheck;
use crate::response::result::ToolError;
use crate::tools::ast_graph::graph::capture;
use octocode_engine::text::analysis::{ObservedSpan, SourceDescriptors};

/// One file's origins: its first origin reference and their span keys,
/// ascending; the origin at position `n` is reference `first + n`.
type FileOrigins = (u32, Vec<(u32, u32, u32)>);

pub(super) struct Sources {
    pub table: super::super::format::sources::SourceTable,
    indexes: BTreeMap<String, FileOrigins>,
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
        let mut captured = built
            .facts
            .iter_mut()
            .map(|(file, facts)| (file.as_str(), std::mem::take(&mut facts.source)))
            .collect::<Vec<_>>();
        for (file, source) in &mut captured {
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
            let too_many = || {
                ToolError::new(
                    octocode_contract_types::ErrorCode::ScopeTooBroad,
                    "graph has too many source origins",
                )
            };
            let first = u32::try_from(origin_count).map_err(|_| too_many())?;
            origin_count = origin_count.saturating_add(source.keys.len());
            // Every reference this file takes stays below `NONE`.
            if !source.keys.is_empty() && origin_count > NONE as usize {
                return Err(too_many());
            }
            output.indexes.insert(
                (*file).to_owned(),
                (first, std::mem::take(&mut source.keys)),
            );
        }
        let descriptors = SourceDescriptors {
            snapshots: snapshots.into_values().cloned().collect(),
            representations: representations.into_values().cloned().collect(),
            units: units.into_values().cloned().collect(),
        };
        output.table = super::super::format::sources::SourceTable::from_packed(
            descriptors,
            captured
                .iter()
                .map(|(_, source)| (&source.origins, &source.descriptors)),
            cancel,
        )
        .map_err(ToolError::source)?;
        Ok(output)
    }

    pub(super) fn origin(&self, file: &str, span: Option<ObservedSpan>) -> u32 {
        span.and_then(|span| {
            let (first, keys) = self.indexes.get(file)?;
            let at = keys.binary_search(&capture::key(span)).ok()?;
            Some(first + u32::try_from(at).ok()?)
        })
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
