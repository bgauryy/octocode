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
/// Always names at least the top file when one qualifies.
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
    // Files that fit one call's budget come first (packed, most changed
    // first), so the first read delivers whole patches; the rest follow in
    // rank order and arrive through `continuePatch`.
    let mut used = 0usize;
    let mut packed = Vec::new();
    let mut rest = Vec::new();
    for (_, path, changed) in ranked {
        let cost = changed.saturating_mul(REVIEW_LINE_CHARS) + REVIEW_FILE_CHARS;
        if used + cost <= budget {
            used += cost;
            packed.push(path.to_owned());
        } else {
            rest.push(path.to_owned());
        }
    }
    packed.extend(rest);
    // The pick rides one lead's `include`: as many files as it admits. The
    // rest of the diff stays one `readPatches` away.
    packed.truncate(crate::contracts::query_limits::gh_get_history_item::INCLUDE_MAX_ITEMS);
    packed
}
