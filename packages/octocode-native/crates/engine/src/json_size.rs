//! JSON wire sizes without rendering, and retained heap estimates.
use serde::Serialize;
use std::io::{self, Write};
use std::mem::size_of;

/// Approximate allocator rounding and bookkeeping for one heap block.
const BLOCK_BYTES: usize = 16;

/// An object's entry: its hash, key and value, index slot and key allocation.
pub const OBJECT_ENTRY_BYTES: usize =
    size_of::<(u64, String, serde_json::Value)>() + size_of::<usize>() + BLOCK_BYTES;

/// About the heap below a JSON value's slot, including structural overhead.
/// Lengths, rather than capacities, keep a scan's budget a fact of its input.
/// Serialized bytes alone undercount objects with many small fields.
pub fn heap_bytes(value: &serde_json::Value) -> usize {
    use serde_json::Value;
    match value {
        Value::String(text) => BLOCK_BYTES.saturating_add(text.len()),
        Value::Array(items) => items.iter().fold(
            BLOCK_BYTES.saturating_add(items.len().saturating_mul(size_of::<Value>())),
            |bytes, item| bytes.saturating_add(heap_bytes(item)),
        ),
        Value::Object(fields) => fields.iter().fold(BLOCK_BYTES, |bytes, (key, field)| {
            bytes
                .saturating_add(OBJECT_ENTRY_BYTES)
                .saturating_add(key.len())
                .saturating_add(heap_bytes(field))
        }),
        Value::Null | Value::Bool(_) | Value::Number(_) => 0,
    }
}

struct Counter {
    size: usize,
    utf16: bool,
}

/// Why [`Size`] stopped.
#[derive(Debug)]
enum SizeError {
    /// The value serializes, but not into a size this serializer can add up.
    Fallback,
    /// `serde_json` would fail on it too.
    Failed,
}

impl std::fmt::Display for SizeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("size serializer stopped")
    }
}

impl std::error::Error for SizeError {}

impl serde::ser::Error for SizeError {
    fn custom<T: std::fmt::Display>(_: T) -> Self {
        Self::Failed
    }
}

/// A serializer whose output is the size of the compact JSON that
/// `serde_json::to_vec` writes for the same value: its bytes, or with
/// `utf16` its UTF-16 code units (what a JavaScript reader's `length` counts).
struct Size {
    total: usize,
    utf16: bool,
}

/// What each byte of a string adds to its JSON form, by [`Size::utf16`]: an
/// escaped byte its escape (`\n` 2, `\u0001` 6), any other ASCII byte 1. In
/// bytes every other byte is 1; in UTF-16 units a scalar's lead byte is 1 (2
/// for a four-byte scalar, a surrogate pair) and a continuation byte 0. An
/// escape is ASCII, so the two measures differ only on non-ASCII bytes.
const fn string_weights(utf16: bool) -> [u8; 256] {
    let mut weights = [1; 256];
    let mut byte = 0;
    while byte < 256 {
        weights[byte] = match byte {
            0x08 | 0x09 | 0x0a | 0x0c | 0x0d | 0x22 | 0x5c => 2,
            0..0x20 => 6,
            0x80..0xc0 if utf16 => 0,
            0xf0.. if utf16 => 2,
            _ => 1,
        };
        byte += 1;
    }
    weights
}

static BYTE_WEIGHTS: [u8; 256] = string_weights(false);
static UTF16_WEIGHTS: [u8; 256] = string_weights(true);

/// What each byte of raw UTF-8 text adds to its UTF-16 length: a scalar's
/// lead byte 1 (2 for a four-byte scalar), a continuation byte 0.
static RAW_UTF16_WEIGHTS: [u8; 256] = {
    let mut weights = [1; 256];
    let mut byte = 0x80;
    while byte < 256 {
        weights[byte] = match byte {
            0x80..0xc0 => 0,
            0xf0.. => 2,
            _ => 1,
        };
        byte += 1;
    }
    weights
};

/// The size of `text` as a JSON string, quotes and escapes included: what
/// `serde_json` writes for it, in bytes or UTF-16 units.
fn string_size(text: &str, utf16: bool) -> usize {
    2 + weigh(
        text.as_bytes(),
        if utf16 { &UTF16_WEIGHTS } else { &BYTE_WEIGHTS },
    )
}

/// The sum of each byte's weight. A table lookup per byte, eight bytes
/// summed per step: the one loop under every response measure, written with
/// plain indexing and casts (no iterator adapters, closures or `From` calls)
/// so an unoptimized build runs it about as fast as the table allows.
fn weigh(bytes: &[u8], weights: &[u8; 256]) -> usize {
    let mut total = 0;
    let mut at = 0;
    while at + 8 <= bytes.len() {
        let chunk = &bytes[at..at + 8];
        // Eight weights of at most 6 each fit a byte.
        total += (weights[chunk[0] as usize]
            + weights[chunk[1] as usize]
            + weights[chunk[2] as usize]
            + weights[chunk[3] as usize]
            + weights[chunk[4] as usize]
            + weights[chunk[5] as usize]
            + weights[chunk[6] as usize]
            + weights[chunk[7] as usize]) as usize;
        at += 8;
    }
    while at < bytes.len() {
        total += weights[bytes[at] as usize] as usize;
        at += 1;
    }
    total
}

/// Decimal digits of `value`.
fn digits(value: u128) -> usize {
    value.checked_ilog10().map_or(1, |log| log as usize + 1)
}

/// A sequence, map or struct body in progress.
struct Body<'a> {
    size: &'a mut Size,
    first: bool,
    /// Bytes added when the body closes: its bracket, and the brace of the
    /// enum variant around it.
    close: usize,
}

impl Body<'_> {
    fn comma(&mut self) {
        if !std::mem::replace(&mut self.first, false) {
            self.size.total += 1;
        }
    }

    fn element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), SizeError> {
        self.comma();
        value.serialize(&mut *self.size)
    }

    fn field<T: ?Sized + Serialize>(&mut self, key: &str, value: &T) -> Result<(), SizeError> {
        self.comma();
        self.size.total += string_size(key, self.size.utf16) + 1;
        value.serialize(&mut *self.size)
    }

    fn close(self) -> Result<(), SizeError> {
        self.size.total += self.close;
        Ok(())
    }
}

macro_rules! signed {
    ($($method:ident: $ty:ty),*) => {$(
        fn $method(self, value: $ty) -> Result<(), SizeError> {
            self.total += digits(u128::from(value.unsigned_abs())) + usize::from(value < 0);
            Ok(())
        }
    )*};
}

macro_rules! unsigned {
    ($($method:ident: $ty:ty),*) => {$(
        fn $method(self, value: $ty) -> Result<(), SizeError> {
            self.total += digits(u128::from(value));
            Ok(())
        }
    )*};
}

impl<'a> serde::Serializer for &'a mut Size {
    type Ok = ();
    type Error = SizeError;
    type SerializeSeq = Body<'a>;
    type SerializeTuple = Body<'a>;
    type SerializeTupleStruct = Body<'a>;
    type SerializeTupleVariant = Body<'a>;
    type SerializeMap = Body<'a>;
    type SerializeStruct = Body<'a>;
    type SerializeStructVariant = Body<'a>;

    signed!(serialize_i8: i8, serialize_i16: i16, serialize_i32: i32, serialize_i64: i64);
    unsigned!(serialize_u8: u8, serialize_u16: u16, serialize_u32: u32, serialize_u64: u64);

    fn serialize_i128(self, value: i128) -> Result<(), SizeError> {
        self.total += digits(value.unsigned_abs()) + usize::from(value < 0);
        Ok(())
    }

    fn serialize_u128(self, value: u128) -> Result<(), SizeError> {
        self.total += digits(value);
        Ok(())
    }

    fn serialize_bool(self, value: bool) -> Result<(), SizeError> {
        self.total += if value { 4 } else { 5 };
        Ok(())
    }

    // Floats are formatted by `serde_json` (shortest round trip, `null` for a
    // non-finite one); asking it keeps the size exact.
    fn serialize_f32(self, value: f32) -> Result<(), SizeError> {
        self.total += count(&value, false).ok_or(SizeError::Failed)?;
        Ok(())
    }

    fn serialize_f64(self, value: f64) -> Result<(), SizeError> {
        self.total += count(&value, false).ok_or(SizeError::Failed)?;
        Ok(())
    }

    fn serialize_char(self, value: char) -> Result<(), SizeError> {
        self.serialize_str(value.encode_utf8(&mut [0; 4]))
    }

    fn serialize_str(self, value: &str) -> Result<(), SizeError> {
        self.total += string_size(value, self.utf16);
        Ok(())
    }

    fn serialize_bytes(self, value: &[u8]) -> Result<(), SizeError> {
        // An array of numbers, as `serde_json` writes bytes.
        self.total += 2 + value.len().saturating_sub(1);
        self.total += value
            .iter()
            .map(|&byte| digits(u128::from(byte)))
            .sum::<usize>();
        Ok(())
    }

    fn serialize_none(self) -> Result<(), SizeError> {
        self.serialize_unit()
    }

    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<(), SizeError> {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<(), SizeError> {
        self.total += 4;
        Ok(())
    }

    fn serialize_unit_struct(self, _: &'static str) -> Result<(), SizeError> {
        self.serialize_unit()
    }

    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
    ) -> Result<(), SizeError> {
        self.serialize_str(variant)
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        value: &T,
    ) -> Result<(), SizeError> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<(), SizeError> {
        // {"variant":value}
        self.total += 1 + string_size(variant, self.utf16) + 1;
        value.serialize(&mut *self)?;
        self.total += 1;
        Ok(())
    }

    fn serialize_seq(self, _: Option<usize>) -> Result<Body<'a>, SizeError> {
        self.total += 1;
        Ok(Body {
            size: self,
            first: true,
            close: 1,
        })
    }

    fn serialize_tuple(self, length: usize) -> Result<Body<'a>, SizeError> {
        self.serialize_seq(Some(length))
    }

    fn serialize_tuple_struct(self, _: &'static str, length: usize) -> Result<Body<'a>, SizeError> {
        self.serialize_seq(Some(length))
    }

    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Body<'a>, SizeError> {
        // {"variant":[ ... ]}
        self.total += 1 + string_size(variant, self.utf16) + 1 + 1;
        Ok(Body {
            size: self,
            first: true,
            close: 2,
        })
    }

    fn serialize_map(self, _: Option<usize>) -> Result<Body<'a>, SizeError> {
        self.total += 1;
        Ok(Body {
            size: self,
            first: true,
            close: 1,
        })
    }

    fn serialize_struct(self, name: &'static str, length: usize) -> Result<Body<'a>, SizeError> {
        // `serde_json`'s own raw-value and number structs write their payload
        // bare.
        if name.starts_with("$serde_json::private::") {
            return Err(SizeError::Fallback);
        }
        self.serialize_map(Some(length))
    }

    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Body<'a>, SizeError> {
        // {"variant":{ ... }}
        self.total += 1 + string_size(variant, self.utf16) + 1 + 1;
        Ok(Body {
            size: self,
            first: true,
            close: 2,
        })
    }

    fn collect_str<T: ?Sized + std::fmt::Display>(self, value: &T) -> Result<(), SizeError> {
        self.serialize_str(&value.to_string())
    }
}

macro_rules! sequence_body {
    ($($trait:ident::$method:ident),*) => {$(
        impl serde::ser::$trait for Body<'_> {
            type Ok = ();
            type Error = SizeError;
            fn $method<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), SizeError> {
                self.element(value)
            }
            fn end(self) -> Result<(), SizeError> {
                self.close()
            }
        }
    )*};
}

sequence_body!(
    SerializeSeq::serialize_element,
    SerializeTuple::serialize_element,
    SerializeTupleStruct::serialize_field,
    SerializeTupleVariant::serialize_field
);

macro_rules! field_body {
    ($($trait:ident),*) => {$(
        impl serde::ser::$trait for Body<'_> {
            type Ok = ();
            type Error = SizeError;
            fn serialize_field<T: ?Sized + Serialize>(
                &mut self,
                key: &'static str,
                value: &T,
            ) -> Result<(), SizeError> {
                self.field(key, value)
            }
            fn end(self) -> Result<(), SizeError> {
                self.close()
            }
        }
    )*};
}

field_body!(SerializeStruct, SerializeStructVariant);

impl serde::ser::SerializeMap for Body<'_> {
    type Ok = ();
    type Error = SizeError;
    fn serialize_key<T: ?Sized + Serialize>(&mut self, key: &T) -> Result<(), SizeError> {
        self.comma();
        self.size.total += key.serialize(StringKey(self.size.utf16))? + 1;
        Ok(())
    }
    fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), SizeError> {
        value.serialize(&mut *self.size)
    }
    fn end(self) -> Result<(), SizeError> {
        self.close()
    }
}

/// A map key: the bytes of a string key. Any other key (`serde_json` writes
/// numbers quoted and refuses the rest) is left to the rendering path.
struct StringKey(bool);

macro_rules! other_key {
    ($($method:ident($($arg:ty),*))*) => {$(
        fn $method(self $(, _: $arg)*) -> Result<usize, SizeError> {
            Err(SizeError::Fallback)
        }
    )*};
}

macro_rules! other_compound {
    ($($method:ident($($arg:ty),*))*) => {$(
        fn $method(self $(, _: $arg)*) -> Result<serde::ser::Impossible<usize, SizeError>, SizeError> {
            Err(SizeError::Fallback)
        }
    )*};
}

impl serde::Serializer for StringKey {
    type Ok = usize;
    type Error = SizeError;
    type SerializeSeq = serde::ser::Impossible<usize, SizeError>;
    type SerializeTuple = serde::ser::Impossible<usize, SizeError>;
    type SerializeTupleStruct = serde::ser::Impossible<usize, SizeError>;
    type SerializeTupleVariant = serde::ser::Impossible<usize, SizeError>;
    type SerializeMap = serde::ser::Impossible<usize, SizeError>;
    type SerializeStruct = serde::ser::Impossible<usize, SizeError>;
    type SerializeStructVariant = serde::ser::Impossible<usize, SizeError>;

    fn serialize_str(self, value: &str) -> Result<usize, SizeError> {
        Ok(string_size(value, self.0))
    }

    other_key! {
        serialize_bool(bool) serialize_i8(i8) serialize_i16(i16) serialize_i32(i32)
        serialize_i64(i64) serialize_i128(i128) serialize_u8(u8) serialize_u16(u16)
        serialize_u32(u32) serialize_u64(u64) serialize_u128(u128) serialize_f32(f32)
        serialize_f64(f64) serialize_char(char) serialize_bytes(&[u8]) serialize_none()
        serialize_unit() serialize_unit_struct(&'static str)
        serialize_unit_variant(&'static str, u32, &'static str)
    }

    other_compound! {
        serialize_seq(Option<usize>) serialize_tuple(usize)
        serialize_tuple_struct(&'static str, usize)
        serialize_tuple_variant(&'static str, u32, &'static str, usize)
        serialize_map(Option<usize>) serialize_struct(&'static str, usize)
        serialize_struct_variant(&'static str, u32, &'static str, usize)
    }

    fn serialize_some<T: ?Sized + Serialize>(self, _: &T) -> Result<usize, SizeError> {
        Err(SizeError::Fallback)
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        _: &T,
    ) -> Result<usize, SizeError> {
        Err(SizeError::Fallback)
    }

    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: &T,
    ) -> Result<usize, SizeError> {
        Err(SizeError::Fallback)
    }

    fn collect_str<T: ?Sized + std::fmt::Display>(self, value: &T) -> Result<usize, SizeError> {
        Ok(string_size(&value.to_string(), self.0))
    }
}

/// UTF-16 units of UTF-8 `bytes`: one per scalar start byte, one more for
/// each four-byte scalar (a surrogate pair). Exact for any chunk that splits
/// at scalar boundaries or inside one: continuation bytes count nothing.
pub fn utf16_units_of_utf8(bytes: &[u8]) -> usize {
    weigh(bytes, &RAW_UTF16_WEIGHTS)
}

impl Write for Counter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.size += if self.utf16 {
            utf16_units_of_utf8(bytes)
        } else {
            bytes.len()
        };
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn count(value: &(impl Serialize + ?Sized), utf16: bool) -> Option<usize> {
    let mut counter = Counter { size: 0, utf16 };
    serde_json::to_writer(&mut counter, value).ok()?;
    Some(counter.size)
}

/// The compact JSON size of `value`, in bytes or UTF-16 units: added up by
/// [`Size`] without writing it (no escape buffers, no per-token
/// `write_all`). A value it cannot size exactly (a map key that is not a
/// string) is serialized and counted.
fn size(value: &(impl Serialize + ?Sized), utf16: bool) -> Option<usize> {
    let mut size = Size { total: 0, utf16 };
    match value.serialize(&mut size) {
        Ok(()) => Some(size.total),
        Err(SizeError::Fallback) => count(value, utf16),
        Err(SizeError::Failed) => None,
    }
}

/// JSON bytes, or `None` if serialization failed; a partial document has no size.
pub fn bytes(value: &(impl Serialize + ?Sized)) -> Option<usize> {
    size(value, false)
}

/// JSON UTF-16 units, including escaping, or `None` if serialization failed.
pub fn utf16_units(value: &(impl Serialize + ?Sized)) -> Option<usize> {
    size(value, true)
}

/// [`bytes`], or `fallback` when serialization failed: the one failure
/// policy. Pick `usize::MAX` where an unmeasured value must not count as
/// small (a size bound, a cache weight); sums over it saturate.
pub fn bytes_or(value: &(impl Serialize + ?Sized), fallback: usize) -> usize {
    bytes(value).unwrap_or(fallback)
}

/// [`utf16_units`], or `fallback` when serialization failed; see [`bytes_or`].
pub fn utf16_units_or(value: &(impl Serialize + ?Sized), fallback: usize) -> usize {
    utf16_units(value).unwrap_or(fallback)
}

/// UTF-16 units of `text` as a JSON string body: escaped, unquoted. A string
/// always serializes, so there is no failure to price.
pub fn string_body_units(text: &str) -> usize {
    string_size(text, true) - 2
}

#[cfg(test)]
mod tests;
