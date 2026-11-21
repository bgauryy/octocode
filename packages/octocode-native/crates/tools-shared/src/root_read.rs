//! The entry text shared by local and remote directory listings.
//! The grammar lives with the listing groups
//! (`octocode_runtime_base::response::groups`), below every module that
//! writes or reads an entry.
pub use octocode_runtime_base::response::groups::{entry_fields, entry_text, listing_entry_note};

#[cfg(test)]
mod tests;
