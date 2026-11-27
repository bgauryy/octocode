//! The process environment as UTF-8 text. `std::env::vars()` panics on the
//! first non-UTF-8 entry, which would let any stray variable crash the CLI;
//! the lossy form keeps every entry and replaces only the invalid bytes.

use std::collections::BTreeMap;
use std::ffi::OsString;

/// Every environment entry, invalid UTF-8 replaced rather than rejected.
pub fn vars_lossy() -> BTreeMap<String, String> {
    lossy_pairs(std::env::vars_os())
}

/// The lossy conversion over any `(name, value)` source; testable without
/// mutating the process environment.
fn lossy_pairs(pairs: impl IntoIterator<Item = (OsString, OsString)>) -> BTreeMap<String, String> {
    pairs
        .into_iter()
        .map(|(name, value)| {
            (
                name.to_string_lossy().into_owned(),
                value.to_string_lossy().into_owned(),
            )
        })
        .collect()
}

#[cfg(test)]
#[cfg(unix)]
mod tests;
