//! Immutable parser coordinates and bounded correspondence to admitted source bytes.

mod binding;
mod catalog;
pub use catalog::SourceCatalog;
mod budget;
mod decode;
mod descriptors;
mod model;
mod projection;
mod templates;
mod validation;

pub use binding::BoundSource;
pub use budget::{MapBudget, MapLimits, MapUsage, SourceError};
pub use decode::AdmittedSource;
pub use model::*;
pub use templates::{
    MapBranch, MapContribution, MapSegment, RepresentationContent, SourceTemplates,
};

#[cfg(test)]
mod tests;
