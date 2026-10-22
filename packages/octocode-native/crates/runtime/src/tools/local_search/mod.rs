mod binaries;
mod coverage;
mod cursor;
mod enclosing;
mod executor;
mod handoff;
mod layout;
mod leads;
mod manifest;
mod output;
mod packed;
mod probes;
mod regex_repair;
mod rows;
mod selection;
pub(crate) use output::Output;
mod types;
mod verify;
mod walked;
pub use executor::execute_local_search;
pub use types::*;

#[cfg(test)]
mod tests;
