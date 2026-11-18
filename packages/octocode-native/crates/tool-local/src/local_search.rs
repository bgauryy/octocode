mod binaries;
mod coverage;
pub mod cursor;
mod enclosing;
pub mod executor;
mod handoff;
pub mod key_scope;
mod layout;
mod leads;
mod local_search_query;
mod manifest;
pub mod output;
mod packed;
mod page_view;
mod pattern;
pub mod probes;
mod rows;
mod selection;
mod sources;
mod tally;
pub use output::Output;
pub mod types;
pub mod verify;
mod walked;
mod window;
pub use executor::execute_local_search;
#[cfg(any(test, feature = "test-support"))]
pub use types::LocalSearchResult;
pub use types::SearchStatus;

#[cfg(test)]
mod tests;
