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

/// A serializer whose output is the byte count of the compact JSON that
/// `serde_json::to_vec` writes for the same value.
struct Size(usize);

/// Bytes of `text` as a JSON string, quotes and escapes included.
fn string_bytes(text: &str) -> usize {
    2 + text
        .as_bytes()
        .iter()
        .map(|&byte| match byte {
            b'"' | b'\\' | b'\n' | b'\r' | b'\t' | 0x08 | 0x0c => 2,
            0..0x20 => 6,
            _ => 1,
        })
        .sum::<usize>()
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
            self.size.0 += 1;
        }
    }

    fn element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), SizeError> {
        self.comma();
        value.serialize(&mut *self.size)
    }

    fn field<T: ?Sized + Serialize>(&mut self, key: &str, value: &T) -> Result<(), SizeError> {
        self.comma();
        self.size.0 += string_bytes(key) + 1;
        value.serialize(&mut *self.size)
    }

    fn close(self) -> Result<(), SizeError> {
        self.size.0 += self.close;
        Ok(())
    }
}

macro_rules! signed {
    ($($method:ident: $ty:ty),*) => {$(
        fn $method(self, value: $ty) -> Result<(), SizeError> {
            self.0 += digits(u128::from(value.unsigned_abs())) + usize::from(value < 0);
            Ok(())
        }
    )*};
}

macro_rules! unsigned {
    ($($method:ident: $ty:ty),*) => {$(
        fn $method(self, value: $ty) -> Result<(), SizeError> {
            self.0 += digits(u128::from(value));
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
        self.0 += digits(value.unsigned_abs()) + usize::from(value < 0);
        Ok(())
    }

    fn serialize_u128(self, value: u128) -> Result<(), SizeError> {
        self.0 += digits(value);
        Ok(())
    }

    fn serialize_bool(self, value: bool) -> Result<(), SizeError> {
        self.0 += if value { 4 } else { 5 };
        Ok(())
    }

    // Floats are formatted by `serde_json` (shortest round trip, `null` for a
    // non-finite one); asking it keeps the size exact.
    fn serialize_f32(self, value: f32) -> Result<(), SizeError> {
        self.0 += count(&value, false).ok_or(SizeError::Failed)?;
        Ok(())
    }

    fn serialize_f64(self, value: f64) -> Result<(), SizeError> {
        self.0 += count(&value, false).ok_or(SizeError::Failed)?;
        Ok(())
    }

    fn serialize_char(self, value: char) -> Result<(), SizeError> {
        self.serialize_str(value.encode_utf8(&mut [0; 4]))
    }

    fn serialize_str(self, value: &str) -> Result<(), SizeError> {
        self.0 += string_bytes(value);
        Ok(())
    }

    fn serialize_bytes(self, value: &[u8]) -> Result<(), SizeError> {
        // An array of numbers, as `serde_json` writes bytes.
        self.0 += 2 + value.len().saturating_sub(1);
        self.0 += value
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
        self.0 += 4;
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
        self.0 += 1 + string_bytes(variant) + 1;
        value.serialize(&mut *self)?;
        self.0 += 1;
        Ok(())
    }

    fn serialize_seq(self, _: Option<usize>) -> Result<Body<'a>, SizeError> {
        self.0 += 1;
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
        self.0 += 1 + string_bytes(variant) + 1 + 1;
        Ok(Body {
            size: self,
            first: true,
            close: 2,
        })
    }

    fn serialize_map(self, _: Option<usize>) -> Result<Body<'a>, SizeError> {
        self.0 += 1;
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
        self.0 += 1 + string_bytes(variant) + 1 + 1;
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
        self.size.0 += key.serialize(StringKey)? + 1;
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
struct StringKey;

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
        Ok(string_bytes(value))
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
        Ok(string_bytes(&value.to_string()))
    }
}

/// UTF-16 units of UTF-8 `bytes`: one per scalar start byte, one more for
/// each four-byte scalar (a surrogate pair). Exact for any chunk that splits
/// at scalar boundaries or inside one: continuation bytes count nothing.
pub fn utf16_units_of_utf8(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .map(|&byte| usize::from(byte & 0xc0 != 0x80) + usize::from(byte >= 0xf0))
        .sum()
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

/// JSON bytes, or `None` if serialization failed; a partial document has no size.
///
/// Sized by a serializer that adds up what `serde_json` would write without
/// writing it: no escape buffers, no per-token `write_all`. A value it cannot
/// size exactly (a map key that is not a string) is rendered the old way.
pub fn bytes(value: &(impl Serialize + ?Sized)) -> Option<usize> {
    let mut size = Size(0);
    match value.serialize(&mut size) {
        Ok(()) => Some(size.0),
        Err(SizeError::Fallback) => count(value, false),
        Err(SizeError::Failed) => None,
    }
}

/// JSON UTF-16 units, including escaping, or `None` if serialization failed.
pub fn utf16_units(value: &(impl Serialize + ?Sized)) -> Option<usize> {
    count(value, true)
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
    utf16_units_or(text, 2).saturating_sub(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_json_weight_counts_payload_and_structural_overhead() {
        let text = "λ東京😀".repeat(128);
        let string = serde_json::json!(text);
        assert!(heap_bytes(&string) >= text.len());
        let array = serde_json::json!([null, null, string]);
        assert!(heap_bytes(&array) >= 3 * size_of::<serde_json::Value>() + text.len());
        let object = serde_json::json!({"small": 1, "nested": array});
        assert!(heap_bytes(&object) >= 2 * OBJECT_ENTRY_BYTES + heap_bytes(&array));
        let many_fields: serde_json::Value = serde_json::Value::Object(
            (0..128)
                .map(|index| (format!("k{index}"), serde_json::json!(0)))
                .collect(),
        );
        assert!(heap_bytes(&many_fields) > bytes(&many_fields).expect("wire bytes"));
    }

    #[test]
    fn json_size_matches_rendered_bytes_and_utf16() {
        for value in [
            serde_json::json!(null),
            serde_json::json!("quote \" tab \t é 中文 😀"),
            serde_json::json!({"nested": [1, true, "\n"]}),
            serde_json::json!({"message": "héllo \"quoted\"", "items": [1, 2.5, null, {"k": "v"}]}),
        ] {
            let rendered = serde_json::to_string(&value).unwrap();
            assert_eq!(bytes(&value), Some(rendered.len()));
            assert_eq!(utf16_units(&value), Some(rendered.encode_utf16().count()));
        }
        let invalid = std::collections::BTreeMap::from([((1, 2), "value")]);
        assert_eq!(bytes(&invalid), None);
        assert_eq!(utf16_units(&invalid), None);
        assert_eq!(bytes_or(&invalid, usize::MAX), usize::MAX);
        assert_eq!(utf16_units_or(&invalid, 7), 7);
        assert_eq!(bytes_or(&value_of("a"), usize::MAX), 3);
    }

    #[derive(serde::Serialize)]
    enum Shape {
        Unit,
        Newtype(String),
        Tuple(u8, Option<f64>),
        Struct { id: i64, tags: Vec<&'static str> },
    }

    #[derive(serde::Serialize)]
    struct Zoo {
        name: String,
        shapes: Vec<Shape>,
        #[serde(skip_serializing_if = "Option::is_none")]
        skipped: Option<u8>,
        wide: (i128, u128, i8, u16, f32, f64, char, bool, ()),
        bytes: serde_bytes_like::Bytes,
        by_name: std::collections::BTreeMap<String, Vec<u32>>,
        by_number: std::collections::BTreeMap<u32, bool>,
        #[serde(flatten)]
        rest: std::collections::BTreeMap<&'static str, i32>,
    }

    mod serde_bytes_like {
        pub struct Bytes(pub Vec<u8>);
        impl serde::Serialize for Bytes {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_bytes(&self.0)
            }
        }
    }

    /// The size serializer adds up exactly what `serde_json::to_vec` writes,
    /// over derived structs and enums of every shape, numbers at their edges,
    /// escapes, empty and nested containers, and non-string map keys.
    #[test]
    fn size_serializer_equals_the_rendered_length() {
        let zoo = Zoo {
            name: "q\" \\ \u{0} \u{1f} \u{7f} \n \r \t \u{8} \u{c} é 中 😀".to_owned(),
            shapes: vec![
                Shape::Unit,
                Shape::Newtype(String::new()),
                Shape::Tuple(255, Some(1.5e300)),
                Shape::Tuple(0, None),
                Shape::Struct {
                    id: i64::MIN,
                    tags: vec![],
                },
                Shape::Struct {
                    id: -1,
                    tags: vec!["a", "b\"c"],
                },
            ],
            skipped: None,
            wide: (
                i128::MIN,
                u128::MAX,
                -128,
                65535,
                f32::NAN,
                f64::INFINITY,
                '\u{1}',
                false,
                (),
            ),
            bytes: serde_bytes_like::Bytes(vec![0, 9, 10, 99, 100, 255]),
            by_name: [
                ("k\"".to_owned(), vec![1, 10, 100]),
                (String::new(), vec![]),
            ]
            .into(),
            by_number: [(0, true), (u32::MAX, false)].into(),
            rest: [("x", -10), ("y", 0)].into(),
        };
        assert_eq!(bytes(&zoo), Some(serde_json::to_vec(&zoo).unwrap().len()));
        let with = Zoo {
            skipped: Some(7),
            ..zoo_again()
        };
        assert_eq!(bytes(&with), Some(serde_json::to_vec(&with).unwrap().len()));
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..2000 {
            let value = random_value(&mut next, 4);
            assert_eq!(
                bytes(&value),
                Some(serde_json::to_vec(&value).unwrap().len()),
                "{value}"
            );
        }
    }

    fn zoo_again() -> Zoo {
        Zoo {
            name: String::new(),
            shapes: vec![],
            skipped: None,
            wide: (0, 0, 0, 0, 0.0, 0.0, 'a', true, ()),
            bytes: serde_bytes_like::Bytes(vec![]),
            by_name: Default::default(),
            by_number: Default::default(),
            rest: Default::default(),
        }
    }

    fn random_value(next: &mut impl FnMut() -> u64, depth: u32) -> serde_json::Value {
        use serde_json::Value;
        match next() % if depth == 0 { 6 } else { 8 } {
            0 => Value::Null,
            1 => Value::Bool(next().is_multiple_of(2)),
            2 => Value::from(next() as i64 >> (next() % 64)),
            3 => Value::from(next() >> (next() % 64)),
            4 => Value::from((next() as f64) / 7.0 - 1e9),
            5 => Value::String(
                (0..next() % 12)
                    .map(|_| {
                        ['a', '"', '\\', '\n', '\u{2}', 'é', '😀', '中', ' '][(next() % 9) as usize]
                    })
                    .collect(),
            ),
            6 => Value::Array(
                (0..next() % 4)
                    .map(|_| random_value(next, depth - 1))
                    .collect(),
            ),
            _ => Value::Object(
                (0..next() % 4)
                    .map(|index| {
                        (
                            format!("k{index}\"{}", next() % 3),
                            random_value(next, depth - 1),
                        )
                    })
                    .collect(),
            ),
        }
    }

    fn value_of(text: &str) -> serde_json::Value {
        serde_json::json!(text)
    }

    #[test]
    fn a_string_body_counts_escapes_and_utf16_units_as_rendered() {
        assert_eq!(string_body_units("a\"b\u{1F600}"), 6);
        assert_eq!(string_body_units(""), 0);
        assert_eq!(string_body_units("tab\t"), 5);
    }
}
