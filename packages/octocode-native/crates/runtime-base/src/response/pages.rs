//! Shared pagination facts and explicit remaining-evidence disclosures.
mod facts;
mod publish;
pub use facts::{PageFacts, SpanFacts};
pub use octocode_engine::text::counted;
pub use publish::drop_echoes;
pub(crate) use publish::{publish_rows, state_out_of_range};
