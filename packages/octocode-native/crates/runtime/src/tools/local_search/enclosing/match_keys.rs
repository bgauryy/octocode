//! Key paths of hit lines in shown JSON and TOML files (`matchKeys`), read
//! from the same current bytes the enclosing names are.
use super::super::types::SearchFile;
use super::{row_lines, shown_bytes};
use crate::security::ContentSecurity;

/// Set `matchKeys` on each shown JSON or TOML file with hit rows: the key
/// path of every hit line, read from the file's current bytes, in page
/// order while the page stays within `page_budget`. Returns the note for
/// files whose keys did not fit, never dropped silently.
pub(in super::super) fn annotate_match_keys(
    files: &mut [SearchFile],
    output_root: &std::path::Path,
    page_budget: usize,
    expected: &dyn Fn(&std::path::Path) -> super::super::sources::Expected,
    security: &ContentSecurity,
) -> Option<String> {
    let mut used = octocode_engine::json_size::bytes(&*files).unwrap_or(usize::MAX);
    let mut over_budget = 0usize;
    for file in files.iter_mut() {
        let source = output_root.join(&file.path);
        let Some(rows) = file.matches.as_ref().filter(|rows| !rows.is_empty()) else {
            continue;
        };
        if !crate::tools::match_keys::is_keyed_path(&source) {
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
        let limit = crate::security::SourceLimits::REAL.whole_source;
        let Some(bytes) = shown_bytes(&source, expected(&source)) else {
            continue;
        };
        let Ok(text) = security.decode_source_bytes(&bytes, limit) else {
            continue;
        };
        let keys = crate::tools::match_keys::line_key_paths(&source, &text, &lines);
        if keys.is_empty() {
            continue;
        }
        let added =
            octocode_engine::json_size::bytes(&crate::tools::match_keys::MatchKeyRows(&keys))
                .unwrap_or(usize::MAX)
                .saturating_add(MATCH_KEYS_FIELD_CHARS);
        if over_budget == 0 && used.saturating_add(added) <= page_budget {
            used += added;
            file.match_keys = keys;
        } else {
            over_budget += 1;
        }
    }
    (over_budget > 0).then(|| {
        let noun = if over_budget == 1 { "file" } else { "files" };
        format!(
            "Key paths (matchKeys) omitted on {over_budget} later {noun} to keep the page within its size budget."
        )
    })
}

/// Serialized chars around a file's key map.
const MATCH_KEYS_FIELD_CHARS: usize = ",\"matchKeys\":".len();
