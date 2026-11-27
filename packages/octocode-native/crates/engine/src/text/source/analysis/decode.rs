use std::borrow::Cow;

use super::{
    ByteRange, Encoding, MapBudget, OriginBranch, OriginalRange, SourceError, SourceSnapshot,
    UnmappedReason,
};
use crate::text::Charset;

pub struct AdmittedSource<'a> {
    text: Cow<'a, str>,
    pub snapshot: Option<SourceSnapshot>,
    // Only UTF-16 needs non-affine scalar correspondence; UTF-8 is one interval.
    boundaries: Vec<(u32, u32)>,
    /// Why the text is not the file's UTF-8 ([`crate::text::LATIN1_WARNING`],
    /// [`crate::text::LOSSY_WARNING`]): a tool states it with its rows.
    warning: Option<&'static str>,
    /// SHA-256 of `text`, computed at most once ([`Self::text_digest`]).
    text_digest: std::sync::OnceLock<String>,
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
        let admitted = super::super::admit(bytes, super::super::Decoding::Lenient)
            .map_err(SourceError::Refused)?;
        let (encoding, bom_bytes) = if bytes.starts_with(&[0xff, 0xfe]) {
            (Encoding::Utf16Le, 2)
        } else if bytes.starts_with(&[0xfe, 0xff]) {
            (Encoding::Utf16Be, 2)
        } else {
            let encoding = match admitted.charset {
                Charset::Latin1 => Encoding::Latin1,
                Charset::Lossy => Encoding::Lossy,
                Charset::Utf8 | Charset::Utf16 => Encoding::Utf8,
            };
            (
                encoding,
                crate::num::saturating_u32(crate::text::bom_len(bytes)),
            )
        };
        let warning = match encoding {
            Encoding::Latin1 | Encoding::Lossy => admitted.warning,
            _ => None,
        };
        let byte_length =
            u32::try_from(bytes.len()).map_err(|_| SourceError::LimitExceeded("source bytes"))?;
        let decoded_length = u32::try_from(admitted.text.len())
            .map_err(|_| SourceError::LimitExceeded("decoded bytes"))?;
        if let Cow::Owned(text) = &admitted.text {
            budget.allocate(text.capacity().saturating_sub(reserved_decode))?;
        }
        let mut boundaries = Vec::new();
        if encoding == Encoding::Lossy {
            // Valid runs are one interval each; a replacement char stands for
            // a run of stray bytes, so it and the text after it start one.
            let mut raw = bom_bytes;
            let mut decoded = 0_u32;
            for chunk in bytes[bom_bytes as usize..].utf8_chunks() {
                budget.check()?;
                reserve_boundary(&mut boundaries, budget)?;
                boundaries.push((decoded, raw));
                let valid = crate::num::saturating_u32(chunk.valid().len());
                decoded += valid;
                raw += valid;
                if !chunk.invalid().is_empty() {
                    reserve_boundary(&mut boundaries, budget)?;
                    boundaries.push((decoded, raw));
                    decoded += crate::num::saturating_u32('\u{fffd}'.len_utf8());
                    raw += crate::num::saturating_u32(chunk.invalid().len());
                }
            }
            reserve_boundary(&mut boundaries, budget)?;
            boundaries.push((decoded_length, raw));
        } else if encoding != Encoding::Utf8 {
            let mut raw = bom_bytes;
            for (index, (decoded, ch)) in admitted.text.char_indices().enumerate() {
                if index % 256 == 0 {
                    budget.check()?;
                    reserve_boundary(&mut boundaries, budget)?;
                    // `decoded` < `decoded_length`, which fits a `u32`.
                    boundaries.push((crate::num::saturating_u32(decoded), raw));
                }
                raw += raw_bytes(encoding, ch);
            }
            reserve_boundary(&mut boundaries, budget)?;
            boundaries.push((decoded_length, raw));
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
        // UTF-8 text borrowed whole is the source bytes: one digest names both.
        let text_digest = std::sync::OnceLock::new();
        if encoding == Encoding::Utf8
            && matches!(admitted.text, Cow::Borrowed(text) if text.len() == bytes.len())
        {
            let _ = text_digest.set(digest.clone());
        }
        Ok(Self {
            text: admitted.text,
            boundaries,
            warning,
            text_digest,
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
            warning: None,
            text_digest: std::sync::OnceLock::new(),
        }
    }
    /// The warning to state with rows read from this source, when its text
    /// is a legacy 8-bit decoding of the file ([`crate::text::decode`]).
    pub fn warning(&self) -> Option<&'static str> {
        self.warning
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
    /// This source as a read shows it ([`crate::structural::JudgeText`]):
    /// each of `hidden` (ascending, disjoint byte ranges of its text, each
    /// with the text a read shows in its place: a placeholder and the line
    /// breaks it keeps) replaced by that text, and the source admitted as the
    /// result, the shown text. Every byte offset, column and line counts the
    /// shown text, as a read serves it, and the snapshot (digest, length,
    /// identity) is the shown text's: nothing it states is computed over a
    /// hidden byte, so no digest or length tests a guess of one. The text
    /// matched is the shown text with each placeholder written over in
    /// place, its letters and digits first, then `x`, line breaks kept, so
    /// what a hidden value sits in (a string, a name) parses as it did.
    /// Returns the shown text too: what a match's texts are restated from
    /// ([`crate::structural::restore_texts`]).
    #[must_use]
    pub fn judged(self, hidden: &[(std::ops::Range<usize>, String)]) -> (Self, String) {
        let mut shown = String::with_capacity(self.text.len());
        let mut masked = String::with_capacity(self.text.len());
        let mut from = 0;
        for (range, placeholder) in hidden {
            let (Some(before), true) = (
                self.text.get(from..range.start),
                from <= range.start && range.end <= self.text.len(),
            ) else {
                continue;
            };
            shown.push_str(before);
            masked.push_str(before);
            shown.push_str(placeholder);
            let mut letters = placeholder.chars().filter(char::is_ascii_alphanumeric);
            masked.extend(placeholder.chars().map(|c| match c {
                '\n' | '\r' => c,
                _ if c.is_ascii() => letters.next().unwrap_or('x'),
                _ => match c.len_utf8() {
                    2 => '\u{e9}',
                    3 => '\u{30a2}',
                    _ => '\u{1d465}',
                },
            }));
            from = range.end;
        }
        shown.push_str(self.text.get(from..).unwrap_or_default());
        masked.push_str(self.text.get(from..).unwrap_or_default());
        let snapshot = self.snapshot.as_ref().map(|snapshot| {
            let digest = crate::digest::sha256(shown.as_bytes());
            let byte_length = crate::num::saturating_u32(shown.len());
            SourceSnapshot {
                id: super::model::identity(&(
                    "raw-source:1",
                    &snapshot.path,
                    &digest,
                    byte_length,
                    Encoding::Utf8,
                    0_u32,
                ))
                .unwrap_or_else(|_| digest.clone()),
                path: snapshot.path.clone(),
                digest,
                byte_length,
                encoding: Encoding::Utf8,
                bom_bytes: 0,
            }
        });
        let source = Self {
            text: Cow::Owned(masked),
            snapshot,
            boundaries: Vec::new(),
            warning: self.warning,
            text_digest: std::sync::OnceLock::new(),
        };
        (source, shown)
    }
    /// SHA-256 of [`Self::text`]: the source bytes' digest when the text is
    /// those bytes, else hashed once on first use.
    pub fn text_digest(&self) -> &str {
        self.text_digest
            .get_or_init(|| crate::digest::sha256(self.text.as_bytes()))
    }
    /// SHA-256 of the admitted source bytes; `None` for text admitted
    /// without them ([`Self::from_text`]).
    pub fn source_digest(&self) -> Option<&str> {
        self.snapshot
            .as_ref()
            .map(|snapshot| snapshot.digest.as_str())
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
                        .map(|ch| raw_bytes(snapshot.encoding, ch))
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

/// Raw bytes that encode `ch` in a source of `encoding`: a surrogate pair
/// past the BMP in UTF-16, one in Latin-1, its own UTF-8 in the valid runs of
/// lossy text (a replacement char never lies inside a measured run).
fn raw_bytes(encoding: Encoding, ch: char) -> u32 {
    match encoding {
        Encoding::Latin1 => 1,
        Encoding::Utf8 | Encoding::Lossy => crate::num::saturating_u32(ch.len_utf8()),
        Encoding::Utf16Le | Encoding::Utf16Be => {
            if u32::from(ch) > 0xFFFF {
                4
            } else {
                2
            }
        }
    }
}
