use super::*;
use octocode_engine::text::analysis as domain;
use std::collections::{BTreeMap, BTreeSet};

fn unique<'a, T>(
    rows: &'a [T],
    id: impl Fn(&'a T) -> &'a str,
    cancel: &dyn CancellationCheck,
) -> Result<BTreeMap<&'a str, &'a T>, ToolError> {
    let mut output = BTreeMap::new();
    for row in rows {
        cancel.check().map_err(ToolError::cancelled)?;
        let id = id(row);
        if id.is_empty() || output.insert(id, row).is_some() {
            return Err(invalid(
                "source descriptors contain an empty or duplicate identity",
            ));
        }
    }
    Ok(output)
}

fn validation_budget(stop: &(dyn Fn() -> bool + Sync)) -> domain::MapBudget<'_> {
    domain::MapBudget::new(
        domain::MapLimits::default(),
        std::time::Instant::now() + std::time::Duration::from_secs(30),
        stop,
    )
}

pub(crate) fn validate_catalog(
    value: &SourceDescriptors,
    cancel: &dyn CancellationCheck,
) -> Result<(), ToolError> {
    let stop = || cancel.check().is_err();
    domain::SourceCatalog::new(value, &mut validation_budget(&stop))
        .map(|_| ())
        .map_err(ToolError::source)
}

#[cfg(test)]
pub(super) fn validate_origin(
    origin: &OriginProjection,
    range: domain::ByteRange,
    descriptors: &SourceDescriptors,
    cancel: &dyn CancellationCheck,
) -> Result<(), ToolError> {
    let stop = || cancel.check().is_err();
    let mut budget = validation_budget(&stop);
    domain::SourceCatalog::new(descriptors, &mut budget)
        .and_then(|catalog| catalog.validate_projection(origin, range, &mut budget))
        .map_err(ToolError::source)
}

fn merge_rows<T: Clone + PartialEq>(
    mut rows: Vec<&T>,
    id: impl Fn(&T) -> &str,
    weight: impl Fn(&T) -> usize,
    budget: &mut ProjectionBudget,
    cancel: &dyn CancellationCheck,
) -> Result<Vec<T>, ToolError> {
    cancel.check().map_err(ToolError::cancelled)?;
    rows.sort_unstable_by(|a, b| id(a).cmp(id(b)));
    let mut unique = 0usize;
    let mut previous: Option<&T> = None;
    for &row in &rows {
        cancel.check().map_err(ToolError::cancelled)?;
        if id(row).is_empty() {
            return Err(invalid("source descriptors contain an empty identity"));
        }
        if let Some(prior) = previous.filter(|prior| id(prior) == id(row)) {
            if prior != row {
                return Err(invalid(
                    "source descriptors reuse an identity for different records",
                ));
            }
        } else {
            unique += 1;
            previous = Some(row);
        }
    }
    budget.reserve(
        unique.saturating_mul(std::mem::size_of::<T>()),
        unique,
        cancel,
    )?;
    let mut output = Vec::with_capacity(unique);
    for row in rows {
        cancel.check().map_err(ToolError::cancelled)?;
        if output.last().is_some_and(|prior| id(prior) == id(row)) {
            continue;
        }
        budget.reserve(weight(row).saturating_mul(2), 1, cancel)?;
        output.push(row.clone());
    }
    Ok(output)
}

fn append_rows<'a, T>(
    target: &mut Vec<&'a T>,
    source: &'a [T],
    budget: &mut ProjectionBudget,
    cancel: &dyn CancellationCheck,
) -> Result<(), ToolError> {
    for row in source {
        budget.reserve(0, 1, cancel)?;
        if target.len() == target.capacity() {
            let capacity = target.capacity().saturating_mul(2).max(16);
            budget.reserve(
                capacity.saturating_mul(std::mem::size_of::<&T>()),
                0,
                cancel,
            )?;
            target.reserve_exact(capacity - target.len());
        }
        target.push(row);
    }
    Ok(())
}

pub(crate) fn merge_descriptors<'a>(
    catalogs: impl IntoIterator<Item = &'a SourceDescriptors>,
    budget: &mut ProjectionBudget,
    cancel: &dyn CancellationCheck,
) -> Result<SourceDescriptors, ToolError> {
    let mut snapshots = Vec::new();
    let mut representations = Vec::new();
    let mut units = Vec::new();
    for source in catalogs {
        budget.reserve(0, 1, cancel)?;
        append_rows(&mut snapshots, &source.snapshots, budget, cancel)?;
        append_rows(
            &mut representations,
            &source.representations,
            budget,
            cancel,
        )?;
        append_rows(&mut units, &source.units, budget, cancel)?;
    }
    let output = SourceDescriptors {
        snapshots: merge_rows(snapshots, |r| &r.id, snapshot_weight, budget, cancel)?,
        representations: merge_rows(
            representations,
            |r| &r.id,
            representation_weight,
            budget,
            cancel,
        )?,
        units: merge_rows(units, |r| &r.id, unit_weight, budget, cancel)?,
    };
    let rows = output
        .snapshots
        .len()
        .saturating_add(output.representations.len())
        .saturating_add(output.units.len());
    budget.reserve(rows.saturating_mul(96), rows, cancel)?;
    validate_catalog(&output, cancel)?;
    Ok(output)
}

struct Rows<'a, T> {
    values: &'a [T],
    id: fn(&T) -> &str,
    index: Option<BTreeMap<&'a str, &'a T>>,
}

impl<'a, T> Rows<'a, T> {
    fn new(
        values: &'a [T],
        id: fn(&T) -> &str,
        budget: &mut ProjectionBudget,
        cancel: &dyn CancellationCheck,
    ) -> Result<Self, ToolError> {
        let mut sorted = true;
        for pair in values.windows(2) {
            cancel.check().map_err(ToolError::cancelled)?;
            sorted &= id(&pair[0]) < id(&pair[1]);
        }
        let index = if sorted {
            None
        } else {
            budget.reserve(values.len().saturating_mul(96), values.len(), cancel)?;
            Some(unique(values, id, cancel)?)
        };
        Ok(Self { values, id, index })
    }

    fn get(&self, id: &str) -> Option<&'a T> {
        match &self.index {
            Some(index) => index.get(id).copied(),
            None => self
                .values
                .binary_search_by(|value| (self.id)(value).cmp(id))
                .ok()
                .map(|at| &self.values[at]),
        }
    }
}

pub(super) fn closure<'a>(
    descriptors: &SourceDescriptors,
    unit_ids: impl IntoIterator<Item = &'a str>,
    budget: &mut ProjectionBudget,
    cancel: &dyn CancellationCheck,
) -> Result<SourceDescriptors, ToolError> {
    budget.record(3, cancel)?;
    let unit_rows = Rows::new(&descriptors.units, |row| &row.id, budget, cancel)?;
    let representation_rows =
        Rows::new(&descriptors.representations, |row| &row.id, budget, cancel)?;
    let source_rows = Rows::new(&descriptors.snapshots, |row| &row.id, budget, cancel)?;
    let mut selected = SourceDescriptors::default();
    let mut units = BTreeSet::new();
    let mut representations = BTreeSet::new();
    let mut sources = BTreeSet::new();
    for id in unit_ids {
        budget.reserve(96, 1, cancel)?;
        if !units.insert(id) {
            continue;
        }
        let unit = unit_rows
            .get(id)
            .ok_or_else(|| invalid("emitted source origin has no analysis unit"))?;
        budget.reserve(
            unit_weight(unit).saturating_mul(2).saturating_add(96),
            1,
            cancel,
        )?;
        selected.units.push(unit.clone());
        if !representations.insert(unit.representation_id.as_str()) {
            continue;
        }
        let representation = representation_rows
            .get(&unit.representation_id)
            .ok_or_else(|| invalid("emitted unit has no representation"))?;
        budget.reserve(
            representation_weight(representation).saturating_mul(2),
            1,
            cancel,
        )?;
        selected.representations.push(representation.clone());
        for id in &representation.source_ids {
            budget.reserve(96, 1, cancel)?;
            if sources.insert(id.as_str()) {
                let source = source_rows
                    .get(id)
                    .ok_or_else(|| invalid("emitted representation has no raw source snapshot"))?;
                budget.reserve(snapshot_weight(source).saturating_mul(2), 1, cancel)?;
                selected.snapshots.push(source.clone());
            }
        }
    }
    let records = selected
        .units
        .len()
        .saturating_add(selected.representations.len())
        .saturating_add(selected.snapshots.len());
    budget.reserve(records.saturating_mul(96), records, cancel)?;
    validate_catalog(&selected, cancel)?;
    Ok(selected)
}

fn producer_weight(value: &domain::Producer) -> usize {
    value.id.capacity().saturating_add(value.version.capacity())
}

fn snapshot_weight(value: &domain::SourceSnapshot) -> usize {
    std::mem::size_of_val(value)
        .saturating_add(value.id.capacity())
        .saturating_add(value.path.capacity())
        .saturating_add(value.digest.capacity())
}

fn representation_weight(value: &domain::TextRepresentation) -> usize {
    std::mem::size_of_val(value)
        .saturating_add(value.id.capacity())
        .saturating_add(value.source_digest.capacity())
        .saturating_add(producer_weight(&value.producer))
        .saturating_add(
            value
                .source_ids
                .capacity()
                .saturating_mul(std::mem::size_of::<String>()),
        )
        .saturating_add(
            value
                .source_ids
                .iter()
                .fold(0usize, |bytes, id| bytes.saturating_add(id.capacity())),
        )
}

fn unit_weight(value: &domain::AnalysisUnit) -> usize {
    std::mem::size_of_val(value)
        .saturating_add(value.id.capacity())
        .saturating_add(value.representation_id.capacity())
        .saturating_add(producer_weight(&value.adapter))
        .saturating_add(
            value
                .regions
                .capacity()
                .saturating_mul(std::mem::size_of::<domain::ByteRange>()),
        )
}
