use super::*;
use crate::test_support::TmpDir;
use serde_json::json;
use std::fs;

/// Diagnostic codes of a directory search, its own and each file's.
fn diagnostic_codes(result: &StructuralSearchFilesDetailedResult) -> Vec<&str> {
    result
        .diagnostics
        .iter()
        .chain(result.files.iter().flat_map(|file| &file.diagnostics))
        .map(|diagnostic| diagnostic.code.as_str())
        .collect()
}

/// Paths of the files a directory search matched in.
fn matched_paths(result: &StructuralSearchFilesDetailedResult) -> Vec<&str> {
    result
        .files
        .iter()
        .filter(|file| !file.matches.is_empty())
        .map(|file| file.path.as_str())
        .collect()
}

fn review_file_options(
    root: &std::path::Path,
    rule: serde_json::Value,
    limit: u32,
) -> StructuralSearchFilesOptions {
    StructuralSearchFilesOptions {
        path: root.to_string_lossy().into_owned(),
        pattern: None,
        rule: Some(rule),
        include: None,
        exclude: None,
        exclude_dir: None,
        default_exclude_dir: None,
        ignore: crate::types::IgnorePolicy::default(),
        max_depth: Some(1),
        max_files: Some(limit),
        skip_files: None,
        max_file_bytes: None,
    }
}

fn scan_options(root: &std::path::Path, skip: u32) -> StructuralSearchFilesOptions {
    StructuralSearchFilesOptions {
        path: root.to_string_lossy().into_owned(),
        pattern: Some("hit($A)".to_owned()),
        rule: None,
        include: None,
        exclude: None,
        exclude_dir: None,
        default_exclude_dir: None,
        ignore: crate::types::IgnorePolicy::default(),
        max_depth: None,
        max_files: None,
        skip_files: Some(skip),
        max_file_bytes: None,
    }
}

mod detailed;
mod file_reads;
mod limits;
mod patterns;
mod qualified;
mod queries;
mod search_files;
mod walker;
