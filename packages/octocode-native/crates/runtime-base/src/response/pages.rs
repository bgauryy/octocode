//! Shared pagination facts and explicit remaining-evidence disclosures.
mod batch;
mod facts;
mod identity;
mod publish;
mod remaining;
pub(crate) use batch::state_once_per_batch;
pub use facts::{PageFacts, SpanFacts};
pub(crate) use identity::state_shared_identity;
pub use identity::state_shared_identity_on;
pub use publish::drop_echoes;
pub(crate) use publish::{publish_rows, state_out_of_range};
pub use remaining::{counted, first_named};
