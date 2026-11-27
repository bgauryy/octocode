//! Follow-up leads shared by several tools, built once: leads one tool
//! offers into another (so no tool imports the target tool's internals), an
//! empty search's broadening, and the prose hint beside them.

pub mod broaden;
pub mod files_listing;
pub mod find_file;
pub mod gh_file_read;
pub mod skipped;
pub mod unparsed;
