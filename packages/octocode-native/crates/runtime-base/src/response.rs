//! Public response shaping shared by both native interfaces: contract-checked
//! rows, compression, pagination and continuations, all over one JSON
//! envelope.
//! Execution finishes in `octocode_native::runtime`; `stage` turns executed rows
//! into the envelope a caller receives.

pub(crate) mod answered_leads;
pub mod collection;
pub mod continuations;
pub mod cursor;
pub mod cut;
pub mod exit;
pub mod key_order;
pub mod notices;
pub mod numbered;
pub mod outcome;
pub mod output;
pub mod page_model;
pub(crate) mod page_receipts;
pub mod page_windows;
pub mod pager;
pub mod pages;
pub mod paging;
pub mod paths;
pub mod read_share;
pub mod result;
mod row_pages;
pub mod rows;
pub mod selection_errors;
pub mod source_evidence;
pub mod stage;
pub mod symbol_outline;
pub mod wire;
pub mod workflow_lead;
