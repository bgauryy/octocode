//! Follow-up leads shared by several tools, built once: leads one tool
//! offers into another (so no tool imports the target tool's internals), an
//! empty search's broadening, and the prose hint beside them.

pub(crate) mod broaden;
pub(crate) mod files_listing;
pub(crate) mod find_file;
pub(crate) mod gh_file_read;
pub(crate) mod lsp;
pub(crate) mod skipped;

