use super::*;
use crate::execution::cancel::CancellationCheck;
use octocode_engine::text::analysis::{
    ByteRange, OriginProjection, SourceDescriptors, SourceError,
};
mod budget;
mod catalog;
mod pool;
#[cfg(test)]
mod tests;

#[derive(Debug, Default, Eq, PartialEq)]
pub(crate) struct SourceTable {
    pub descriptors: SourceDescriptors,
    origins: pool::OriginPool,
}

fn range(origin: &OriginProjection) -> ByteRange {
    ByteRange::new(
        origin
            .segments
            .first()
            .map_or(0, |s| s.source_range.start_byte),
        origin
            .segments
            .last()
            .map_or(0, |s| s.source_range.end_byte),
    )
}
fn map_budget(stop: &(dyn Fn() -> bool + Sync)) -> octocode_engine::text::analysis::MapBudget<'_> {
    use octocode_engine::text::analysis::{MapBudget, MapLimits};
    MapBudget::new(
        MapLimits {
            bytes: super::super::MAX_GRAPH_BYTES,
            work: super::super::MAX_GRAPH_BYTES,
            ..Default::default()
        },
        std::time::Instant::now() + std::time::Duration::from_secs(30),
        stop,
    )
}

impl SourceTable {
    pub(crate) fn from_origins<'a>(
        descriptors: SourceDescriptors,
        origins: impl IntoIterator<Item = &'a OriginProjection>,
        cancel: &dyn CancellationCheck,
    ) -> Result<Self, SourceError> {
        let mut output = Self {
            descriptors,
            ..Default::default()
        };
        let mut budget = budget::Budget::new(cancel);
        for origin in origins {
            output
                .origins
                .push(origin, &output.descriptors, &mut budget)?;
        }
        output.validate_with_cancel(cancel)?;
        Ok(output)
    }
    fn admit_read(&self, budget: &mut budget::Budget<'_>) -> Result<(), SourceError> {
        catalog::admit_read(&self.descriptors, budget)?;
        self.origins.admit_read(budget)
    }
    pub(crate) fn validate_with_cancel(
        &self,
        cancel: &dyn CancellationCheck,
    ) -> Result<(), SourceError> {
        cancel.check().map_err(|_| SourceError::Cancelled)?;
        sorted_ids(
            self.descriptors.snapshots.iter().map(|row| row.id.as_str()),
            cancel,
        )?;
        sorted_ids(
            self.descriptors
                .representations
                .iter()
                .map(|row| row.id.as_str()),
            cancel,
        )?;
        sorted_ids(
            self.descriptors.units.iter().map(|row| row.id.as_str()),
            cancel,
        )?;
        self.admit_read(&mut budget::Budget::new(cancel))?;
        self.origins
            .validate(&self.descriptors, &mut budget::Budget::new(cancel))
    }
    pub(crate) fn origin(
        &self,
        reference: u32,
        projection: &mut crate::response::source_evidence::ProjectionBudget,
        cancel: &dyn CancellationCheck,
    ) -> Result<OriginProjection, crate::response::result::ToolError> {
        use crate::response::result::ToolError;
        let entry = self.origins.get(reference).map_err(ToolError::source)?;
        projection.reserve(entry.bytes as usize, entry.items as usize, cancel)?;
        self.origins
            .origin(
                reference,
                &self.descriptors,
                &mut budget::Budget::with_limits(
                    cancel,
                    entry.bytes as usize,
                    entry.items as usize,
                ),
            )
            .map_err(ToolError::source)
    }
    pub(crate) fn check_range(&self, reference: u32, start: u32, end: u32) -> Result<(), String> {
        if start == NONE || end == NONE {
            return if reference == NONE && start == end {
                Ok(())
            } else {
                Err("graph source span and origin must both be present or absent".into())
            };
        }
        let origin = self
            .origins
            .get(reference)
            .map_err(|error| error.to_string())?;
        if origin.range != ByteRange::new(start, end) {
            return Err("graph source origin does not partition its paired range".into());
        }
        Ok(())
    }

    pub(crate) fn same_representation(&self, first: u32, second: u32) -> bool {
        self.representation(first)
            .is_some_and(|value| Some(value) == self.representation(second))
    }

    fn representation(&self, reference: u32) -> Option<&str> {
        let origin = self.origins.get(reference).ok()?;
        Some(
            &self
                .descriptors
                .units
                .get(origin.unit as usize)?
                .representation_id,
        )
    }

    pub(crate) fn digest(&self, reference: u32) -> Option<&str> {
        let id = self.representation(reference)?;
        let index = self
            .descriptors
            .representations
            .binary_search_by(|representation| representation.id.as_str().cmp(id))
            .ok()?;
        Some(&self.descriptors.representations[index].source_digest)
    }

    pub(crate) fn check_digest(&self, reference: u32, digest: &str) -> Result<(), String> {
        if reference != NONE && self.digest(reference) != Some(digest) {
            return Err("graph source digest does not identify its origin representation".into());
        }
        Ok(())
    }

    pub(crate) fn validate_records(
        &self,
        tables: &GraphTables,
        cancel: &dyn CancellationCheck,
    ) -> Result<(), String> {
        for declaration in &tables.declarations {
            cancel.check()?;
            self.check_range(
                declaration.source_origin,
                declaration.source_start,
                declaration.source_end,
            )?;
            self.check_range(
                declaration.selection_origin,
                declaration.selection_start,
                declaration.selection_end,
            )?;
            self.check_digest(
                declaration.source_origin,
                tables.str(declaration.source_digest),
            )?;
        }
        for witness in &tables.witnesses {
            cancel.check()?;
            let provenance = witness.provenance;
            self.check_range(
                provenance.source_origin,
                provenance.source_start,
                provenance.source_end,
            )?;
            self.check_digest(
                provenance.source_origin,
                tables.str(provenance.source_digest),
            )?;
            arguments::validate(
                tables,
                witness.argument,
                witness.edge.kind,
                provenance,
                None,
            )?;
        }
        for site in &tables.unresolved {
            cancel.check()?;
            let provenance = site.provenance;
            self.check_range(
                provenance.source_origin,
                provenance.source_start,
                provenance.source_end,
            )?;
            self.check_digest(
                provenance.source_origin,
                tables.str(provenance.source_digest),
            )?;
            arguments::validate(
                tables,
                site.argument,
                site.relation,
                provenance,
                Some(tables.str(site.reason)),
            )?;
        }
        Ok(())
    }
}

fn sorted_ids<'a>(
    ids: impl Iterator<Item = &'a str>,
    cancel: &dyn crate::execution::cancel::CancellationCheck,
) -> Result<(), SourceError> {
    let mut previous = None;
    for id in ids {
        cancel.check().map_err(|_| SourceError::Cancelled)?;
        if previous.is_some_and(|previous| previous >= id) {
            return Err(SourceError::InvalidMap(
                "graph source descriptor indexes are not strictly sorted",
            ));
        }
        previous = Some(id);
    }
    Ok(())
}

pub(super) fn read(
    sections: &Sections,
    tables: &mut GraphTables,
    cancel: &dyn CancellationCheck,
) -> Result<(), String> {
    let reader = sections.get(b"SRCE", "SRCE")?;
    tables.sources = decode(reader.bytes, cancel)
        .map_err(|error| format!("graph source correspondence is invalid: {error}"))?;
    Ok(())
}
fn decode(bytes: &[u8], cancel: &dyn CancellationCheck) -> Result<SourceTable, SourceError> {
    decode_with_budget(bytes, &mut budget::Budget::new(cancel))
}
fn decode_with_budget(
    bytes: &[u8],
    budget: &mut budget::Budget<'_>,
) -> Result<SourceTable, SourceError> {
    let cancel = budget.cancel;
    let mut input = budget::Input::new(bytes, budget);
    let descriptors = catalog::read(&mut input)?;
    sorted_ids(
        descriptors.snapshots.iter().map(|row| row.id.as_str()),
        cancel,
    )?;
    sorted_ids(
        descriptors
            .representations
            .iter()
            .map(|row| row.id.as_str()),
        cancel,
    )?;
    sorted_ids(descriptors.units.iter().map(|row| row.id.as_str()), cancel)?;
    let origins = pool::OriginPool::read(&mut input, &descriptors)?;
    Ok(SourceTable {
        descriptors,
        origins,
    })
}
pub(super) fn encode(
    sources: &SourceTable,
    cancel: &dyn CancellationCheck,
) -> Result<Vec<u8>, SourceError> {
    let mut out = Vec::new();
    let mut budget = budget::Budget::new(cancel);
    catalog::write(&sources.descriptors, &mut out, &mut budget)?;
    sources.origins.write(&mut out, &mut budget)?;
    Ok(out)
}
