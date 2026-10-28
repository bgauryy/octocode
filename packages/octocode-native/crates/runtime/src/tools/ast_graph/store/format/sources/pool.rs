use super::*;
use budget::{Budget, Input, invalid};
use octocode_engine::text::analysis::SourceCatalog;
mod codec;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct OriginIndex {
    pub offset: u32,
    pub unit: u32,
    pub range: ByteRange,
    pub bytes: u32,
    pub items: u32,
}
#[derive(Debug, Default, Eq, PartialEq)]
pub(super) struct OriginPool {
    encoded: Vec<u8>,
    index: Vec<OriginIndex>,
}
impl OriginPool {
    #[cfg(test)]
    pub(super) fn storage_bytes(&self) -> (usize, usize) {
        (
            self.encoded.len(),
            self.index.len() * std::mem::size_of::<OriginIndex>(),
        )
    }

    pub fn get(&self, reference: u32) -> Result<&OriginIndex, SourceError> {
        self.index
            .get(reference as usize)
            .ok_or_else(|| invalid("graph source origin reference is out of bounds"))
    }
    pub fn origin(
        &self,
        reference: u32,
        descriptors: &SourceDescriptors,
        budget: &mut Budget<'_>,
    ) -> Result<OriginProjection, SourceError> {
        let entry = self.get(reference)?;
        let end = self
            .index
            .get(reference as usize + 1)
            .map_or(self.encoded.len(), |next| next.offset as usize);
        let bytes = self
            .encoded
            .get(entry.offset as usize..end)
            .ok_or_else(|| invalid("invalid source origin byte index"))?;
        let mut input = Input::new(bytes, budget);
        let (_, origin) = codec::read(&mut input, descriptors)?;
        input.finish()?;
        Ok(origin)
    }
    /// Admission validates every record, including origins no query selects.
    pub fn read(
        input: &mut Input<'_, '_, '_>,
        descriptors: &SourceDescriptors,
    ) -> Result<Self, SourceError> {
        let count = input.count()?;
        let mut index = input.budget.vector(count)?;
        let begin = input.reader.at;
        let cancel = input.budget.cancel;
        let stop = || cancel.check().is_err();
        let mut map = super::map_budget(&stop);
        let catalog = SourceCatalog::new(descriptors, &mut map)?;
        for _ in 0..count {
            let offset = input.reader.at - begin;
            let before = (input.budget.bytes, input.budget.items);
            let (unit, origin) = codec::read(input, descriptors)?;
            let range = super::range(&origin);
            catalog.validate_projection(&origin, range, &mut map)?;
            index.push(OriginIndex {
                offset: narrow(offset)?,
                unit,
                range,
                bytes: narrow(input.budget.bytes - before.0)?,
                items: narrow(input.budget.items - before.1)?,
            });
        }
        input.finish()?;
        let encoded = input
            .budget
            .copy(&input.reader.bytes[begin..input.reader.at])?;
        Ok(Self { encoded, index })
    }
    pub fn write(&self, out: &mut Vec<u8>, budget: &mut Budget<'_>) -> Result<(), SourceError> {
        budget.put(out, self.index.len())?;
        budget.grow(out, self.encoded.len())?;
        out.extend_from_slice(&self.encoded);
        Ok(())
    }
    pub fn push(
        &mut self,
        origin: &OriginProjection,
        descriptors: &SourceDescriptors,
        budget: &mut Budget<'_>,
    ) -> Result<u32, SourceError> {
        if self.index.len() >= MAX_DECODED_ITEMS {
            return Err(SourceError::LimitExceeded("graph source origin count"));
        }
        let reference = narrow(self.index.len())?;
        let offset = self.encoded.len();
        if let Err(error) = codec::write(&mut self.encoded, origin, descriptors, budget) {
            self.encoded.truncate(offset);
            return Err(error);
        }
        // Decode once to compute the exact reconstruction admission cost. This
        // retains neither cloned IDs nor nested vectors in the published table.
        let before = (budget.bytes, budget.items);
        let mut input = Input::new(&self.encoded[offset..], budget);
        let (unit, decoded) = codec::read(&mut input, descriptors)?;
        let entry = OriginIndex {
            offset: narrow(offset)?,
            unit,
            range: super::range(&decoded),
            bytes: narrow(input.budget.bytes - before.0)?,
            items: narrow(input.budget.items - before.1)?,
        };
        budget.grow(&mut self.index, 1)?;
        self.index.push(entry);
        Ok(reference)
    }
    pub fn admit_read(&self, budget: &mut Budget<'_>) -> Result<(), SourceError> {
        budget.vector_cost::<OriginIndex>(self.index.len())?;
        for entry in &self.index {
            budget.charge(entry.bytes as usize, entry.items as usize)?;
        }
        budget.charge(self.encoded.len(), 0)
    }
    pub fn validate(
        &self,
        descriptors: &SourceDescriptors,
        budget: &mut Budget<'_>,
    ) -> Result<(), SourceError> {
        let cancel = budget.cancel;
        let stop = || cancel.check().is_err();
        let mut map = super::map_budget(&stop);
        let catalog = SourceCatalog::new(descriptors, &mut map)?;
        for (reference, entry) in self.index.iter().enumerate() {
            let before = (budget.bytes, budget.items);
            let origin = self.origin(narrow(reference)?, descriptors, budget)?;
            let range = super::range(&origin);
            if entry.range != range
                || entry.unit != catalog::unit_index(descriptors, &origin.analysis_unit_id)?
                || entry.bytes as usize != budget.bytes - before.0
                || entry.items as usize != budget.items - before.1
            {
                return Err(invalid(
                    "source origin index disagrees with encoded evidence",
                ));
            }
            catalog.validate_projection(&origin, range, &mut map)?;
        }
        Ok(())
    }
}
fn narrow(value: usize) -> Result<u32, SourceError> {
    u32::try_from(value).map_err(|_| invalid("source origin index overflows u32"))
}
