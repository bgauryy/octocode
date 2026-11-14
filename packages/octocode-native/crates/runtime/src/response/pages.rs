//! Shared pagination facts and explicit remaining-evidence disclosures.
mod batch;
mod facts;
mod identity;
mod publish;
mod remaining;
pub(crate) use batch::state_once_per_batch;
pub(crate) use facts::{PageFacts, SpanFacts};
pub(crate) use identity::{state_shared_identity, state_shared_identity_on};
pub(crate) use publish::{drop_echoes, publish_rows, state_out_of_range};
pub(crate) use remaining::{counted, first_named};
