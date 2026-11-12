//! The review pick: the changed files whose patches answer "what changed",
//! packed into one call's patch budget.
use super::missing_patch::is_binary_name;
use crate::tools::gh_get_history_item::util::{str_at, usize_at};
use octocode_engine::text::test_paths::is_test_path;
use serde_json::Value;

/// Review priority of a changed path: source first, then tests, then other
/// files; docs, changesets, lockfiles, and generated output last.
fn review_tier(path: &str) -> u8 {
    use crate::content::{FileType, classify_file_type};
    let lower = path.to_ascii_lowercase();
    let generated = lower.starts_with(".changeset/")
        || lower.contains("/.changeset/")
        || lower.contains("generated")
        || lower.contains(".min.")
        || lower.ends_with(".snap")
        || lower.contains("__snapshots__/");
    match classify_file_type(path) {
        _ if generated => 3,
        Some(FileType::Doc | FileType::Lock) => 3,
        Some(FileType::Code) if is_test_path(path) => 1,
        Some(FileType::Code) => 0,
        _ => 2,
    }
}

/// Estimated rendered patch chars per changed line (diff marker, text,
/// newline), and per file header, for packing a review into one budget.
const REVIEW_LINE_CHARS: usize = 45;

const REVIEW_FILE_CHARS: usize = 80;

/// Optional patch selection: the files whose patches answer "what changed" —
/// source files first (then other non-test, non-doc files), most changed
/// lines first — packed into one call's patch budget. Tests, docs,
/// changesets, lockfiles and generated output are left to the inventory.
/// Always names the top file when one qualifies, whatever its size.
pub(in crate::tools::gh_get_history_item) fn review_selection(
    files: &[Value],
    budget: usize,
) -> Vec<String> {
    let mut ranked = files
        .iter()
        .enumerate()
        .filter_map(|(index, file)| {
            let path = str_at(file, "/filename")?;
            let binary = is_binary_name(path);
            let tier = if binary { 3 } else { review_tier(path) };
            let changed = usize_at(file, "/additions") + usize_at(file, "/deletions");
            (tier == 0 || tier == 2).then_some((
                (tier, std::cmp::Reverse(changed), index),
                path,
                changed,
            ))
        })
        .collect::<Vec<_>>();
    ranked.sort_by_key(|(rank, _, _)| *rank);
    // The pick is the most-changed file and the files whose whole patches
    // fit one call's budget (packed, most changed first): a lead names how
    // to read more, not the whole list. Every other patch stays one
    // `sections: ["patches"]` read away.
    let mut used = 0usize;
    let mut packed = Vec::new();
    let mut top = None;
    for (_, path, changed) in ranked {
        top.get_or_insert(path);
        let cost = changed.saturating_mul(REVIEW_LINE_CHARS) + REVIEW_FILE_CHARS;
        if used + cost <= budget {
            used += cost;
            packed.push(path.to_owned());
        }
    }
    if let Some(top) = top.filter(|top| packed.first().is_none_or(|first| first != top)) {
        packed.insert(0, top.to_owned());
    }
    // One lead's `include` admits at most the contract's bound.
    packed.truncate(crate::contracts::query_limits::gh_get_history_item::INCLUDE_MAX_ITEMS);
    packed
}
