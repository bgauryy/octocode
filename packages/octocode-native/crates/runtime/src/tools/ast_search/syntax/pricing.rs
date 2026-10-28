use crate::execution::cancel::CancellationCheck;
use crate::response::result::ToolError;
use crate::response::source_evidence::{self, ProjectionBudget, ProjectionLimits};
use octocode_contract_types::{OutputSourceEvidence, OutputSourceSnapshot};
use octocode_engine::structural::SyntaxTreeInspectResult;
use octocode_engine::text::analysis::{AdmittedSource, BoundSource, MapBudget, UnitHandle};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashSet, btree_map::Entry};
use std::ops::Range;

#[cfg(test)]
thread_local! {
    static PRICED_NODES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(super) fn priced_nodes() -> usize {
    PRICED_NODES.with(std::cell::Cell::get)
}

#[derive(Clone, Default)]
pub(super) struct Prices {
    sizes: Vec<usize>,
    unit_ids: BTreeMap<UnitHandle, String>,
}

impl Prices {
    pub(super) fn weight(&self) -> usize {
        self.sizes.capacity() * std::mem::size_of::<usize>()
            + self
                .unit_ids
                .values()
                .map(|id| id.capacity() + 96)
                .sum::<usize>()
    }

    fn extend(
        &mut self,
        result: &SyntaxTreeInspectResult,
        bound: &BoundSource<'_>,
        maps: &mut MapBudget<'_>,
        projection: &mut ProjectionBudget,
        cancel: &dyn CancellationCheck,
    ) -> Result<(), ToolError> {
        let remaining = result.nodes.len().saturating_sub(self.sizes.len());
        projection.reserve(
            remaining.saturating_mul(2 * std::mem::size_of::<usize>()),
            remaining,
            cancel,
        )?;
        for node in &result.nodes[self.sizes.len()..] {
            #[cfg(test)]
            PRICED_NODES.with(|count| count.set(count.get() + 1));
            let mut failure = None;
            let mut unit_id = String::new();
            let digest = &bound
                .representation_for_unit(node.source_span.unit)
                .map_err(ToolError::source)?
                .source_digest;
            let size = bound
                .project_size(node.source_span, maps, |origin| {
                    unit_id.clone_from(&origin.analysis_unit_id);
                    let mut scratch = ProjectionBudget::new(ProjectionLimits::default());
                    let row = source_evidence::project_origin(origin, &mut scratch, cancel)
                        .and_then(|origin| {
                            let mut row = super::render::node_page_row(node);
                            row["sourceOrigin"] =
                                serde_json::to_value(origin).map_err(encoding_error)?;
                            row["sourceDigest"] = Value::String(digest.clone());
                            Ok(row)
                        });
                    match row {
                        Ok(row) => octocode_engine::json_size::utf16_units(&row)
                            .unwrap_or(usize::MAX)
                            .saturating_add(1),
                        Err(error) => {
                            failure = Some(error);
                            0
                        }
                    }
                })
                .map_err(ToolError::source)?;
            if let Some(error) = failure {
                return Err(error);
            }
            if let Entry::Vacant(entry) = self.unit_ids.entry(node.source_span.unit) {
                projection.reserve(96 + unit_id.capacity(), 1, cancel)?;
                entry.insert(unit_id);
            }
            self.sizes.push(size);
        }
        Ok(())
    }
}

/// Price complete node origins and each page's distinct descriptor closure.
/// Only scalar prices survive temporary projection; the walk memo holds no wire rows.
pub(super) fn windows(
    result: &SyntaxTreeInspectResult,
    prices: &mut Prices,
    admitted: &AdmittedSource<'_>,
    maps: &mut MapBudget<'_>,
    budget: usize,
    cancel: &dyn CancellationCheck,
) -> Result<Vec<Range<usize>>, ToolError> {
    if result.nodes.is_empty() {
        return Ok(Vec::new());
    }
    let bound = result
        .source_templates
        .bind(admitted, maps)
        .map_err(ToolError::source)?;
    let mut projection = ProjectionBudget::new(ProjectionLimits::default());
    let mut units = BTreeSet::new();
    for node in &result.nodes {
        if !units.contains(&node.source_span.unit) {
            projection.reserve(96, 1, cancel)?;
            units.insert(node.source_span.unit);
        }
    }
    let descriptors = bound
        .descriptors_for_units(&units.into_iter().collect::<Vec<_>>(), maps)
        .map_err(ToolError::source)?;
    let catalog = source_evidence::project_descriptors(
        &descriptors,
        descriptors.units.iter().map(|unit| unit.id.as_str()),
        &mut projection,
        cancel,
    )?;
    let records = descriptors
        .units
        .len()
        .saturating_add(descriptors.representations.len())
        .saturating_add(descriptors.snapshots.len());
    projection.reserve(
        records.saturating_mul(192),
        records.saturating_mul(2),
        cancel,
    )?;
    let costs = DescriptorCosts::new(&catalog, &mut projection, cancel)?;
    prices.extend(result, &bound, maps, &mut projection, cancel)?;
    let mut windows = Vec::new();
    let mut seen = HashSet::with_capacity(records);
    let mut used = 0usize;
    let mut start = 0;
    for (index, node) in result.nodes.iter().enumerate() {
        cancel.check().map_err(ToolError::cancelled)?;
        let size = prices.sizes[index];
        let unit_id = prices
            .unit_ids
            .get(&node.source_span.unit)
            .ok_or_else(invalid_closure)?;
        let records = costs
            .closures
            .get(unit_id.as_str())
            .ok_or_else(invalid_closure)?;
        let added = |seen: &HashSet<_>| {
            records
                .iter()
                .filter(|(kind, id, _)| !seen.contains(&(*kind, *id)))
                .map(|(_, _, size)| size)
                .sum::<usize>()
        };
        if index > start && used.saturating_add(size).saturating_add(added(&seen)) > budget {
            projection.reserve(2 * std::mem::size_of::<Range<usize>>(), 1, cancel)?;
            windows.push(start..index);
            start = index;
            seen.clear();
            used = 0;
        }
        let descriptor_chars = records
            .iter()
            .filter(|(kind, id, _)| !seen.contains(&(*kind, *id)))
            .map(|(_, _, size)| size)
            .sum::<usize>();
        used = used.saturating_add(size).saturating_add(descriptor_chars);
        seen.extend(records.iter().map(|(kind, id, _)| (*kind, *id)));
    }
    projection.reserve(2 * std::mem::size_of::<Range<usize>>(), 1, cancel)?;
    windows.push(start..result.nodes.len());
    Ok(windows)
}

struct DescriptorCosts<'a> {
    closures: BTreeMap<&'a str, Vec<(u8, &'a str, usize)>>,
}

impl<'a> DescriptorCosts<'a> {
    fn new(
        catalog: &'a OutputSourceEvidence,
        budget: &mut ProjectionBudget,
        cancel: &dyn CancellationCheck,
    ) -> Result<Self, ToolError> {
        let units = descriptor_index(&catalog.units, |unit| unit.id.as_str(), cancel)?;
        let representations =
            descriptor_index(&catalog.representations, |row| row.id.as_str(), cancel)?;
        let snapshots = descriptor_index(
            &catalog.snapshots,
            |row| match row {
                OutputSourceSnapshot::Utf8 { id, .. }
                | OutputSourceSnapshot::Utf16le { id, .. }
                | OutputSourceSnapshot::Utf16be { id, .. } => id.as_str(),
            },
            cancel,
        )?;
        let mut costs = Self {
            closures: BTreeMap::new(),
        };
        for (id, (unit, chars)) in units {
            let (representation_id, (representation, representation_chars)) = representations
                .get_key_value(unit.representation_id.as_str())
                .ok_or_else(invalid_closure)?;
            let count = 2 + representation.source_ids.len();
            budget.reserve(
                96 + count.saturating_mul(2 * std::mem::size_of::<(u8, &str, usize)>()),
                count,
                cancel,
            )?;
            let mut records = Vec::with_capacity(count);
            records.push((0, id, chars));
            records.push((1, *representation_id, *representation_chars));
            for source in &representation.source_ids {
                cancel.check().map_err(ToolError::cancelled)?;
                let (source_id, (_, chars)) = snapshots
                    .get_key_value(source.as_str())
                    .ok_or_else(invalid_closure)?;
                records.push((2, *source_id, *chars));
            }
            costs.closures.insert(id, records);
        }
        Ok(costs)
    }
}

fn descriptor_index<'a, T: serde::Serialize>(
    rows: &'a [T],
    id: impl Fn(&'a T) -> &'a str,
    cancel: &dyn CancellationCheck,
) -> Result<BTreeMap<&'a str, (&'a T, usize)>, ToolError> {
    let mut index = BTreeMap::new();
    for row in rows {
        cancel.check().map_err(ToolError::cancelled)?;
        let chars = octocode_engine::json_size::utf16_units(row)
            .and_then(|size| size.checked_add(1))
            .ok_or_else(invalid_closure)?;
        if index.insert(id(row), (row, chars)).is_some() {
            return Err(invalid_closure());
        }
    }
    Ok(index)
}

fn invalid_closure() -> ToolError {
    ToolError::new(
        octocode_contract_types::ErrorCode::ExecutionFailed,
        "Could not price the source descriptor closure.",
    )
}

fn encoding_error(error: serde_json::Error) -> ToolError {
    ToolError::new(
        octocode_contract_types::ErrorCode::ExecutionFailed,
        format!("Could not price source correspondence: {error}"),
    )
}

#[cfg(test)]
mod tests;
