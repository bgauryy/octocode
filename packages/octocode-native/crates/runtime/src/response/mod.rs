//! Public response shaping shared by both native interfaces: contract-checked
//! rows, compression, pagination, continuations, and sanitized rendering.
//! Execution finishes in [`crate::runtime`]; `stage` turns executed rows
//! into the envelope a caller receives.

pub(crate) mod channels;
pub(crate) mod continuations;
pub mod exit;
pub(crate) mod extras;
#[cfg(test)]
mod hint_budget;
pub(crate) mod notices;
pub(crate) mod numbered;
pub mod outcome;
pub(crate) mod output;
pub(crate) mod page_windows;
pub mod pager;
pub(crate) mod pages;
pub(crate) mod paging;
pub(crate) mod read_share;
pub(crate) mod render;
pub(crate) mod result;
mod row_pages;
pub mod rows;
pub(crate) mod selection_errors;
pub(crate) mod stage;
pub(crate) mod stream_page;
pub(crate) mod symbol_outline;
