use super::*;
use crate::test_support::TmpDir;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

use crate::error::ErrorKind;
use crate::search::fs_resume::WalkCursor;
use crate::search::walk::IgnorePolicy;
use crate::types::{EntryType, FileKind};

mod cursor;
mod filters;
mod metadata_bounds;
mod options;
mod resume;
mod walk_order;

fn query_file_system_inner(
    options: FileSystemQueryOptions,
) -> Result<FileSystemQueryResult, String> {
    query_file_system(options, &|_, _, _| true, &|| false).map_err(|err| err.reason)
}
