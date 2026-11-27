//! How a listing names its entries.

/// The name of a listing entry `<name>[/][ (<fields>)]`.
pub fn entry_name(entry: &str) -> &str {
    octocode_runtime_base::response::groups::entry_fields(entry).map_or(entry, |(name, _)| name)
}
