//! The packed encoding's writer: LEB128 numbers and length-prefixed values,
//! into a buffer ([`Writer`]) or straight into a hasher ([`Identity`]).

use octocode_engine::digest::HashWriter;
use octocode_engine::types::TextSearchFile;
use std::io::Write;
use std::time::SystemTime;

pub(super) struct Writer<W: Write> {
    sink: W,
    written: usize,
    max: usize,
}

impl<W: Write> Writer<W> {
    /// The sink everything was written into.
    pub(super) fn into_sink(self) -> W {
        self.sink
    }

    /// A writer into `sink` that refuses to pass `max` bytes.
    pub(super) fn new(sink: W, max: usize) -> Self {
        Self {
            sink,
            written: 0,
            max,
        }
    }

    fn bytes(&mut self, bytes: &[u8]) -> Option<()> {
        self.written = self.written.checked_add(bytes.len())?;
        (self.written <= self.max).then_some(())?;
        self.sink.write_all(bytes).ok()
    }

    /// LEB128: seven bits a byte, low first, high bit set on all but the last.
    fn number(&mut self, mut value: u64) -> Option<()> {
        let mut buffer = [0u8; 10];
        let mut len = 0;
        loop {
            let low = (value & 0x7f) as u8;
            value >>= 7;
            if value == 0 {
                buffer[len] = low;
                len += 1;
                break;
            }
            buffer[len] = low | 0x80;
            len += 1;
        }
        self.bytes(&buffer[..len])
    }

    fn text(&mut self, text: &str) -> Option<()> {
        self.number(text.len() as u64)?;
        self.bytes(text.as_bytes())
    }

    /// `None` as 0, `Some(n)` as n + 1.
    fn optional(&mut self, value: Option<u32>) -> Option<()> {
        self.number(value.map_or(0, |n| u64::from(n) + 1))
    }

    /// Every field of `file`; its source's size and time only when
    /// `stamps` (an identity binds the source by its digest alone).
    pub(super) fn file(
        &mut self,
        file: &TextSearchFile,
        stamps: bool,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<()> {
        self.text(&file.path)?;
        self.number(u64::from(file.match_count))?;
        self.number(file.term_counts.len() as u64)?;
        for count in &file.term_counts {
            self.number(u64::from(*count))?;
        }
        match &file.source {
            None => self.number(0)?,
            Some(source) => {
                self.number(1)?;
                if stamps {
                    self.number(source.size)?;
                    self.time(source.modified)?;
                }
                self.bytes(&source.digest)?;
            }
        }
        self.number(file.matches.len() as u64)?;
        let mut classes = std::collections::HashMap::new();
        for hit in &file.matches {
            if cancelled() {
                return None;
            }
            self.number(u64::from(hit.line))?;
            self.number(u64::from(hit.column))?;
            self.number(u64::from(hit.byte_offset))?;
            self.optional(hit.deferred)?;
            self.text(&hit.value)?;
            if stamps && hit.deferred.is_some() {
                let next = classes.len() as u64;
                let class = *classes.entry(hit.value_digest).or_insert(next);
                self.number(class)?;
            } else {
                self.bytes(&hit.value_digest)?;
            }
            for value in [hit.last_line, hit.count, hit.rank] {
                self.optional(value)?;
            }
        }
        Some(())
    }

    /// No time as 0; a time at or after the epoch as 1, before it as 2,
    /// then the distance in seconds and nanoseconds.
    fn time(&mut self, time: Option<SystemTime>) -> Option<()> {
        let Some(time) = time else {
            return self.number(0);
        };
        let (tag, distance) = match time.duration_since(SystemTime::UNIX_EPOCH) {
            Ok(after) => (1, after),
            Err(before) => (2, before.duration()),
        };
        self.number(tag)?;
        self.number(distance.as_secs())?;
        self.number(u64::from(distance.subsec_nanos()))
    }
}

/// SHA-256 over a scan written in the packed encoding (every value is
/// length-prefixed, so distinct inputs write distinct bytes), streamed into
/// the hasher: hashing a scan of any size keeps no copy of it.
pub struct Identity(Writer<HashWriter>);

impl Default for Identity {
    fn default() -> Self {
        Self(Writer::new(HashWriter::default(), usize::MAX))
    }
}

impl Identity {
    pub fn text(&mut self, text: &str) -> &mut Self {
        self.0.text(text);
        self
    }

    pub fn number(&mut self, value: u64) -> &mut Self {
        self.0.number(value);
        self
    }

    /// `None` as 0, `Some(n)` as n + 1.
    pub fn optional(&mut self, value: Option<u32>) -> &mut Self {
        self.0.optional(value);
        self
    }

    /// `None` as 0, `Some(text)` as 1 then the text.
    pub fn optional_text(&mut self, text: Option<&str>) -> &mut Self {
        match text {
            None => self.number(0),
            Some(text) => self.number(1).text(text),
        }
    }

    /// Every field of `file` except its source's size and time: the source
    /// is bound by the digest of its bytes.
    pub fn file(&mut self, file: &TextSearchFile) -> &mut Self {
        self.0.file(file, false, &|| false);
        self
    }

    /// Lowercase-hex SHA-256 of everything written.
    pub fn finish(self) -> String {
        hex::encode(self.0.sink.finalize())
    }
}
