//! A BOM-marked UTF-16 source as the UTF-8 text the search matched: its lines
//! and markers are not in its raw bytes. Streamed, so a source of any size keeps
//! the read's bounds; a malformed unit reads as U+FFFD, as it did in the search.

use super::{CancellationCheck, VERIFY_CHUNK_BYTES, VerifyReadError};
use octocode_engine::security::SourceNames;
use std::io::{BufRead, Read};

#[derive(Clone, Copy)]
pub(super) enum ByteOrder {
    Little,
    Big,
}

/// Consume a leading UTF-16 byte-order mark and return its order. Bytes that
/// are no such mark stay unread.
pub(super) fn byte_order(reader: &mut impl BufRead) -> std::io::Result<Option<ByteOrder>> {
    let order = match reader.fill_buf()? {
        [0xff, 0xfe, ..] => ByteOrder::Little,
        [0xfe, 0xff, ..] => ByteOrder::Big,
        _ => return Ok(None),
    };
    reader.consume(2);
    Ok(Some(order))
}

/// Whether the text of the source `reader` holds, UTF-16 or not, names a Kubernetes
/// Secret or a JSON Web Key ([`SourceNames`]). Nothing is kept and no line is
/// split, so a source that names neither costs one read.
pub(super) fn names_an_object(
    mut reader: impl BufRead,
    cancel: &impl CancellationCheck,
) -> Result<bool, VerifyReadError> {
    match byte_order(&mut reader)? {
        Some(order) => read_names(Utf16Text::new(reader, order), cancel),
        None => read_names(reader, cancel),
    }
}

fn read_names(
    mut reader: impl Read,
    cancel: &impl CancellationCheck,
) -> Result<bool, VerifyReadError> {
    // No path: what a path's YAML says is the line-by-line pass's to find.
    let mut names = SourceNames::for_path("");
    let mut chunk = vec![0u8; VERIFY_CHUNK_BYTES];
    loop {
        cancel.check().map_err(VerifyReadError::Cancelled)?;
        let read = reader.read(&mut chunk)?;
        if read == 0 {
            return Ok(names.finish().objects.any());
        }
        names.feed(&chunk[..read]);
    }
}

/// [`Read`] of the UTF-8 text a UTF-16 stream decodes to.
pub(super) struct Utf16Text<R> {
    inner: R,
    order: ByteOrder,
    /// The first byte of a unit whose second has not arrived.
    odd: Option<u8>,
    /// A high surrogate waiting for its low half.
    high: Option<u16>,
    text: Vec<u8>,
    at: usize,
}

impl<R: BufRead> Utf16Text<R> {
    pub(super) fn new(inner: R, order: ByteOrder) -> Self {
        Self {
            inner,
            order,
            odd: None,
            high: None,
            text: Vec::new(),
            at: 0,
        }
    }

    fn push(&mut self, decoded: char) {
        let mut encoded = [0; 4];
        self.text
            .extend_from_slice(decoded.encode_utf8(&mut encoded).as_bytes());
    }

    fn unit(&mut self, unit: u16) {
        let high = self.high.take();
        if let Some(high) = high
            && (0xdc00..=0xdfff).contains(&unit)
        {
            let code = 0x1_0000 + ((u32::from(high) - 0xd800) << 10) + (u32::from(unit) - 0xdc00);
            self.push(char::from_u32(code).unwrap_or(char::REPLACEMENT_CHARACTER));
            return;
        }
        if high.is_some() {
            self.push(char::REPLACEMENT_CHARACTER);
        }
        if (0xd800..=0xdbff).contains(&unit) {
            self.high = Some(unit);
        } else {
            self.push(char::from_u32(u32::from(unit)).unwrap_or(char::REPLACEMENT_CHARACTER));
        }
    }
}

impl<R: BufRead> Read for Utf16Text<R> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        while self.at == self.text.len() {
            self.text.clear();
            self.at = 0;
            let chunk = self.inner.fill_buf()?.to_vec();
            if chunk.is_empty() {
                if self.odd.take().is_some() || self.high.take().is_some() {
                    self.push(char::REPLACEMENT_CHARACTER);
                    continue;
                }
                return Ok(0);
            }
            self.inner.consume(chunk.len());
            for byte in chunk {
                match self.odd.take() {
                    None => self.odd = Some(byte),
                    Some(first) => self.unit(match self.order {
                        ByteOrder::Little => u16::from_le_bytes([first, byte]),
                        ByteOrder::Big => u16::from_be_bytes([first, byte]),
                    }),
                }
            }
        }
        let taken = out.len().min(self.text.len() - self.at);
        out[..taken].copy_from_slice(&self.text[self.at..self.at + taken]);
        self.at += taken;
        Ok(taken)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decoded(bytes: &[u8], order: ByteOrder) -> String {
        let mut text = String::new();
        Utf16Text::new(bytes, order)
            .read_to_string(&mut text)
            .expect("decodes");
        text
    }

    /// A reader that hands out a few bytes at a time, so a literal falls across
    /// reads wherever it sits.
    struct Trickle<'a>(&'a [u8]);

    impl Read for Trickle<'_> {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            let taken = self.0.len().min(out.len()).min(3);
            out[..taken].copy_from_slice(&self.0[..taken]);
            self.0 = &self.0[taken..];
            Ok(taken)
        }
    }

    #[test]
    fn a_name_is_found_across_reads_and_in_utf16() {
        let cancel = crate::execution::cancel::NeverCancel;
        let found = |bytes: &[u8]| {
            let reader = std::io::BufReader::new(Trickle(bytes));
            matches!(names_an_object(reader, &cancel), Ok(true))
        };
        assert!(found(b"{\"kind\": \"Secret\"}"));
        assert!(found(b"{\"kty\": \"EC\"}"));
        assert!(!found(b"a kty, a Secret, and a ky, in \"text\""));
        let mut wide = vec![0xff, 0xfe];
        wide.extend(
            "{\"kind\": \"Secret\"}"
                .encode_utf16()
                .flat_map(u16::to_le_bytes),
        );
        assert!(found(&wide));
        // Past a whole chunk, and cut by its edge.
        let mut long = vec![b' '; VERIFY_CHUNK_BYTES - 2];
        long.extend_from_slice(b"kty: \"EC\"");
        let reader = std::io::BufReader::new(long.as_slice());
        assert!(matches!(names_an_object(reader, &cancel), Ok(true)));
    }

    #[test]
    fn it_reads_what_the_engine_decodes_in_either_order() {
        let text = "line é 😀\r\nnext\n";
        let little = text
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        let big = text
            .encode_utf16()
            .flat_map(u16::to_be_bytes)
            .collect::<Vec<_>>();
        assert_eq!(decoded(&little, ByteOrder::Little), text);
        assert_eq!(decoded(&big, ByteOrder::Big), text);
    }

    /// One byte at a time, as a short read may hand them, changes nothing.
    #[test]
    fn a_unit_split_across_reads_still_decodes() {
        let text = "😀a";
        let bytes = text
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        struct OneByOne<'a>(&'a [u8]);
        impl Read for OneByOne<'_> {
            fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
                let one = out.len().min(1);
                self.0.read(&mut out[..one])
            }
        }
        let reader = std::io::BufReader::with_capacity(1, OneByOne(&bytes));
        let mut out = String::new();
        Utf16Text::new(reader, ByteOrder::Little)
            .read_to_string(&mut out)
            .expect("decodes");
        assert_eq!(out, text);
    }

    #[test]
    fn a_malformed_unit_reads_as_a_replacement() {
        let lone_high = [0x3d, 0xd8, b'a', 0];
        assert_eq!(decoded(&lone_high, ByteOrder::Little), "\u{fffd}a");
        let cut_off = [b'a', 0, b'b'];
        assert_eq!(decoded(&cut_off, ByteOrder::Little), "a\u{fffd}");
    }
}
