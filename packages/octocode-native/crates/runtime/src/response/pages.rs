//! Shared pagination facts and explicit remaining-evidence disclosures.
mod facts;
mod publish;
mod remaining;
pub(crate) use facts::{PageFacts, SpanFacts};
pub(crate) use publish::{drop_echoes, publish_rows};
pub(crate) use remaining::{
    counted, first_named, items_left, more, note_remaining, redisclose, take_remaining_notes,
};
const WARNINGS: &str = "warnings";
