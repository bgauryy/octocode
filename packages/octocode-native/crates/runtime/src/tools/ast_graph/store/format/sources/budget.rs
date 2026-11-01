use super::*;
use crate::execution::cancel::CancellationCheck;
use std::time::Instant;

pub(super) fn invalid(message: &'static str) -> SourceError {
    SourceError::InvalidMap(message)
}

/// Cumulative allocation/work admission for this section, including scratch records.
pub(super) struct Budget<'a> {
    pub cancel: &'a dyn CancellationCheck,
    pub bytes: usize,
    pub items: usize,
    max_bytes: usize,
    max_items: usize,
    deadline: Instant,
    /// Checks made: the request is polled on every [`POLL`]th.
    ticks: std::cell::Cell<u32>,
}

/// Source steps between polls of the request's cancellation and deadline.
/// A step is a few instructions, while a poll can take the request's lock:
/// a stop is still seen within microseconds. Tests poll every step, so
/// their poll counts name exact stopping points.
#[cfg(not(test))]
pub(super) const POLL: u32 = 256;
#[cfg(test)]
pub(super) const POLL: u32 = 1;
impl<'a> Budget<'a> {
    /// The source table's charge within `limits` (its bytes and items),
    /// for one pass that ends with its request.
    pub fn new(cancel: &'a dyn CancellationCheck, limits: &Limits) -> Self {
        Self {
            cancel,
            bytes: 0,
            items: 0,
            max_bytes: limits.source_bytes,
            max_items: limits.table_items,
            deadline: crate::execution::cancel::deadline_of(cancel),
            ticks: std::cell::Cell::new(0),
        }
    }
    pub fn check(&self) -> Result<(), SourceError> {
        let tick = self.ticks.get();
        self.ticks.set(tick.wrapping_add(1));
        if !tick.is_multiple_of(POLL) {
            return Ok(());
        }
        self.cancel.check().map_err(|_| SourceError::Cancelled)?;
        if Instant::now() >= self.deadline {
            return Err(SourceError::Deadline);
        }
        Ok(())
    }
    pub(super) fn charge(&mut self, bytes: usize, items: usize) -> Result<(), SourceError> {
        self.check()?;
        let total_bytes = self.bytes.checked_add(bytes);
        let total_items = self.items.checked_add(items);
        match (total_bytes, total_items) {
            (Some(bytes), Some(items)) if bytes <= self.max_bytes && items <= self.max_items => {
                self.bytes = bytes;
                self.items = items;
                Ok(())
            }
            _ => Err(SourceError::LimitExceeded(
                "graph source allocation/work budget",
            )),
        }
    }
    pub fn vector_cost<T>(&mut self, count: usize) -> Result<(), SourceError> {
        self.charge(
            count
                .checked_mul(std::mem::size_of::<T>())
                .ok_or_else(|| invalid("source allocation overflow"))?,
            count,
        )
    }
    pub fn vector<T>(&mut self, count: usize) -> Result<Vec<T>, SourceError> {
        self.vector_cost::<T>(count)?;
        let mut out = Vec::new();
        out.try_reserve_exact(count)
            .map_err(|_| SourceError::LimitExceeded("graph source allocation"))?;
        self.charge(
            out.capacity()
                .saturating_sub(count)
                .checked_mul(std::mem::size_of::<T>())
                .ok_or_else(|| invalid("source allocation overflow"))?,
            0,
        )?;
        Ok(out)
    }
    pub fn grow<T>(&mut self, out: &mut Vec<T>, extra: usize) -> Result<(), SourceError> {
        self.check()?;
        let needed = out
            .len()
            .checked_add(extra)
            .ok_or_else(|| invalid("source allocation overflow"))?;
        if needed > out.capacity() {
            let capacity = needed
                .checked_next_power_of_two()
                .ok_or_else(|| invalid("source allocation overflow"))?;
            let more = capacity - out.capacity();
            self.charge(
                more.checked_mul(std::mem::size_of::<T>())
                    .ok_or_else(|| invalid("source allocation overflow"))?,
                0,
            )?;
            out.try_reserve_exact(capacity - out.len())
                .map_err(|_| SourceError::LimitExceeded("graph source allocation"))?;
            self.charge(
                out.capacity()
                    .saturating_sub(capacity)
                    .checked_mul(std::mem::size_of::<T>())
                    .ok_or_else(|| invalid("source allocation overflow"))?,
                0,
            )?;
        }
        Ok(())
    }
    pub fn string(&mut self, value: &str) -> Result<String, SourceError> {
        self.charge(value.len(), 1)?;
        let mut out = String::new();
        out.try_reserve_exact(value.len())
            .map_err(|_| SourceError::LimitExceeded("graph source string allocation"))?;
        self.charge(out.capacity().saturating_sub(value.len()), 0)?;
        out.push_str(value);
        Ok(out)
    }
    pub fn copy(&mut self, value: &[u8]) -> Result<Vec<u8>, SourceError> {
        self.charge(value.len(), 0)?;
        let mut out = Vec::new();
        out.try_reserve_exact(value.len())
            .map_err(|_| SourceError::LimitExceeded("graph source byte allocation"))?;
        self.charge(out.capacity().saturating_sub(value.len()), 0)?;
        out.extend_from_slice(value);
        Ok(out)
    }
    pub fn put(&mut self, out: &mut Vec<u8>, value: usize) -> Result<(), SourceError> {
        let value = u32::try_from(value).map_err(|_| invalid("source field overflows u32"))?;
        self.grow(out, 5)?;
        writer::put(out, u64::from(value));
        Ok(())
    }
    pub fn text(&mut self, out: &mut Vec<u8>, value: &str) -> Result<(), SourceError> {
        self.put(out, value.len())?;
        self.grow(out, value.len())?;
        out.extend_from_slice(value.as_bytes());
        Ok(())
    }
}

pub(super) struct Input<'a, 'b, 'c> {
    pub reader: Reader<'a>,
    pub budget: &'b mut Budget<'c>,
}
impl<'a, 'b, 'c> Input<'a, 'b, 'c> {
    pub fn new(bytes: &'a [u8], budget: &'b mut Budget<'c>) -> Self {
        Self {
            reader: Reader::new(bytes, "SRCE"),
            budget,
        }
    }
    pub fn number(&mut self) -> Result<u32, SourceError> {
        self.budget.check()?;
        self.reader
            .var_u32()
            .map_err(|_| invalid("invalid source varint"))
    }
    pub fn count(&mut self) -> Result<usize, SourceError> {
        self.budget.check()?;
        self.reader
            .count()
            .map_err(|_| invalid("invalid source record count"))
    }
    pub fn tag(&mut self, count: u32) -> Result<u32, SourceError> {
        let value = self.number()?;
        if value >= count {
            return Err(invalid("unknown source enum tag"));
        }
        Ok(value)
    }
    pub fn range(&mut self) -> Result<ByteRange, SourceError> {
        let range = ByteRange::new(self.number()?, self.number()?);
        if range.start_byte > range.end_byte {
            return Err(invalid("reversed source range"));
        }
        Ok(range)
    }
    pub fn text(&mut self) -> Result<String, SourceError> {
        let len = self.number()? as usize;
        let value = self
            .reader
            .take(len)
            .map_err(|_| invalid("truncated source string"))?;
        let value =
            std::str::from_utf8(value).map_err(|_| invalid("source string is not UTF-8"))?;
        self.budget.string(value)
    }
    pub fn finish(&self) -> Result<(), SourceError> {
        if self.reader.at != self.reader.bytes.len() {
            return Err(invalid("trailing source section bytes"));
        }
        Ok(())
    }
}
