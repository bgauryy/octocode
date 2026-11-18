pub mod acquire;
pub mod capture;
pub use acquire::MAX_SOURCE_BYTES;
pub use capture::capture_text;
pub mod executor;
pub mod output;
pub use executor::execute_local_fetch;
pub use output::Output;

#[cfg(test)]
mod tests;
