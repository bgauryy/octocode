//! Origins encoded and validated in parallel. Records join the pool and
//! are charged in reference order, so the pool's bytes, its index and its
//! budget match a one-at-a-time pass at any width. Charges only grow, so
//! summing each task's usage enforces the same total caps.
use super::*;
use octocode_engine::text::analysis::{MapBudget, MapUsage};

/// Origins encoded per window: bounds the records held before they join
/// the pool.
const WINDOW: usize = 1 << 16;

/// One origin's encoded record, with its unit, its range and what decoding
/// it costs (the index entry's admission charge).
pub(super) struct Record {
    bytes: Vec<u8>,
    unit: u32,
    range: ByteRange,
    decode: (usize, usize),
}

/// Encodes `origin` on its own, then decodes it once to price its
/// reconstruction exactly. The index keeps neither cloned ids nor nested
/// vectors.
pub(super) fn encode(
    origin: &OriginProjection,
    descriptors: &SourceDescriptors,
    cancel: &dyn CancellationCheck,
) -> Result<Record, SourceError> {
    let mut bytes = Vec::new();
    codec::write(
        &mut bytes,
        origin,
        descriptors,
        &mut Budget::new(cancel, &LIMITS),
    )?;
    let mut decode = Budget::new(cancel, &LIMITS);
    let (unit, decoded) = codec::read(&mut Input::new(&bytes, &mut decode), descriptors)?;
    Ok(Record {
        unit,
        range: super::super::range(&decoded),
        decode: (decode.bytes, decode.items),
        bytes,
    })
}

/// Contiguous index ranges covering `0..len`, a few per worker: each task
/// takes a range, so workers meet once per range rather than per item.
fn batches(len: usize, width: usize) -> Vec<std::ops::Range<usize>> {
    let size = len.div_ceil(width.saturating_mul(4).max(1)).max(1);
    (0..len)
        .step_by(size)
        .map(|start| start..(start + size).min(len))
        .collect()
}

impl OriginPool {
    /// Appends `origins` in order, each as the next reference.
    pub fn extend(
        &mut self,
        origins: &[&OriginProjection],
        descriptors: &SourceDescriptors,
        budget: &mut Budget<'_>,
    ) -> Result<(), SourceError> {
        let cancel = budget.cancel;
        for window in origins.chunks(WINDOW) {
            let width = crate::tools::workers::workers(window.len(), None);
            let mut ranges = batches(window.len(), width);
            let encoded = crate::tools::workers::each_parallel(&mut ranges, width, |range| {
                window[range.clone()]
                    .iter()
                    .map(|origin| encode(origin, descriptors, cancel))
                    .collect::<Vec<_>>()
            });
            for record in encoded.into_iter().flatten() {
                self.append(record?, budget)?;
            }
        }
        Ok(())
    }

    /// Adds one encoded record as the next reference.
    pub(super) fn append(
        &mut self,
        record: Record,
        budget: &mut Budget<'_>,
    ) -> Result<u32, SourceError> {
        if self.index.len() >= LIMITS.table_items {
            return Err(SourceError::LimitExceeded("graph source origin count"));
        }
        let reference = narrow(self.index.len())?;
        let offset = self.encoded.len();
        budget.grow(&mut self.encoded, record.bytes.len())?;
        self.encoded.extend_from_slice(&record.bytes);
        let (bytes, items) = record.decode;
        budget.charge(bytes, items)?;
        let entry = OriginIndex {
            offset: narrow(offset)?,
            unit: record.unit,
            range: record.range,
            bytes: narrow(bytes)?,
            items: narrow(items)?,
        };
        budget.grow(&mut self.index, 1)?;
        self.index.push(entry);
        Ok(reference)
    }

    /// Every origin decoded and checked against `descriptors` and its index
    /// entry, references in parallel; charges are summed in order.
    pub fn validate(
        &self,
        descriptors: &SourceDescriptors,
        budget: &mut Budget<'_>,
    ) -> Result<(), SourceError> {
        let cancel = budget.cancel;
        let poll = super::super::Poll::new(cancel);
        let stop = || poll.stopped();
        let mut map = super::super::map_budget(cancel, &stop);
        let catalog = SourceCatalog::new(descriptors, &mut map)?;
        let width = crate::tools::workers::workers(self.index.len(), None);
        let mut ranges = batches(self.index.len(), width);
        // Each range charges its own budgets; charges only grow, so their
        // sums decide the caps as one pass would.
        let checked = crate::tools::workers::each_parallel(&mut ranges, width, |range| {
            let mut own = Budget::new(cancel, &LIMITS);
            let own_poll = super::super::Poll::new(cancel);
            let own_stop = || own_poll.stopped();
            let mut own_map = super::super::map_budget(cancel, &own_stop);
            for reference in range.clone() {
                self.checked_origin(
                    narrow(reference)?,
                    descriptors,
                    &catalog,
                    &mut own_map,
                    &mut own,
                )?;
            }
            Ok::<_, SourceError>((own.bytes, own.items, own_map.usage))
        });
        for outcome in checked {
            let (bytes, items, usage) = outcome?;
            budget.charge(bytes, items)?;
            absorb(&mut map, usage)?;
        }
        Ok(())
    }
}

/// Adds one task's source-map usage to `map`, refusing past its limits as
/// the one-at-a-time pass would.
fn absorb(map: &mut MapBudget<'_>, used: MapUsage) -> Result<(), SourceError> {
    fn add(
        value: &mut usize,
        more: usize,
        max: usize,
        name: &'static str,
    ) -> Result<(), SourceError> {
        *value = value
            .checked_add(more)
            .filter(|total| *total <= max)
            .ok_or(SourceError::LimitExceeded(name))?;
        Ok(())
    }
    let limits = map.limits;
    let usage = &mut map.usage;
    add(
        &mut usage.representations,
        used.representations,
        limits.representations,
        "representations",
    )?;
    add(
        &mut usage.segments,
        used.segments,
        limits.segments,
        "segments",
    )?;
    add(
        &mut usage.branches,
        used.branches,
        limits.branches,
        "branches",
    )?;
    add(
        &mut usage.contributors,
        used.contributors,
        limits.contributors,
        "contributors",
    )?;
    add(&mut usage.bytes, used.bytes, limits.bytes, "bytes")?;
    add(&mut usage.work, used.work, limits.work, "work")?;
    usage.peak_temporary_bytes = usage.peak_temporary_bytes.max(used.peak_temporary_bytes);
    Ok(())
}
