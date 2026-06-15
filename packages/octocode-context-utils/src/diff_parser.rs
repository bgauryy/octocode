/// Unified diff parser and filter.
///
/// Replaces `utils/parsers/diff.ts` in octocode-tools-core which called
/// `patch.split('\n')` independently in both `filterPatch` and `trimDiffContext`,
/// allocating the line array twice per invocation. This module processes a diff
/// in a single pass combining both operations.
use crate::types::{FilterPatchOptions, ParsedPatchLine, PatchLineType};

// ── internal patch line type ──────────────────────────────────────────────────

struct PatchLine {
    original_line_number: Option<u32>,
    new_line_number: Option<u32>,
    content: String,
    line_type: PatchLineType,
}

// ── core parsing ──────────────────────────────────────────────────────────────

fn parse_patch(patch: &str) -> Vec<PatchLine> {
    let mut result: Vec<PatchLine> = Vec::new();
    let mut original_line: i64 = 0;
    let mut new_line: i64 = 0;

    for line in patch.split('\n') {
        if line.starts_with("@@") {
            // Parse: @@ -start[,count] +start[,count] @@
            let re_match = parse_hunk_header(line);
            if let Some((orig_start, new_start)) = re_match {
                original_line = orig_start as i64 - 1;
                new_line = new_start as i64 - 1;
            }
            continue;
        }

        if line.starts_with('+') {
            new_line += 1;
            result.push(PatchLine {
                original_line_number: None,
                new_line_number: Some(new_line as u32),
                content: line.to_owned(),
                line_type: PatchLineType::Addition,
            });
        } else if line.starts_with('-') {
            original_line += 1;
            result.push(PatchLine {
                original_line_number: Some(original_line as u32),
                new_line_number: None,
                content: line.to_owned(),
                line_type: PatchLineType::Deletion,
            });
        } else if !line.starts_with('\\') {
            original_line += 1;
            new_line += 1;
            result.push(PatchLine {
                original_line_number: Some(original_line as u32),
                new_line_number: Some(new_line as u32),
                content: line.to_owned(),
                line_type: PatchLineType::Context,
            });
        }
    }

    result
}

/// Extract (original_start, new_start) from a `@@ -N[,N] +N[,N] @@` header.
fn parse_hunk_header(line: &str) -> Option<(u32, u32)> {
    // Find "-N" and "+N"
    let after_at = line.strip_prefix("@@")?.trim_start();
    let minus_pos = after_at.find('-')?;
    let plus_pos = after_at.find('+')?;

    let orig_str = &after_at[minus_pos + 1..];
    let orig_num: u32 = orig_str
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()?;

    let new_str = &after_at[plus_pos + 1..];
    let new_num: u32 = new_str
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()?;

    Some((orig_num, new_num))
}

// ── public API ────────────────────────────────────────────────────────────────

/// Filter a unified diff patch and optionally trim surrounding context.
///
/// - `additions`: keep only these new-file line numbers (all additions kept if `None`)
/// - `deletions`: keep only these original-file line numbers (all deletions kept if `None`)
/// - `trim_context`: trim pure context lines to at most `context_lines` around changes
/// - `context_lines`: window size for `trim_context`, default 2
pub(crate) fn filter_patch_inner(patch: &str, options: Option<FilterPatchOptions>) -> String {
    if patch.is_empty() {
        return String::new();
    }

    let additions = options.as_ref().and_then(|o| o.additions.as_ref());
    let deletions = options.as_ref().and_then(|o| o.deletions.as_ref());
    let trim_context = options
        .as_ref()
        .and_then(|o| o.trim_context)
        .unwrap_or(false);
    let context_lines = options.as_ref().and_then(|o| o.context_lines).unwrap_or(2) as usize;

    // If no filtering and no trimming requested — return early
    if additions.is_none() && deletions.is_none() && !trim_context {
        return patch.to_owned();
    }

    if additions.is_none() && deletions.is_none() && trim_context {
        return trim_raw_diff_context(patch, context_lines);
    }

    let parsed = parse_patch(patch);

    // ── Filter ────────────────────────────────────────────────────────────────
    let add_set: Option<std::collections::HashSet<u32>> =
        additions.map(|v| v.iter().map(|&n| n as u32).collect());
    let del_set: Option<std::collections::HashSet<u32>> =
        deletions.map(|v| v.iter().map(|&n| n as u32).collect());

    let filtered: Vec<&PatchLine> = if additions.is_some() || deletions.is_some() {
        parsed
            .iter()
            .filter(|line| match line.line_type {
                PatchLineType::Addition => {
                    if let Some(n) = line.new_line_number {
                        add_set.as_ref().is_none_or(|s| s.contains(&n))
                    } else {
                        false
                    }
                }
                PatchLineType::Deletion => {
                    if let Some(n) = line.original_line_number {
                        del_set.as_ref().is_none_or(|s| s.contains(&n))
                    } else {
                        false
                    }
                }
                PatchLineType::Context => {
                    add_set.as_ref().is_none_or(|s| !s.is_empty())
                        || del_set.as_ref().is_none_or(|s| !s.is_empty())
                }
            })
            .collect()
    } else {
        parsed.iter().collect()
    };

    if filtered.is_empty() {
        return String::new();
    }

    // ── Trim context ──────────────────────────────────────────────────────────
    let output_lines: Vec<String> = if trim_context && filtered.len() > 30 {
        apply_context_trim(&filtered, context_lines)
    } else {
        filtered.iter().map(|pl| format_patch_line(pl)).collect()
    };

    output_lines.join("\n")
}

/// Trim raw unified diff context while preserving the original diff format.
/// This powers `trimDiffContext`: no line-number annotations, hunk headers are
/// retained, and unchanged/no-op cases return the original patch.
fn trim_raw_diff_context(patch: &str, context_lines: usize) -> String {
    const TRIM_THRESHOLD_LINES: usize = 30;

    let lines: Vec<&str> = patch.split('\n').collect();
    if lines.len() <= TRIM_THRESHOLD_LINES {
        return patch.to_owned();
    }

    let changed_indexes: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| is_changed_raw_patch_line(line).then_some(index))
        .collect();

    if changed_indexes.is_empty() {
        return patch.to_owned();
    }

    let mut keep = vec![false; lines.len()];
    for (index, line) in lines.iter().enumerate() {
        if line.starts_with("@@") {
            keep[index] = true;
        }
    }

    for index in changed_indexes {
        let start = index.saturating_sub(context_lines);
        let end = (index + context_lines).min(lines.len().saturating_sub(1));
        keep[start..=end].fill(true);
    }

    let mut trimmed: Vec<&str> = Vec::new();
    let mut omitted = false;
    for (index, line) in lines.iter().enumerate() {
        if keep[index] {
            trimmed.push(line);
            omitted = false;
        } else if !omitted {
            trimmed.push("...");
            omitted = true;
        }
    }

    let result = trimmed.join("\n");
    if result.len() < patch.len() {
        result
    } else {
        patch.to_owned()
    }
}

fn is_changed_raw_patch_line(line: &str) -> bool {
    if line.starts_with("+++") || line.starts_with("---") {
        return false;
    }
    line.starts_with('+') || line.starts_with('-')
}

/// Apply context trimming: keep at most `context_lines` pure-context lines
/// around each changed line. Matches `trimDiffContext` from TypeScript.
fn apply_context_trim(lines: &[&PatchLine], context_lines: usize) -> Vec<String> {
    // Identify changed line indices
    let changed: std::collections::HashSet<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| {
            matches!(
                l.line_type,
                PatchLineType::Addition | PatchLineType::Deletion
            )
        })
        .map(|(i, _)| i)
        .collect();

    if changed.is_empty() {
        return lines.iter().map(|l| format_patch_line(l)).collect();
    }

    let n = lines.len();
    let mut keep: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for &ci in &changed {
        let lo = ci.saturating_sub(context_lines);
        let hi = (ci + context_lines).min(n - 1);
        for j in lo..=hi {
            keep.insert(j);
        }
    }

    let mut result: Vec<String> = Vec::new();
    let mut prev_kept = true;

    for (i, line) in lines.iter().enumerate() {
        if keep.contains(&i) {
            result.push(format_patch_line(line));
            prev_kept = true;
        } else {
            if prev_kept {
                result.push("...".to_owned());
            }
            prev_kept = false;
        }
    }

    result
}

fn format_patch_line(pl: &PatchLine) -> String {
    let content_body = if pl.content.len() > 1 {
        &pl.content[1..]
    } else {
        ""
    };
    match pl.line_type {
        PatchLineType::Addition => {
            format!("+{}: {content_body}", pl.new_line_number.unwrap_or(0))
        }
        PatchLineType::Deletion => {
            format!("-{}: {content_body}", pl.original_line_number.unwrap_or(0))
        }
        PatchLineType::Context => {
            format!(" {}: {content_body}", pl.new_line_number.unwrap_or(0))
        }
    }
}

/// Expose parsed patch lines for testing. Returns a simplified representation.
#[allow(dead_code)]
pub(crate) fn parse_patch_for_inspection(patch: &str) -> Vec<ParsedPatchLine> {
    parse_patch(patch)
        .into_iter()
        .map(|pl| ParsedPatchLine {
            original_line_number: pl.original_line_number,
            new_line_number: pl.new_line_number,
            content: pl.content,
            line_type: pl.line_type,
        })
        .collect()
}

// ── unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_patch() -> &'static str {
        "@@ -1,4 +1,4 @@\n context1\n-deleted\n+added\n context2"
    }

    #[test]
    fn parse_hunk_header_extracts_numbers() {
        assert_eq!(parse_hunk_header("@@ -10,3 +12,5 @@"), Some((10, 12)));
        assert_eq!(parse_hunk_header("@@ -1 +1 @@"), Some((1, 1)));
    }

    #[test]
    fn parse_patch_classifies_lines() {
        let lines = parse_patch(sample_patch());
        assert_eq!(lines.len(), 4);
        assert!(matches!(lines[0].line_type, PatchLineType::Context));
        assert!(matches!(lines[1].line_type, PatchLineType::Deletion));
        assert!(matches!(lines[2].line_type, PatchLineType::Addition));
        assert!(matches!(lines[3].line_type, PatchLineType::Context));
    }

    #[test]
    fn filter_patch_no_options_returns_original() {
        let patch = sample_patch();
        let result = filter_patch_inner(patch, None);
        assert_eq!(result, patch);
    }

    #[test]
    fn filter_patch_empty_returns_empty() {
        assert_eq!(filter_patch_inner("", None), "");
    }

    #[test]
    fn filter_patch_filters_by_additions() {
        let patch = "@@ -1,3 +1,3 @@\n context\n+line2\n+line3";
        let result = filter_patch_inner(
            patch,
            Some(FilterPatchOptions {
                additions: Some(vec![2]),
                deletions: None,
                trim_context: None,
                context_lines: None,
            }),
        );
        assert!(result.contains("+2:"));
        assert!(!result.contains("+3:"));
    }

    #[test]
    fn filter_patch_filters_by_deletions() {
        let patch = "@@ -1,3 +1,3 @@\n context\n-line2\n-line3";
        let result = filter_patch_inner(
            patch,
            Some(FilterPatchOptions {
                additions: None,
                deletions: Some(vec![2]),
                trim_context: None,
                context_lines: None,
            }),
        );
        assert!(result.contains("-2:"));
        assert!(!result.contains("-3:"));
    }

    #[test]
    fn trim_context_preserves_changed_lines() {
        // Build a >30 line patch
        let mut lines = vec!["@@ -1,50 +1,50 @@".to_owned()];
        for i in 1..=48 {
            lines.push(format!(" context{i}"));
        }
        lines.push("+added_line".to_owned());
        lines.push("-deleted_line".to_owned());
        let patch = lines.join("\n");

        let result = filter_patch_inner(
            &patch,
            Some(FilterPatchOptions {
                additions: None,
                deletions: None,
                trim_context: Some(true),
                context_lines: Some(2),
            }),
        );
        assert!(result.contains("added_line"));
        assert!(result.contains("deleted_line"));
        assert!(result.contains("..."));
    }

    #[test]
    fn trim_context_preserves_raw_diff_format_for_full_patch() {
        let mut lines = vec!["@@ -1,35 +1,36 @@".to_owned()];
        for i in 0..15 {
            lines.push(format!(" ctx{i}"));
        }
        lines.push("+added".to_owned());
        for i in 0..19 {
            lines.push(format!(" after{i}"));
        }
        let patch = lines.join("\n");

        let result = filter_patch_inner(
            &patch,
            Some(FilterPatchOptions {
                additions: None,
                deletions: None,
                trim_context: Some(true),
                context_lines: Some(2),
            }),
        );

        assert!(result.len() < patch.len());
        assert!(result.contains("@@ -1,35 +1,36 @@"));
        assert!(result.contains("+added"));
        assert!(result.contains(" ctx13"));
        assert!(result.contains(" ctx14"));
        assert!(result.contains(" after0"));
        assert!(result.contains(" after1"));
        assert!(result.contains("..."));
        assert!(!result.contains("+16: added"));
    }

    #[test]
    fn trim_context_returns_original_when_no_changed_lines() {
        let patch = (0..35)
            .map(|i| format!(" context{i}"))
            .collect::<Vec<_>>()
            .join("\n");

        let result = filter_patch_inner(
            &patch,
            Some(FilterPatchOptions {
                additions: None,
                deletions: None,
                trim_context: Some(true),
                context_lines: Some(2),
            }),
        );

        assert_eq!(result, patch);
    }

    #[test]
    fn trim_context_skipped_for_short_patches() {
        let patch = sample_patch();
        let result = filter_patch_inner(
            patch,
            Some(FilterPatchOptions {
                additions: None,
                deletions: None,
                trim_context: Some(true),
                context_lines: None,
            }),
        );
        // Short patch (< 30 lines) — should not add "..." markers
        assert!(!result.contains("..."));
    }
}
