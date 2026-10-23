mod acquire;
mod capture;
pub(crate) use acquire::MAX_SOURCE_BYTES;
pub(crate) use capture::capture_text;
mod executor;
mod large_source;
mod output;
pub use executor::execute_local_fetch;
pub(crate) use output::Output;

#[cfg(test)]
mod tests;
