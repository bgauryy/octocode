//! Key paths of hit lines in shown JSON and TOML files (`matchKeys`), read
//! from the same current bytes the enclosing names are.
use super::super::types::SearchFile;
use super::{row_lines, shown_bytes};
use octocode_runtime_base::security::ContentSecurity;

/// Set `matchKeys` on each shown JSON or TOML file with hit rows: the key
/// path of every hit line, read from the file's current bytes, in line
/// order. Every key path of a shown hit is kept: a page larger than its
/// window is cut by the response pager, which routes `files[].matchKeys` as
/// an evidence collection and leads to the rest, so no key path is dropped
/// to fit a size.
pub(in super::super) fn annotate_match_keys(
    files: &mut [SearchFile],
    output_root: &std::path::Path,
    expected: &dyn Fn(&std::path::Path) -> super::super::sources::Expected,
    security: &ContentSecurity,
) {
    for file in files.iter_mut() {
        let source = output_root.join(&file.path);
        let Some(rows) = file.matches.as_ref().filter(|rows| !rows.is_empty()) else {
            continue;
        };
        if !octocode_tools_shared::match_keys::is_keyed_path(&source) {
            continue;
        }
        let mut lines = rows
            .iter()
            .flat_map(row_lines)
            .filter(|line| *line > 0)
            .map(|line| line as usize)
            .collect::<Vec<_>>();
        lines.sort_unstable();
        lines.dedup();
        let limit = octocode_runtime_base::security::SourceLimits::REAL.whole_source;
        let Some(bytes) = shown_bytes(&source, expected(&source)) else {
            continue;
        };
        let Ok(text) = security.decode_source_bytes(&bytes, limit) else {
            continue;
        };
        let keys = octocode_tools_shared::match_keys::line_key_paths(&source, &text, &lines);
        if keys.is_empty() {
            continue;
        }
        file.match_keys = keys;
    }
}
