//! A minimal protobuf reader for the SCIP messages the precise tier reads:
//! `Index.documents`, `Document.{relative_path, occurrences, position_encoding}`
//! and `Occurrence.{range, symbol, symbol_roles}`. Every other field,
//! `enclosing_range` and the symbol tables included, is skipped by wire type.
//! Malformed input is a [`ScipError`], never a panic.

use std::fmt;

/// `Index.documents`.
const INDEX_DOCUMENTS: u64 = 2;
/// `Document.relative_path`, `Document.occurrences`, `Document.position_encoding`
/// (scip.proto: `symbols = 3`, `language = 4`, `text = 5` come between).
const DOCUMENT_PATH: u64 = 1;
const DOCUMENT_OCCURRENCES: u64 = 2;
const DOCUMENT_ENCODING: u64 = 6;
/// `Occurrence.range`, `.symbol`, `.symbol_roles`.
const OCCURRENCE_RANGE: u64 = 1;
const OCCURRENCE_SYMBOL: u64 = 2;
const OCCURRENCE_ROLES: u64 = 3;

/// Why a SCIP index could not be read.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ScipError {
    /// The input ends inside a field.
    Truncated { at: usize },
    /// A varint longer than ten bytes.
    VarintOverflow { at: usize },
    /// A wire type the format does not use here (groups, 6, 7).
    WireType { wire: u64, at: usize },
    /// A length that runs past the end of its message.
    Length { at: usize },
    /// A string field that is not UTF-8.
    Utf8 { at: usize },
    /// An occurrence range of neither three nor four numbers, or a negative
    /// or oversized one.
    Range { at: usize },
}

impl fmt::Display for ScipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated { at } => write!(f, "SCIP index truncated at byte {at}"),
            Self::VarintOverflow { at } => write!(f, "SCIP varint overflows at byte {at}"),
            Self::WireType { wire, at } => {
                write!(f, "SCIP wire type {wire} unsupported at byte {at}")
            }
            Self::Length { at } => write!(f, "SCIP length runs past its message at byte {at}"),
            Self::Utf8 { at } => write!(f, "SCIP string is not UTF-8 at byte {at}"),
            Self::Range { at } => write!(f, "SCIP occurrence range is malformed at byte {at}"),
        }
    }
}

impl std::error::Error for ScipError {}

/// How a document counts the columns of its ranges (`PositionEncoding`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PositionEncoding {
    Utf8,
    Utf16,
    Utf32,
}

impl PositionEncoding {
    /// scip.proto: 0 unspecified, 1 UTF-8 bytes, 2 UTF-16 units, 3 UTF-32
    /// scalars. Unspecified and unknown values are read as UTF-16, the
    /// encoding SCIP indexers have used by default.
    fn of(value: u64) -> Self {
        match value {
            1 => Self::Utf8,
            3 => Self::Utf32,
            _ => Self::Utf16,
        }
    }
}

/// One occurrence as written: zero-based start line and column, end line
/// and column in the document's encoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RawOccurrence<'a> {
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
    pub symbol: &'a str,
    pub roles: u32,
}

#[derive(Debug)]
pub(crate) struct RawDocument<'a> {
    pub path: &'a str,
    pub encoding: PositionEncoding,
    pub occurrences: Vec<RawOccurrence<'a>>,
}

struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn done(&self) -> bool {
        self.at >= self.bytes.len()
    }

    fn varint(&mut self) -> Result<u64, ScipError> {
        let start = self.at;
        let mut value = 0u64;
        for shift in (0..70).step_by(7) {
            let byte = *self
                .bytes
                .get(self.at)
                .ok_or(ScipError::Truncated { at: self.at })?;
            self.at += 1;
            if shift == 63 && byte > 1 {
                return Err(ScipError::VarintOverflow { at: start });
            }
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(ScipError::VarintOverflow { at: start })
    }

    /// The next length-delimited payload.
    fn payload(&mut self) -> Result<&'a [u8], ScipError> {
        let at = self.at;
        let len = usize::try_from(self.varint()?).map_err(|_| ScipError::Length { at })?;
        let end = self
            .at
            .checked_add(len)
            .filter(|end| *end <= self.bytes.len())
            .ok_or(ScipError::Length { at })?;
        let payload = &self.bytes[self.at..end];
        self.at = end;
        Ok(payload)
    }

    fn text(&mut self) -> Result<&'a str, ScipError> {
        let at = self.at;
        std::str::from_utf8(self.payload()?).map_err(|_| ScipError::Utf8 { at })
    }

    fn skip(&mut self, wire: u64) -> Result<(), ScipError> {
        let width = match wire {
            0 => return self.varint().map(drop),
            1 => 8,
            2 => return self.payload().map(drop),
            5 => 4,
            wire => return Err(ScipError::WireType { wire, at: self.at }),
        };
        let end = self.at + width;
        if end > self.bytes.len() {
            return Err(ScipError::Truncated { at: self.at });
        }
        self.at = end;
        Ok(())
    }

    /// The next `(field, wire)`, or `None` at the end.
    fn tag(&mut self) -> Result<Option<(u64, u64)>, ScipError> {
        if self.done() {
            return Ok(None);
        }
        let tag = self.varint()?;
        Ok(Some((tag >> 3, tag & 7)))
    }
}

/// Every document of a serialized `Index`.
pub(crate) fn documents(bytes: &[u8]) -> Result<Vec<RawDocument<'_>>, ScipError> {
    let mut cursor = Cursor { bytes, at: 0 };
    let mut documents = Vec::new();
    while let Some((field, wire)) = cursor.tag()? {
        if field == INDEX_DOCUMENTS && wire == 2 {
            let base = cursor.at;
            let payload = cursor.payload()?;
            documents.push(document(payload, base)?);
        } else {
            cursor.skip(wire)?;
        }
    }
    Ok(documents)
}

fn document(bytes: &[u8], base: usize) -> Result<RawDocument<'_>, ScipError> {
    let mut cursor = Cursor { bytes, at: 0 };
    let mut found = RawDocument {
        path: "",
        encoding: PositionEncoding::Utf16,
        occurrences: Vec::new(),
    };
    let fail = |error: ScipError| shift(error, base);
    while let Some((field, wire)) = cursor.tag().map_err(fail)? {
        match (field, wire) {
            (DOCUMENT_PATH, 2) => found.path = cursor.text().map_err(fail)?,
            (DOCUMENT_ENCODING, 0) => {
                found.encoding = PositionEncoding::of(cursor.varint().map_err(fail)?);
            }
            (DOCUMENT_OCCURRENCES, 2) => {
                let at = base + cursor.at;
                let payload = cursor.payload().map_err(fail)?;
                found.occurrences.push(occurrence(payload, at)?);
            }
            _ => cursor.skip(wire).map_err(fail)?,
        }
    }
    Ok(found)
}

fn occurrence(bytes: &[u8], base: usize) -> Result<RawOccurrence<'_>, ScipError> {
    let mut cursor = Cursor { bytes, at: 0 };
    let mut numbers = Vec::<u64>::new();
    let (mut symbol, mut roles) = ("", 0u32);
    let fail = |error: ScipError| shift(error, base);
    while let Some((field, wire)) = cursor.tag().map_err(fail)? {
        match (field, wire) {
            (OCCURRENCE_RANGE, 2) => {
                let mut packed = Cursor {
                    bytes: cursor.payload().map_err(fail)?,
                    at: 0,
                };
                while !packed.done() {
                    numbers.push(packed.varint().map_err(fail)?);
                }
            }
            // An unpacked repeated int32 is valid protobuf too.
            (OCCURRENCE_RANGE, 0) => numbers.push(cursor.varint().map_err(fail)?),
            (OCCURRENCE_SYMBOL, 2) => symbol = cursor.text().map_err(fail)?,
            (OCCURRENCE_ROLES, 0) => {
                roles = u32::try_from(cursor.varint().map_err(fail)?)
                    .map_err(|_| ScipError::Range { at: base })?;
            }
            _ => cursor.skip(wire).map_err(fail)?,
        }
    }
    let bad = || ScipError::Range { at: base };
    let to = |n: u64| u32::try_from(n).ok().filter(|n| *n <= i32::MAX as u32);
    let numbers = numbers.into_iter().map(to).collect::<Option<Vec<_>>>();
    let (start_line, start_col, end_line, end_col) = match *numbers.as_deref().ok_or_else(bad)? {
        [line, start, end] if end >= start => (line, start, line, end),
        [start_line, start_col, end_line, end_col]
            if (end_line, end_col) >= (start_line, start_col) =>
        {
            (start_line, start_col, end_line, end_col)
        }
        _ => return Err(bad()),
    };
    Ok(RawOccurrence {
        start_line,
        start_col,
        end_line,
        end_col,
        symbol,
        roles,
    })
}

/// An error positioned inside a message, positioned in the whole input.
fn shift(error: ScipError, base: usize) -> ScipError {
    match error {
        ScipError::Truncated { at } => ScipError::Truncated { at: base + at },
        ScipError::VarintOverflow { at } => ScipError::VarintOverflow { at: base + at },
        ScipError::WireType { wire, at } => ScipError::WireType {
            wire,
            at: base + at,
        },
        ScipError::Length { at } => ScipError::Length { at: base + at },
        ScipError::Utf8 { at } => ScipError::Utf8 { at: base + at },
        ScipError::Range { at } => ScipError::Range { at },
    }
}
