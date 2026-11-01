use super::*;
use budget::{Budget, Input, invalid};
use octocode_engine::text::analysis::SourceCatalog;
mod codec;
mod parallel;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct OriginIndex {
    pub offset: u32,
    pub unit: u32,
    pub range: ByteRange,
    pub bytes: u32,
    pub items: u32,
}
const _: () = assert!(
    std::mem::size_of::<OriginIndex>() == super::super::super::super::limits::ORIGIN_INDEX_BYTES
);
#[derive(Debug, Default, Eq, PartialEq)]
pub(super) struct OriginPool {
    encoded: Vec<u8>,
    index: Vec<OriginIndex>,
    /// A sparse pool of `origins` entries holds only the blocks read so far,
    /// keyed by first reference, with offsets local to each block's records.
    sparse: Option<Sparse>,
}

#[derive(Debug, Default, Eq, PartialEq)]
struct Sparse {
    origins: usize,
    blocks: std::collections::BTreeMap<u32, (Vec<OriginIndex>, Vec<u8>)>,
}
impl OriginPool {
    #[cfg(test)]
    pub(super) fn storage_bytes(&self) -> (usize, usize) {
        (
            self.encoded.len(),
            self.index.len() * std::mem::size_of::<OriginIndex>(),
        )
    }
    #[cfg(test)]
    pub(super) fn entries_mut(&mut self) -> &mut [OriginIndex] {
        &mut self.index
    }

    pub fn sparse(origins: usize) -> Self {
        Self {
            sparse: Some(Sparse {
                origins,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    /// Whether `reference` is readable now: always for a whole pool.
    pub fn has(&self, reference: u32) -> bool {
        self.sparse
            .as_ref()
            .is_none_or(|sparse| Self::block_of(sparse, reference).is_some())
    }

    fn block_of(sparse: &Sparse, reference: u32) -> Option<(&[OriginIndex], &[u8], usize)> {
        let (first, (entries, records)) = sparse.blocks.range(..=reference).next_back()?;
        let at = (reference - first) as usize;
        (at < entries.len()).then_some((entries.as_slice(), records.as_slice(), at))
    }

    /// Admits one `SRCO` block body: `count` entries checked structurally
    /// (each record is decoded and validated when read), then exactly their
    /// records.
    pub fn insert_block(
        &mut self,
        first: u32,
        count: u32,
        body: &[u8],
        descriptors: &SourceDescriptors,
        cancel: &dyn crate::execution::cancel::CancellationCheck,
    ) -> Result<(), SourceError> {
        let origins = self.sparse.as_ref().map_or(0, |sparse| sparse.origins);
        if first as usize + count as usize > origins {
            return Err(invalid("graph source origin block exceeds the pool"));
        }
        let mut budget = Budget::new(cancel, &LIMITS);
        let mut input = Input::new(body, &mut budget);
        let (entries, length) = read_entries(&mut input, count as usize, descriptors)?;
        let records = input
            .reader
            .take(length as usize)
            .map_err(|_| invalid("truncated source origin records"))?
            .to_vec();
        input.finish()?;
        if let Some(sparse) = &mut self.sparse {
            sparse.blocks.insert(first, (entries, records));
        }
        Ok(())
    }

    pub fn get(&self, reference: u32) -> Result<&OriginIndex, SourceError> {
        let entry = match &self.sparse {
            Some(sparse) => Self::block_of(sparse, reference).map(|(entries, _, at)| &entries[at]),
            None => self.index.get(reference as usize),
        };
        entry.ok_or_else(|| invalid("graph source origin reference is out of bounds"))
    }
    /// The decoded record and the unit index it names, unvalidated; readers
    /// use [`Self::checked_origin`].
    fn record(
        &self,
        reference: u32,
        descriptors: &SourceDescriptors,
        budget: &mut Budget<'_>,
    ) -> Result<(u32, OriginProjection), SourceError> {
        let (index, encoded, at): (&[OriginIndex], &[u8], usize) = match &self.sparse {
            Some(sparse) => Self::block_of(sparse, reference)
                .ok_or_else(|| invalid("graph source origin reference is out of bounds"))?,
            None => (&self.index, &self.encoded, reference as usize),
        };
        let entry = index
            .get(at)
            .ok_or_else(|| invalid("graph source origin reference is out of bounds"))?;
        let end = index
            .get(at + 1)
            .map_or(encoded.len(), |next| next.offset as usize);
        let bytes = encoded
            .get(entry.offset as usize..end)
            .ok_or_else(|| invalid("invalid source origin byte index"))?;
        let mut input = Input::new(bytes, budget);
        let record = codec::read(&mut input, descriptors)?;
        input.finish()?;
        Ok(record)
    }
    /// Every record admitted: the persisted index, then each origin decoded
    /// and validated against the catalog and its entry.
    pub fn read(
        input: &mut Input<'_, '_, '_>,
        descriptors: &SourceDescriptors,
    ) -> Result<Self, SourceError> {
        let pool = Self::read_index(input, descriptors)?;
        pool.validate(descriptors, input.budget)?;
        Ok(pool)
    }
    /// The persisted index and the encoded records, without decoding them.
    /// Entries are checked structurally: record extents partition the
    /// records and every unit exists. [`Self::checked_origin`] decodes and
    /// validates a record when it is read.
    pub fn read_index(
        input: &mut Input<'_, '_, '_>,
        descriptors: &SourceDescriptors,
    ) -> Result<Self, SourceError> {
        let count = input.count()?;
        let (index, offset) = read_entries(input, count, descriptors)?;
        let records = input
            .reader
            .take(offset as usize)
            .map_err(|_| invalid("truncated source origin records"))?;
        let encoded = input.budget.copy(records)?;
        input.finish()?;
        Ok(Self {
            encoded,
            index,
            sparse: None,
        })
    }
    /// Index entries (record length, unit, range, decode charge), then the
    /// encoded records.
    pub fn write(&self, out: &mut Vec<u8>, budget: &mut Budget<'_>) -> Result<(), SourceError> {
        budget.put(out, self.index.len())?;
        for (reference, entry) in self.index.iter().enumerate() {
            let end = self
                .index
                .get(reference + 1)
                .map_or(self.encoded.len(), |next| next.offset as usize);
            for value in [
                end - entry.offset as usize,
                entry.unit as usize,
                entry.range.start_byte as usize,
                entry.range.end_byte as usize,
                entry.bytes as usize,
                entry.items as usize,
            ] {
                budget.put(out, value)?;
            }
        }
        budget.grow(out, self.encoded.len())?;
        out.extend_from_slice(&self.encoded);
        Ok(())
    }
    pub fn admit_read(&self, budget: &mut Budget<'_>) -> Result<(), SourceError> {
        budget.vector_cost::<OriginIndex>(self.index.len())?;
        for entry in &self.index {
            budget.charge(entry.bytes as usize, entry.items as usize)?;
        }
        budget.charge(self.encoded.len(), 0)
    }
    /// One origin, decoded and validated against `catalog` and its index entry.
    pub fn checked_origin(
        &self,
        reference: u32,
        descriptors: &SourceDescriptors,
        catalog: &SourceCatalog<'_>,
        map: &mut octocode_engine::text::analysis::MapBudget<'_>,
        budget: &mut Budget<'_>,
    ) -> Result<OriginProjection, SourceError> {
        let entry = *self.get(reference)?;
        let before = (budget.bytes, budget.items);
        let (unit, origin) = self.record(reference, descriptors, budget)?;
        let range = super::range(&origin);
        if entry.range != range
            || entry.unit != unit
            || entry.bytes as usize != budget.bytes - before.0
            || entry.items as usize != budget.items - before.1
        {
            return Err(invalid(
                "source origin index disagrees with encoded evidence",
            ));
        }
        catalog.validate_projection(&origin, range, map)?;
        Ok(origin)
    }
}
/// `count` index entries with offsets from the first record; returns them
/// and their records' total length. Each length is nonzero and each unit
/// exists.
fn read_entries(
    input: &mut Input<'_, '_, '_>,
    count: usize,
    descriptors: &SourceDescriptors,
) -> Result<(Vec<OriginIndex>, u32), SourceError> {
    let mut index = input.budget.vector(count)?;
    let mut offset = 0u32;
    for _ in 0..count {
        let length = input.number()?;
        let entry = OriginIndex {
            offset,
            unit: input.number()?,
            range: input.range()?,
            bytes: input.number()?,
            items: input.number()?,
        };
        if length == 0 || entry.unit as usize >= descriptors.units.len() {
            return Err(invalid(
                "source origin index disagrees with encoded evidence",
            ));
        }
        offset = offset
            .checked_add(length)
            .ok_or_else(|| invalid("source origin index overflows u32"))?;
        index.push(entry);
    }
    Ok((index, offset))
}

fn narrow(value: usize) -> Result<u32, SourceError> {
    u32::try_from(value).map_err(|_| invalid("source origin index overflows u32"))
}
