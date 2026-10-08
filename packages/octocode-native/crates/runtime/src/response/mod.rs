//! Public response shaping shared by both native interfaces: contract-checked
//! rows, compression, pagination, continuations, and sanitized rendering.
//! Execution finishes in [`crate::runtime`]; `stage` turns executed rows
//! into the envelope a caller receives.

pub(crate) mod channels;
pub(crate) mod continuations;
#[cfg(test)]
mod hint_budget;
pub mod pager;
pub(crate) mod pages;
pub(crate) mod read_share;
pub(crate) mod render;
mod row_pages;
pub mod rows;
pub(crate) mod stage;
pub(crate) mod verbose;
