//! Where a streamed source's redactions are, as its windows need to place them:
//! every edit of its sealed view, in order. Every byte held is charged to
//! [`super::memory`]; a budget with no room refuses the read, never a partial
//! view.

use super::error::ReadError;
use super::sealed::{Edit, Shift, keyed_offset, shift};
use octocode_io::memo::Reservation;
use std::ops::Range;
use std::sync::Arc;

type KeyLines = [(Range<usize>, usize)];

/// The redactions of one source's sealed view, in order.
#[derive(Clone)]
pub(super) struct EditMap(pub(super) Arc<[Edit]>);

/// Make room for `more` items in `vec`, held as `held` says: the new block is
/// held on top of the old one while it is copied, then the old one's share is
/// given back.
pub(super) fn make_room<T>(
    vec: &mut Vec<T>,
    more: usize,
    held: &mut Reservation<'static>,
) -> Result<(), ReadError> {
    let size = std::mem::size_of::<T>().max(1);
    let (len, capacity) = (vec.len(), vec.capacity());
    let needed = len.saturating_add(more);
    if needed > capacity {
        let new = capacity.saturating_mul(2).max(needed);
        if !held.grow(new.saturating_mul(size)) {
            return Err(ReadError::Refused);
        }
        vec.reserve_exact(new - len);
        held.shrink(capacity.saturating_mul(size));
    }
    Ok(())
}

impl EditMap {
    /// The edits that start in `span` and end in it.
    pub(super) fn shift(&self, span: Range<usize>) -> Shift {
        shift(&self.0, span)
    }

    /// [`keyed_offset`] of the source's edits and `key_lines`.
    pub(super) fn keyed(
        &self,
        key_lines: &KeyLines,
        range_start: usize,
        sealed: usize,
    ) -> Option<usize> {
        keyed_offset(&self.0, key_lines, range_start, sealed)
    }

    /// The edits that end after `span` starts, in order: the first of them are
    /// those inside it, and the edits past its end are the caller's to stop at.
    pub(super) fn over(&self, span: Range<usize>) -> &[Edit] {
        let first = self.0.partition_point(|edit| edit.range.end <= span.start);
        &self.0[first..]
    }
}
