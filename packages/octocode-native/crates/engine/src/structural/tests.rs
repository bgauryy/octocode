use super::files::{search_files_detailed, search_files_unfiltered};
use super::types::StructuralMatch;
use super::*;
use crate::test_support::TmpDir;
use serde_json::json;
use std::fs;

/// A structural search over `content`, parsed with the grammar
/// resolved from `ext`, returning bare matches. Exactly one of `pattern` /
/// `rule` must be `Some`. Production runs [`search_detailed`].
///
/// Returns `Err` for: an unsupported extension, an invalid pattern, invalid
/// rule object, or both/neither query supplied.
pub(crate) fn search(
    content: &str,
    ext: &str,
    pattern: Option<&str>,
    rule: Option<&Value>,
) -> Result<Vec<StructuralMatch>, String> {
    if content.len() > MAX_CONTENT_SIZE {
        return Err(format!(
            "[structural.content.tooLarge] structural search content exceeds {MAX_CONTENT_SIZE} byte limit"
        ));
    }
    let query = StructuralQuery::new(pattern, rule)
        .map_err(|message| format!("[structural.query.invalid] {message}"))?;
    let entry = languages::find_entry(ext)
        .filter(|entry| entry.admits(crate::text::analysis::AnalysisOperation::StructuralSearch))
        .ok_or_else(|| {
            format!(
                "[structural.language.unsupported] structural search does not support .{ext} files"
            )
        })?;
    let lang = AgLanguage::new(entry);
    let run = compile_matcher(&lang, &query).map_err(|error| error.to_string())?;
    // The non-detailed API returns bare StructuralMatch; node_kind is only
    // surfaced by the detailed shape.
    Ok(matches_with_recovery(&run, content)
        .map_err(|err| err.to_string())?
        .0
        .into_iter()
        .map(|m| m.matched)
        .collect())
}

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
        default_exclude_dir: None,
        ignore: crate::types::IgnorePolicy::default(),
        max_depth: Some(1),
        max_files: Some(limit),
        skip_files: None,
        max_file_bytes: None,
        judge: None,
        global_ignore: Default::default(),
    }
}

fn scan_options(root: &std::path::Path, skip: u32) -> StructuralSearchFilesOptions {
    StructuralSearchFilesOptions {
        path: root.to_string_lossy().into_owned(),
        pattern: Some("hit($A)".to_owned()),
        rule: None,
        include: None,
        exclude: None,
        default_exclude_dir: None,
        ignore: crate::types::IgnorePolicy::default(),
        max_depth: None,
        max_files: None,
        skip_files: Some(skip),
        max_file_bytes: None,
        judge: None,
        global_ignore: Default::default(),
    }
}

mod detailed;
mod differential;
mod file_reads;
mod limits;
mod pattern_facts;
mod patterns;
mod qualified;
mod queries;
mod residuals;
mod search_files;
mod single_items;
mod terminators;
mod unsaid;
mod unsaid_parts;
mod walker;
mod written_terminators;
