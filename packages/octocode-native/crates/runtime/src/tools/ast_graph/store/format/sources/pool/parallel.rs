//! Origins encoded and validated in parallel. Records join the pool in
//! reference order, so the pool's bytes and its index match a one-at-a-time
//! pass at any width. Decoding is charged per block of origins, the unit a
//! reader admits.
use super::*;

/// Origins encoded per window: bounds the records held before they join
/// the pool.
pub(super) const WINDOW: usize = 1 << 16;

/// Origins per `SRCO` block: the unit a reader admits.
pub(super) const BLOCK: usize = super::super::super::blocks::ROWS;

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
/// vectors. `pass` polls the request once per origin; each charge is the
/// origin's own.
pub(super) fn encode(
    origin: &OriginProjection,
    descriptors: &SourceDescriptors,
    pass: &Budget<'_>,
) -> Result<Record, SourceError> {
    pass.check()?;
    let mut bytes = Vec::new();
    codec::write(&mut bytes, origin, descriptors, &mut pass.scratch())?;
    let mut decode = pass.scratch();
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

/// [`batches`] whose ranges start at block boundaries, so each block is
/// admitted whole by one task.
fn block_batches(len: usize, width: usize) -> Vec<std::ops::Range<usize>> {
    let size = len
        .div_ceil(width.saturating_mul(4).max(1))
        .max(1)
        .next_multiple_of(BLOCK);
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
                let pass = Budget::new(cancel, &LIMITS);
                window[range.clone()]
                    .iter()
                    .map(|origin| encode(origin, descriptors, &pass))
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
        // Origins are read a block at a time; their count is bounded only by
        // the `u32` reference every span names.
        let reference = narrow(self.index.len())?;
        let offset = self.encoded.len();
        // The pool's own bytes are the writer's memory, not a reader's
        // admission: a reader admits one block at a time (below), so the
        // whole pool is bounded only by what the build holds.
        self.encoded.extend_from_slice(&record.bytes);
        let (bytes, items) = record.decode;
        let entry = OriginIndex {
            offset: narrow(offset)?,
            unit: record.unit,
            range: record.range,
            bytes: narrow(bytes)?,
            items: narrow(items)?,
        };
        self.index.push(entry);
        // A reader admits one block of origins at a time: the block this
        // record joined, with it, must fit one admission.
        let start = self.index.len() - 1 - (self.index.len() - 1) % BLOCK;
        if let Err(error) = super::admit_block(&mut budget.scratch(), &self.index[start..]) {
            self.index.pop();
            #[cfg_attr(
                octocode_clippy_ratchet,
                expect(
                    clippy::disallowed_methods,
                    reason = "Roll back only the rejected append to its exact prior length; the admission error is returned"
                )
            )]
            {
                self.encoded.truncate(offset);
            }
            return Err(error);
        }
        Ok(reference)
    }

    /// Every origin decoded and checked against `descriptors` and its index
    /// entry, references in parallel; each block of origins is charged on
    /// its own, as a reader admits it.
    pub fn validate(
        &self,
        descriptors: &SourceDescriptors,
        budget: &mut Budget<'_>,
    ) -> Result<(), SourceError> {
        let cancel = budget.cancel;
        let limits = budget.limits();
        let poll = super::super::Poll::new(cancel);
        let stop = || poll.stopped();
        let catalog =
            SourceCatalog::new(descriptors, &mut super::super::map_budget(cancel, &stop))?;
        let width = crate::tools::workers::workers(self.index.len(), None);
        let mut ranges = block_batches(self.index.len(), width);
        let checked = crate::tools::workers::each_parallel(&mut ranges, width, |range| {
            let own_poll = super::super::Poll::new(cancel);
            let own_stop = || own_poll.stopped();
            let mut own = Budget::new(cancel, &limits);
            let mut own_map = super::super::map_budget(cancel, &own_stop);
            for reference in range.clone() {
                // Each block of origins is admitted on its own, as a reader
                // admits it.
                if reference % BLOCK == 0 {
                    own = Budget::new(cancel, &limits);
                    own_map = super::super::map_budget(cancel, &own_stop);
                    // The block's index entries, then (below) each decode:
                    // what [`super::admit_block`] charges.
                    own.vector_cost::<OriginIndex>(BLOCK.min(self.index.len() - reference))?;
                }
                self.checked_origin(
                    narrow(reference)?,
                    descriptors,
                    &catalog,
                    &mut own_map,
                    &mut own,
                )?;
            }
            Ok::<_, SourceError>(())
        });
        checked.into_iter().collect()
    }
}
