//! One file's origins, encoded against that file's own descriptors until
//! the snapshot's table re-encodes them against the merged catalog. A
//! record names its unit and sources by descriptor index, so a captured
//! file holds no per-range copies of their ids.
use super::*;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct PackedOrigins {
    encoded: Vec<u8>,
    /// End offset of each record in `encoded`.
    ends: Vec<u32>,
}

impl PackedOrigins {
    /// `origins` encoded in order against `descriptors`, which must catalog
    /// every unit and source they name, sorted by id.
    pub(crate) fn pack(
        origins: &[OriginProjection],
        descriptors: &SourceDescriptors,
        cancel: &dyn crate::execution::cancel::CancellationCheck,
    ) -> Result<Self, SourceError> {
        let pass = Budget::new(cancel, &LIMITS);
        let mut encoded = Vec::new();
        let mut ends = Vec::with_capacity(origins.len());
        for origin in origins {
            pass.check()?;
            codec::write(&mut encoded, origin, descriptors, &mut pass.scratch())?;
            ends.push(narrow(encoded.len())?);
        }
        encoded.shrink_to_fit();
        Ok(Self { encoded, ends })
    }

    /// The packed origins, decoded in order against the `descriptors` they
    /// were packed with.
    fn unpack<'a>(
        &'a self,
        descriptors: &'a SourceDescriptors,
        pass: &'a Budget<'a>,
    ) -> impl Iterator<Item = Result<OriginProjection, SourceError>> + 'a {
        let starts = std::iter::once(0).chain(self.ends.iter().copied());
        starts.zip(&self.ends).map(move |(start, end)| {
            pass.check()?;
            let bytes = self
                .encoded
                .get(start as usize..*end as usize)
                .ok_or_else(|| invalid("invalid packed source origin extent"))?;
            let mut budget = pass.scratch();
            let mut input = Input::new(bytes, &mut budget);
            let (_, origin) = codec::read(&mut input, descriptors)?;
            input.finish()?;
            Ok(origin)
        })
    }
}

impl OriginPool {
    /// Appends each file's packed origins in order, re-encoded against the
    /// pool's `descriptors`: the same records [`Self::extend`] appends for
    /// the decoded origins. Files are decoded a window at a time.
    pub fn extend_packed<'a>(
        &mut self,
        files: impl IntoIterator<Item = (&'a PackedOrigins, &'a SourceDescriptors)>,
        descriptors: &SourceDescriptors,
        budget: &mut Budget<'_>,
    ) -> Result<(), SourceError> {
        let pass = Budget::new(budget.cancel, &LIMITS);
        let mut window = Vec::new();
        for (packed, local) in files {
            for origin in packed.unpack(local, &pass) {
                window.push(origin?);
            }
            if window.len() >= parallel::WINDOW {
                self.extend(&window.iter().collect::<Vec<_>>(), descriptors, budget)?;
                window.clear();
            }
        }
        self.extend(&window.iter().collect::<Vec<_>>(), descriptors, budget)
    }
}
