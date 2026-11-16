//! The artifact's record in a compact binary form of its serde data model.
//!
//! A file's graph facts are JSON many times the file's size, and most of those
//! bytes are repeats: the same field names in every record, and the same long
//! ids (a declaration's id is written again by each call, edge and argument
//! that names it). Here a string of [`INTERN_MIN`] bytes or more is written
//! once and referred to by its index at every later use, field names
//! included, and numbers are varints; there is no escaping and no number
//! parsing to do on the way back. The form is self-describing (the same tags
//! a JSON value has), so every serde derive in the facts, tagged enums and
//! skipped fields included, round-trips without a second schema.
//!
//! The reader borrows strings from the input, so a repeated string costs one
//! copy where the value is built, none to look up.
//!
//! The record's size as JSON is also what its admission and retention charge
//! (what the facts cost decoded). [`pack`] counts it as it writes: exactly
//! `serde_json::to_vec(value).len()`, which a test holds it to.
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "each cast is masked to its target (a varint's seven bits, a table's power-of-two size), bounded by a u32 index the encoder refuses to pass, or a negative's magnitude"
)]
use crate::tools::ast_graph::hashing::hash_bytes;
use serde::de::{self, DeserializeSeed, IgnoredAny, Visitor};
use serde::ser::{self, Serialize};
use std::fmt;

const NULL: u8 = 0;
const FALSE: u8 = 1;
const TRUE: u8 = 2;
const UINT: u8 = 3;
const NEGATIVE: u8 = 4;
const FLOAT: u8 = 5;
/// A string, first use: `len` and its bytes; later uses refer to its index.
const NEW: u8 = 6;
const REF: u8 = 7;
/// A string too short to be worth an index.
const SMALL: u8 = 8;
const SEQ: u8 = 9;
const MAP: u8 = 10;
const END: u8 = 11;
/// `INLINE | n` is the unsigned integer `n < 128`.
const INLINE: u8 = 0x80;

/// A string shorter than this is written in place: a reference is two or
/// three bytes, and an index entry costs a hash and a probe on each side.
const INTERN_MIN: usize = 4;
/// Containers nested deeper than this are no record this build wrote.
const MAX_DEPTH: usize = 128;

#[derive(Debug)]
pub(super) struct Error(String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl ser::Error for Error {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Self(message.to_string())
    }
}

impl de::Error for Error {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Self(message.to_string())
    }
}

fn error<T>(message: &str) -> Result<T, Error> {
    Err(Error(message.to_owned()))
}

/// A value's compact bytes and its size as JSON.
pub(super) struct Packed {
    pub(super) bytes: Vec<u8>,
    pub(super) json_bytes: usize,
}

/// `value` in compact form, or an error once its JSON would pass `max_json`.
pub(super) fn pack<T: Serialize + ?Sized>(value: &T, max_json: usize) -> Result<Packed, Error> {
    let mut encoder = Encoder {
        out: Vec::with_capacity(4096),
        json: 0,
        max_json,
        index: Index::default(),
    };
    value.serialize(&mut encoder)?;
    encoder.check()?;
    Ok(Packed {
        bytes: encoder.out,
        json_bytes: encoder.json,
    })
}

/// The value `bytes` hold, which [`pack`] wrote.
pub(super) fn unpack<'de, T: de::Deserialize<'de>>(bytes: &'de [u8]) -> Result<T, Error> {
    let mut decoder = Decoder {
        input: bytes,
        at: 0,
        strings: Vec::new(),
        depth: 0,
    };
    let value = T::deserialize(&mut decoder)?;
    if decoder.at != bytes.len() {
        return error("trailing bytes after the record");
    }
    Ok(value)
}

pub(super) fn put_varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

/// The varint at the start of `bytes` and the bytes after it.
pub(super) fn take_varint(bytes: &[u8]) -> Option<(u64, &[u8])> {
    let mut value = 0_u64;
    for (at, byte) in bytes.iter().take(10).enumerate() {
        value |= u64::from(byte & 0x7f) << (7 * at);
        if byte & 0x80 == 0 {
            return Some((value, &bytes[at + 1..]));
        }
    }
    None
}

/// Digits of `n` in decimal.
fn digits(n: u64) -> usize {
    n.checked_ilog10().map_or(1, |log| log as usize + 1)
}

/// What each byte of a string costs as JSON.
const ESCAPED: [u8; 256] = {
    let mut table = [1_u8; 256];
    let mut byte = 0;
    while byte < 0x20 {
        table[byte] = 6;
        byte += 1;
    }
    table[b'"' as usize] = 2;
    table[b'\\' as usize] = 2;
    table[0x08] = 2;
    table[0x09] = 2;
    table[0x0a] = 2;
    table[0x0c] = 2;
    table[0x0d] = 2;
    table
};

/// `s` as a JSON string: its bytes escaped, and the quotes.
fn json_string(s: &str) -> usize {
    2 + s
        .bytes()
        .map(|b| usize::from(ESCAPED[usize::from(b)]))
        .sum::<usize>()
}

/// The strings written so far, found by their bytes in the output itself.
#[derive(Default)]
struct Index {
    /// Entry number + 1; 0 is empty.
    slots: Vec<u32>,
    /// Where each entry's bytes are in the output.
    entries: Vec<(u32, u32)>,
}

enum Seen {
    Known(u32),
    New,
}

impl Index {
    /// The index of `s`, or the slot to claim for it.
    fn find(&self, out: &[u8], s: &[u8], hash: u64) -> (Seen, usize) {
        if self.slots.is_empty() {
            return (Seen::New, 0);
        }
        let mask = self.slots.len() - 1;
        let mut slot = hash as usize & mask;
        loop {
            match self.slots[slot] {
                0 => return (Seen::New, slot),
                entry => {
                    let (at, len) = self.entries[entry as usize - 1];
                    if &out[at as usize..(at + len) as usize] == s {
                        return (Seen::Known(entry - 1), slot);
                    }
                }
            }
            slot = (slot + 1) & mask;
        }
    }

    fn grow(&mut self, out: &[u8]) {
        let capacity = (self.slots.len() * 2).max(256);
        self.slots = vec![0; capacity];
        let mask = capacity - 1;
        for (number, &(at, len)) in self.entries.iter().enumerate() {
            let mut slot = hash_bytes(&out[at as usize..(at + len) as usize]) as usize & mask;
            while self.slots[slot] != 0 {
                slot = (slot + 1) & mask;
            }
            self.slots[slot] = number as u32 + 1;
        }
    }
}

struct Encoder {
    out: Vec<u8>,
    json: usize,
    max_json: usize,
    index: Index,
}

impl Encoder {
    fn check(&self) -> Result<(), Error> {
        if self.json > self.max_json {
            return error("graph extraction artifact exceeds its byte limit");
        }
        Ok(())
    }

    fn string(&mut self, s: &str) -> Result<(), Error> {
        self.json += json_string(s);
        self.check()?;
        if s.len() < INTERN_MIN {
            self.out.push(SMALL);
            put_varint(&mut self.out, s.len() as u64);
            self.out.extend_from_slice(s.as_bytes());
            return Ok(());
        }
        let hash = hash_bytes(s.as_bytes());
        let (seen, slot) = self.index.find(&self.out, s.as_bytes(), hash);
        match seen {
            Seen::Known(entry) => {
                self.out.push(REF);
                put_varint(&mut self.out, u64::from(entry));
            }
            Seen::New => {
                self.out.push(NEW);
                put_varint(&mut self.out, s.len() as u64);
                let at = self.out.len();
                self.out.extend_from_slice(s.as_bytes());
                let (Ok(at), Ok(len)) = (u32::try_from(at), u32::try_from(s.len())) else {
                    return error("graph extraction artifact is too large to index");
                };
                self.index.entries.push((at, len));
                // Past half full the table doubles; the new entry is placed by it.
                if self.index.entries.len() * 2 > self.index.slots.len() {
                    self.index.grow(&self.out);
                } else {
                    self.index.slots[slot] = self.index.entries.len() as u32;
                }
            }
        }
        Ok(())
    }

    fn unsigned(&mut self, n: u64) -> Result<(), Error> {
        self.json += digits(n);
        self.check()?;
        if n < 128 {
            self.out.push(INLINE | n as u8);
        } else {
            self.out.push(UINT);
            put_varint(&mut self.out, n);
        }
        Ok(())
    }

    fn signed(&mut self, n: i64) -> Result<(), Error> {
        if n >= 0 {
            return self.unsigned(n as u64);
        }
        let magnitude = n.unsigned_abs();
        self.json += 1 + digits(magnitude);
        self.check()?;
        self.out.push(NEGATIVE);
        put_varint(&mut self.out, magnitude - 1);
        Ok(())
    }

    fn float(&mut self, json: Result<String, serde_json::Error>, n: f64) -> Result<(), Error> {
        // A float is as JSON what serde_json writes: `null` for one it cannot.
        self.json += json.map_or(4, |text| text.len());
        self.check()?;
        self.out.push(FLOAT);
        self.out.extend_from_slice(&n.to_le_bytes());
        Ok(())
    }

    fn tag(&mut self, tag: u8, json: usize) -> Result<(), Error> {
        self.json += json;
        self.check()?;
        self.out.push(tag);
        Ok(())
    }
}

/// A sequence, map or struct being written; `variant` when it is the value of
/// a one-entry map that names a variant, which its end closes too.
struct Compound<'a> {
    encoder: &'a mut Encoder,
    items: usize,
    variant: bool,
}

impl Compound<'_> {
    fn finish(self) -> Result<(), Error> {
        // The brackets or braces, and a variant's own braces and colon.
        self.encoder.json += 2 + if self.variant { 3 } else { 0 };
        self.encoder.out.push(END);
        if self.variant {
            self.encoder.out.push(END);
        }
        self.encoder.check()
    }

    fn separate(&mut self) {
        if self.items > 0 {
            self.encoder.json += 1;
        }
        self.items += 1;
    }

    fn element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        self.separate();
        value.serialize(&mut *self.encoder)
    }

    fn field<T: Serialize + ?Sized>(&mut self, key: &str, value: &T) -> Result<(), Error> {
        self.separate();
        // The colon after the key.
        self.encoder.json += 1;
        self.encoder.string(key)?;
        value.serialize(&mut *self.encoder)
    }
}

impl ser::SerializeSeq for Compound<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        self.element(value)
    }
    fn end(self) -> Result<(), Error> {
        self.finish()
    }
}

impl ser::SerializeTuple for Compound<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        self.element(value)
    }
    fn end(self) -> Result<(), Error> {
        self.finish()
    }
}

impl ser::SerializeTupleStruct for Compound<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        self.element(value)
    }
    fn end(self) -> Result<(), Error> {
        self.finish()
    }
}

impl ser::SerializeTupleVariant for Compound<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        self.element(value)
    }
    fn end(self) -> Result<(), Error> {
        self.finish()
    }
}

impl ser::SerializeStruct for Compound<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Error> {
        self.field(key, value)
    }
    fn end(self) -> Result<(), Error> {
        self.finish()
    }
}

impl ser::SerializeStructVariant for Compound<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Error> {
        self.field(key, value)
    }
    fn end(self) -> Result<(), Error> {
        self.finish()
    }
}

impl ser::SerializeMap for Compound<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), Error> {
        self.separate();
        self.encoder.json += 1;
        key.serialize(KeyEncoder(&mut *self.encoder))
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        value.serialize(&mut *self.encoder)
    }
    fn end(self) -> Result<(), Error> {
        self.finish()
    }
}

/// A map key: only a string. JSON writes a number key in quotes, which a
/// reader of the number's type refuses, so a record holding one is no artifact.
struct KeyEncoder<'a>(&'a mut Encoder);

macro_rules! key_refused {
    ($($method:ident($($arg:ty)?)),*) => {
        $(fn $method(self $(, _: $arg)?) -> Result<(), Error> { error("a map key must be a string") })*
    };
}

impl ser::Serializer for KeyEncoder<'_> {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = ser::Impossible<(), Error>;
    type SerializeTuple = ser::Impossible<(), Error>;
    type SerializeTupleStruct = ser::Impossible<(), Error>;
    type SerializeTupleVariant = ser::Impossible<(), Error>;
    type SerializeMap = ser::Impossible<(), Error>;
    type SerializeStruct = ser::Impossible<(), Error>;
    type SerializeStructVariant = ser::Impossible<(), Error>;

    fn serialize_str(self, v: &str) -> Result<(), Error> {
        self.0.string(v)
    }
    fn serialize_char(self, v: char) -> Result<(), Error> {
        self.0.string(v.encode_utf8(&mut [0; 4]))
    }
    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
    ) -> Result<(), Error> {
        self.0.string(variant)
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        value: &T,
    ) -> Result<(), Error> {
        value.serialize(self)
    }
    key_refused!(
        serialize_bool(bool),
        serialize_i8(i8),
        serialize_i16(i16),
        serialize_i32(i32),
        serialize_i64(i64),
        serialize_u8(u8),
        serialize_u16(u16),
        serialize_u32(u32),
        serialize_u64(u64),
        serialize_f32(f32),
        serialize_f64(f64),
        serialize_bytes(&[u8]),
        serialize_none(),
        serialize_unit(),
        serialize_unit_struct(&'static str)
    );
    fn serialize_some<T: Serialize + ?Sized>(self, _: &T) -> Result<(), Error> {
        error("a map key must be a string")
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: &T,
    ) -> Result<(), Error> {
        error("a map key must be a string")
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, Error> {
        error("a map key must be a string")
    }
    fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, Error> {
        error("a map key must be a string")
    }
    fn serialize_tuple_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleStruct, Error> {
        error("a map key must be a string")
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, Error> {
        error("a map key must be a string")
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, Error> {
        error("a map key must be a string")
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Self::SerializeStruct, Error> {
        error("a map key must be a string")
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, Error> {
        error("a map key must be a string")
    }
}

macro_rules! unsigned {
    ($($method:ident: $ty:ty),*) => {
        $(fn $method(self, v: $ty) -> Result<(), Error> { self.unsigned(u64::from(v)) })*
    };
}

macro_rules! signed {
    ($($method:ident: $ty:ty),*) => {
        $(fn $method(self, v: $ty) -> Result<(), Error> { self.signed(i64::from(v)) })*
    };
}

impl<'a> ser::Serializer for &'a mut Encoder {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = Compound<'a>;
    type SerializeTuple = Compound<'a>;
    type SerializeTupleStruct = Compound<'a>;
    type SerializeTupleVariant = Compound<'a>;
    type SerializeMap = Compound<'a>;
    type SerializeStruct = Compound<'a>;
    type SerializeStructVariant = Compound<'a>;

    unsigned!(serialize_u8: u8, serialize_u16: u16, serialize_u32: u32, serialize_u64: u64);
    signed!(serialize_i8: i8, serialize_i16: i16, serialize_i32: i32, serialize_i64: i64);

    fn serialize_bool(self, v: bool) -> Result<(), Error> {
        self.tag(if v { TRUE } else { FALSE }, if v { 4 } else { 5 })
    }
    fn serialize_f32(self, v: f32) -> Result<(), Error> {
        self.float(serde_json::to_string(&v), f64::from(v))
    }
    fn serialize_f64(self, v: f64) -> Result<(), Error> {
        self.float(serde_json::to_string(&v), v)
    }
    fn serialize_char(self, v: char) -> Result<(), Error> {
        self.string(v.encode_utf8(&mut [0; 4]))
    }
    fn serialize_str(self, v: &str) -> Result<(), Error> {
        self.string(v)
    }
    fn serialize_bytes(self, v: &[u8]) -> Result<(), Error> {
        use ser::SerializeSeq;
        let mut seq = self.serialize_seq(Some(v.len()))?;
        for byte in v {
            seq.serialize_element(byte)?;
        }
        seq.end()
    }
    fn serialize_none(self) -> Result<(), Error> {
        self.tag(NULL, 4)
    }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<(), Error> {
        value.serialize(self)
    }
    fn serialize_unit(self) -> Result<(), Error> {
        self.tag(NULL, 4)
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<(), Error> {
        self.tag(NULL, 4)
    }
    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
    ) -> Result<(), Error> {
        self.string(variant)
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        value: &T,
    ) -> Result<(), Error> {
        value.serialize(self)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<(), Error> {
        // `{"variant":value}`: the braces and the colon.
        self.json += 3;
        self.out.push(MAP);
        self.string(variant)?;
        value.serialize(&mut *self)?;
        self.out.push(END);
        Ok(())
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Compound<'a>, Error> {
        self.out.push(SEQ);
        Ok(Compound {
            encoder: self,
            items: 0,
            variant: false,
        })
    }
    fn serialize_tuple(self, len: usize) -> Result<Compound<'a>, Error> {
        self.serialize_seq(Some(len))
    }
    fn serialize_tuple_struct(self, _: &'static str, len: usize) -> Result<Compound<'a>, Error> {
        self.serialize_seq(Some(len))
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Compound<'a>, Error> {
        self.out.push(MAP);
        self.string(variant)?;
        self.out.push(SEQ);
        Ok(Compound {
            encoder: self,
            items: 0,
            variant: true,
        })
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Compound<'a>, Error> {
        self.out.push(MAP);
        Ok(Compound {
            encoder: self,
            items: 0,
            variant: false,
        })
    }
    fn serialize_struct(self, _: &'static str, len: usize) -> Result<Compound<'a>, Error> {
        self.serialize_map(Some(len))
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Compound<'a>, Error> {
        self.out.push(MAP);
        self.string(variant)?;
        self.out.push(MAP);
        Ok(Compound {
            encoder: self,
            items: 0,
            variant: true,
        })
    }
}

struct Decoder<'de> {
    input: &'de [u8],
    at: usize,
    /// The strings read so far, in the order they were first written.
    strings: Vec<&'de str>,
    depth: usize,
}

impl<'de> Decoder<'de> {
    fn byte(&mut self) -> Result<u8, Error> {
        let byte = *self
            .input
            .get(self.at)
            .ok_or_else(|| Error("record ends inside a value".into()))?;
        self.at += 1;
        Ok(byte)
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.at).copied()
    }

    fn varint(&mut self) -> Result<u64, Error> {
        let Some((value, rest)) = take_varint(&self.input[self.at..]) else {
            return error("record has a malformed number");
        };
        self.at = self.input.len() - rest.len();
        Ok(value)
    }

    fn take(&mut self) -> Result<&'de str, Error> {
        let len = usize::try_from(self.varint()?).map_err(|_| Error("string too long".into()))?;
        let end = self
            .at
            .checked_add(len)
            .filter(|end| *end <= self.input.len())
            .ok_or_else(|| Error("record ends inside a string".into()))?;
        let text = std::str::from_utf8(&self.input[self.at..end])
            .map_err(|_| Error("record holds a string that is not UTF-8".into()))?;
        self.at = end;
        Ok(text)
    }

    fn close(&mut self) -> Result<(), Error> {
        if self.byte()? != END {
            return error("record has more items than its type holds");
        }
        Ok(())
    }

    fn nest<T>(&mut self, read: impl FnOnce(&mut Self) -> Result<T, Error>) -> Result<T, Error> {
        if self.depth >= MAX_DEPTH {
            return error("record nests too deep");
        }
        self.depth += 1;
        let value = read(self);
        self.depth -= 1;
        value
    }
}

struct Items<'a, 'de> {
    decoder: &'a mut Decoder<'de>,
}

impl<'de> de::SeqAccess<'de> for Items<'_, 'de> {
    type Error = Error;
    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Error> {
        if self.decoder.peek() == Some(END) {
            return Ok(None);
        }
        seed.deserialize(&mut *self.decoder).map(Some)
    }
}

impl<'de> de::MapAccess<'de> for Items<'_, 'de> {
    type Error = Error;
    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Error> {
        if self.decoder.peek() == Some(END) {
            return Ok(None);
        }
        seed.deserialize(&mut *self.decoder).map(Some)
    }
    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, Error> {
        seed.deserialize(&mut *self.decoder)
    }
}

/// An enum written as a one-entry map: the variant's name, then its value.
struct Variant<'a, 'de> {
    decoder: &'a mut Decoder<'de>,
}

impl<'a, 'de> de::EnumAccess<'de> for Variant<'a, 'de> {
    type Error = Error;
    type Variant = Self;
    fn variant_seed<V: DeserializeSeed<'de>>(self, seed: V) -> Result<(V::Value, Self), Error> {
        let name = seed.deserialize(&mut *self.decoder)?;
        Ok((name, self))
    }
}

/// The value after a variant's name; the entry's end is closed by the caller.
impl<'de> de::VariantAccess<'de> for Variant<'_, 'de> {
    type Error = Error;
    fn unit_variant(self) -> Result<(), Error> {
        <IgnoredAny as de::Deserialize>::deserialize(&mut *self.decoder).map(|_| ())
    }
    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value, Error> {
        seed.deserialize(&mut *self.decoder)
    }
    fn tuple_variant<V: Visitor<'de>>(self, _: usize, visitor: V) -> Result<V::Value, Error> {
        de::Deserializer::deserialize_any(&mut *self.decoder, visitor)
    }
    fn struct_variant<V: Visitor<'de>>(
        self,
        _: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        de::Deserializer::deserialize_any(&mut *self.decoder, visitor)
    }
}

impl<'de> de::Deserializer<'de> for &mut Decoder<'de> {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        let tag = self.byte()?;
        if tag >= INLINE {
            return visitor.visit_u64(u64::from(tag & !INLINE));
        }
        match tag {
            NULL => visitor.visit_unit(),
            FALSE => visitor.visit_bool(false),
            TRUE => visitor.visit_bool(true),
            UINT => visitor.visit_u64(self.varint()?),
            NEGATIVE => {
                let magnitude = i64::try_from(self.varint()?)
                    .map_err(|_| Error("negative number out of range".into()))?;
                visitor.visit_i64(-1 - magnitude)
            }
            FLOAT => {
                let end = self.at + 8;
                let bytes = self
                    .input
                    .get(self.at..end)
                    .ok_or_else(|| Error("record ends inside a number".into()))?;
                self.at = end;
                visitor.visit_f64(f64::from_le_bytes(bytes.try_into().unwrap_or([0; 8])))
            }
            NEW => {
                let text = self.take()?;
                self.strings.push(text);
                visitor.visit_borrowed_str(text)
            }
            REF => {
                let at = usize::try_from(self.varint()?)
                    .map_err(|_| Error("string index too large".into()))?;
                let text = *self
                    .strings
                    .get(at)
                    .ok_or_else(|| Error("record refers to a string it has not written".into()))?;
                visitor.visit_borrowed_str(text)
            }
            SMALL => visitor.visit_borrowed_str(self.take()?),
            SEQ => self.nest(|this| {
                let value = visitor.visit_seq(Items {
                    decoder: &mut *this,
                })?;
                this.close()?;
                Ok(value)
            }),
            MAP => self.nest(|this| {
                let value = visitor.visit_map(Items {
                    decoder: &mut *this,
                })?;
                this.close()?;
                Ok(value)
            }),
            _ => error("record holds an unknown tag"),
        }
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        if self.peek() == Some(NULL) {
            self.at += 1;
            return visitor.visit_none();
        }
        visitor.visit_some(self)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _: &'static str,
        _: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        match self.peek() {
            // A unit variant is its name.
            Some(NEW | REF | SMALL) => {
                struct Name<'a, 'de>(&'a mut Option<&'de str>);
                impl<'de> Visitor<'de> for Name<'_, 'de> {
                    type Value = ();
                    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        f.write_str("a string")
                    }
                    fn visit_borrowed_str<E: de::Error>(self, v: &'de str) -> Result<(), E> {
                        *self.0 = Some(v);
                        Ok(())
                    }
                }
                let mut name = None;
                self.deserialize_any(Name(&mut name))?;
                let Some(name) = name else {
                    return error("record holds a variant with no name");
                };
                visitor.visit_enum(de::value::BorrowedStrDeserializer::<Error>::new(name))
            }
            Some(MAP) => {
                self.at += 1;
                self.nest(|this| {
                    let value = visitor.visit_enum(Variant {
                        decoder: &mut *this,
                    })?;
                    this.close()?;
                    Ok(value)
                })
            }
            _ => error("record holds no variant here"),
        }
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf unit unit_struct seq tuple tuple_struct map struct
        identifier ignored_any
    }
}
