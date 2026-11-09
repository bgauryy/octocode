//! Shared pagination facts and explicit remaining-evidence disclosures.
mod facts;
mod identity;
mod publish;
mod remaining;
pub(crate) use facts::{PageFacts, SpanFacts};
pub(crate) use identity::{state_shared_identity, state_shared_identity_on};
pub(crate) use publish::{drop_echoes, publish_rows};
pub(crate) use remaining::{counted, first_named};
