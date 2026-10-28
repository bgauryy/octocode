use std::borrow::Cow;

use super::{
    ByteRange, Encoding, MapBudget, OriginBranch, OriginalRange, SourceError, SourceSnapshot,
    UnmappedReason,
};

pub struct AdmittedSource<'a> {
    text: Cow<'a, str>,
    pub snapshot: Option<SourceSnapshot>,
    // Only UTF-16 needs non-affine scalar correspondence; UTF-8 is one interval.
    boundaries: Vec<(u32, u32)>,
}
impl<'a> AdmittedSource<'a> {
    pub fn from_bytes(
        bytes: &'a [u8],
        source_key: &str,
        budget: &mut MapBudget<'_>,
    ) -> Result<Self, SourceError> {
        budget.check()?;
        let utf16 = bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]);
        let reserved_decode = if utf16 {
            bytes
                .len()
                .checked_mul(3)
                .ok_or(SourceError::LimitExceeded("decoded capacity"))?
                / 2
                + 16
        } else {
            0
        };
        budget.allocate(reserved_decode)?;
        let admitted = super::super::admit(bytes, super::super::Decoding::Unicode)
            .map_err(SourceError::Refused)?;
        let (encoding, bom_bytes) = if bytes.starts_with(&[0xff, 0xfe]) {
            (Encoding::Utf16Le, 2)
        } else if bytes.starts_with(&[0xfe, 0xff]) {
            (Encoding::Utf16Be, 2)
        } else {
            (Encoding::Utf8, crate::text::bom_len(bytes) as u32)
        };
        let byte_length =
            u32::try_from(bytes.len()).map_err(|_| SourceError::LimitExceeded("source bytes"))?;
        u32::try_from(admitted.text.len())
            .map_err(|_| SourceError::LimitExceeded("decoded bytes"))?;
        if let Cow::Owned(text) = &admitted.text {
            budget.allocate(text.capacity().saturating_sub(reserved_decode))?;
        }
        let mut boundaries = Vec::new();
        if encoding != Encoding::Utf8 {
            let mut raw = bom_bytes;
            for (index, (decoded, ch)) in admitted.text.char_indices().enumerate() {
                if index % 256 == 0 {
                    budget.check()?;
                    reserve_boundary(&mut boundaries, budget)?;
                    boundaries.push((decoded as u32, raw));
                }
                raw += ch.len_utf16() as u32 * 2;
            }
            reserve_boundary(&mut boundaries, budget)?;
            boundaries.push((admitted.text.len() as u32, raw));
        }
        budget.allocate(source_key.len() + 128)?;
        let digest = crate::digest::sha256(bytes);
        let id = super::model::identity(&(
            "raw-source:1",
            source_key,
            &digest,
            byte_length,
            encoding,
            bom_bytes,
        ))?;
        Ok(Self {
            text: admitted.text,
            boundaries,
            snapshot: Some(SourceSnapshot {
                id,
                path: source_key.to_owned(),
                digest,
                byte_length,
                encoding,
                bom_bytes,
            }),
        })
    }
    pub fn from_text(text: &'a str) -> Self {
        Self {
            text: Cow::Borrowed(text),
            snapshot: None,
            boundaries: Vec::new(),
        }
    }
    pub fn owned_bytes(&self) -> usize {
        self.boundaries.capacity() * std::mem::size_of::<(u32, u32)>()
            + match &self.text {
                Cow::Owned(text) => text.capacity(),
                Cow::Borrowed(_) => 0,
            }
            + self.snapshot.as_ref().map_or(0, |s| {
                s.id.capacity() + s.path.capacity() + s.digest.capacity()
            })
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub(super) fn project(&self, range: ByteRange) -> Result<OriginBranch, SourceError> {
        self.validate(range)?;
        let Some(snapshot) = &self.snapshot else {
            return Ok(OriginBranch::Unmapped(UnmappedReason::RawSourceUnavailable));
        };
        let boundary = |at| {
            if snapshot.encoding == Encoding::Utf8 {
                Ok(at + snapshot.bom_bytes)
            } else {
                let index = self
                    .boundaries
                    .partition_point(|&(decoded, _)| decoded <= at)
                    .checked_sub(1)
                    .ok_or(SourceError::InvalidMap("scalar boundary"))?;
                let (decoded, raw) = self.boundaries[index];
                let suffix = self
                    .text
                    .get(decoded as usize..at as usize)
                    .ok_or(SourceError::InvalidMap("scalar boundary"))?;
                Ok(raw
                    + suffix
                        .chars()
                        .map(|ch| ch.len_utf16() as u32 * 2)
                        .sum::<u32>())
            }
        };
        let origin = OriginalRange {
            source_id: snapshot.id.clone(),
            raw_range: ByteRange::new(boundary(range.start_byte)?, boundary(range.end_byte)?),
        };
        Ok(if snapshot.encoding == Encoding::Utf8 {
            OriginBranch::Copied(origin)
        } else {
            OriginBranch::Transcoded(origin)
        })
    }
    pub(super) fn validate(&self, range: ByteRange) -> Result<(), SourceError> {
        self.text
            .get(range.start_byte as usize..range.end_byte as usize)
            .map(|_| ())
            .ok_or(SourceError::InvalidMap("decoded range"))
    }
}
fn reserve_boundary(
    boundaries: &mut Vec<(u32, u32)>,
    budget: &mut MapBudget<'_>,
) -> Result<(), SourceError> {
    if boundaries.len() == boundaries.capacity() {
        let old = boundaries.capacity();
        let growth = old.max(256);
        budget.allocate(
            growth
                .checked_mul(8)
                .ok_or(SourceError::LimitExceeded("boundaries"))?,
        )?;
        boundaries
            .try_reserve_exact(growth)
            .map_err(|_| SourceError::LimitExceeded("allocation"))?;
        if boundaries.capacity() > old + growth {
            budget.allocate((boundaries.capacity() - old - growth) * 8)?;
        }
    }
    Ok(())
}
