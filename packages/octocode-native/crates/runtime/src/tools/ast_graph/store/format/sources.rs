use super::*;
use crate::execution::cancel::CancellationCheck;
use octocode_engine::text::analysis::{
    ByteRange, OriginProjection, SourceCatalog, SourceDescriptors, SourceError,
};
mod budget;
mod catalog;
mod codec;
mod pool;
#[cfg(test)]
pub(super) use codec::{decode, encode};
#[cfg(test)]
use codec::{decode_admitted, decode_with_budget};
pub(super) use codec::{encode_parts, read, read_catalog};
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
            bytes: LIMITS.source_bytes,
            work: LIMITS.source_work,
            ..Default::default()
        },
        std::time::Instant::now() + LIMITS.source_deadline,
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
        let mut budget = budget::Budget::new(cancel, &LIMITS);
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
        self.admit_read(&mut budget::Budget::new(cancel, &LIMITS))?;
        self.origins
            .validate(&self.descriptors, &mut budget::Budget::new(cancel, &LIMITS))
    }
    pub(crate) fn origin(
        &self,
        reference: u32,
        projection: &mut crate::response::source_evidence::ProjectionBudget,
        cancel: &dyn CancellationCheck,
    ) -> Result<OriginProjection, crate::response::result::ToolError> {
        use crate::response::result::ToolError;
        let entry = *self.origins.get(reference).map_err(ToolError::source)?;
        projection.reserve(entry.bytes as usize, entry.items as usize, cancel)?;
        let scope = self.unit_scope(entry.unit).map_err(ToolError::source)?;
        let stop = || cancel.check().is_err();
        let mut map = map_budget(&stop);
        let catalog = SourceCatalog::new(&scope, &mut map).map_err(ToolError::source)?;
        self.origins
            .checked_origin(
                reference,
                &self.descriptors,
                &catalog,
                &mut map,
                // Exactly what this origin's index entry admitted.
                &mut budget::Budget::new(
                    cancel,
                    &Limits {
                        source_bytes: entry.bytes as usize,
                        table_items: entry.items as usize,
                        ..LIMITS
                    },
                ),
            )
            .map_err(ToolError::source)
    }

    /// The descriptors one origin's validation reads: its unit, the unit's
    /// representation and that representation's sources. A projection may
    /// cite only those sources, so this catalog decides it exactly as the
    /// whole one does, at the cost of three lookups.
    fn unit_scope(&self, unit: u32) -> Result<SourceDescriptors, SourceError> {
        let missing = || budget::invalid("source origin unit has no representation descriptor");
        let unit = self
            .descriptors
            .units
            .get(unit as usize)
            .ok_or_else(|| budget::invalid("source origin unit index out of bounds"))?;
        let representation = self
            .descriptors
            .representations
            .binary_search_by(|row| row.id.cmp(&unit.representation_id))
            .ok()
            .map(|at| &self.descriptors.representations[at])
            .ok_or_else(missing)?;
        let mut snapshots = representation
            .source_ids
            .iter()
            .filter_map(|id| {
                self.descriptors
                    .snapshots
                    .binary_search_by(|row| row.id.cmp(id))
                    .ok()
                    .map(|at| self.descriptors.snapshots[at].clone())
            })
            .collect::<Vec<_>>();
        snapshots.sort_unstable_by(|a, b| a.id.cmp(&b.id));
        Ok(SourceDescriptors {
            snapshots,
            representations: vec![representation.clone()],
            units: vec![unit.clone()],
        })
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

/// How much of the origin pool a read validates before use.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Admission {
    /// Every origin record, decoded and validated against the catalog.
    Every,
    /// The catalog and the persisted origin index; each origin is decoded
    /// and validated when it is read.
    Accessed,
}

impl SourceTable {
    /// Whether origin `reference` is available without reading a block.
    pub(super) fn has_origin(&self, reference: u32) -> bool {
        self.origins.has(reference)
    }

    /// Admits one authenticated `SRCO` block of `count` origins starting at
    /// `first`: its entries are checked structurally, its records when read.
    pub(super) fn insert_origin_block(
        &mut self,
        first: u32,
        count: u32,
        body: &[u8],
        cancel: &dyn CancellationCheck,
    ) -> Result<(), String> {
        self.origins
            .insert_block(first, count, body, &self.descriptors, cancel)
            .map_err(|error| format!("graph source correspondence is invalid: {error}"))
    }
}

