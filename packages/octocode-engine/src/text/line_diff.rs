//! Myers line diff via the `similar` crate — used by Pi edit-tool and any
//! caller that needs a fast edit script without the O(N·M) LCS cliff.

use similar::{ChangeTag, TextDiff};

/// One line in a Myers edit script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineDiffOpInner {
    /// `"same"` | `"add"` | `"remove"`
    pub op_type: String,
    pub line: String,
}

/// Compute a full line-level edit script from `old_text` → `new_text`.
pub(crate) fn compute_line_diff_inner(old_text: &str, new_text: &str) -> Vec<LineDiffOpInner> {
    let diff = TextDiff::from_lines(old_text, new_text);
    let mut ops = Vec::new();
    for change in diff.iter_all_changes() {
        // similar yields trailing newlines on values; strip so callers match
        // JS `split('\n')` semantics (no embedded `\n` in the line field).
        let line = change.value().trim_end_matches('\n').to_owned();
        let op_type = match change.tag() {
            ChangeTag::Equal => "same",
            ChangeTag::Delete => "remove",
            ChangeTag::Insert => "add",
        };
        ops.push(LineDiffOpInner {
            op_type: op_type.to_owned(),
            line,
        });
    }
    ops
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_line_change() {
        let ops = compute_line_diff_inner("a\nb\nc\n", "a\nB\nc\n");
        let changed: Vec<_> = ops.into_iter().filter(|o| o.op_type != "same").collect();
        assert_eq!(changed.len(), 2);
        assert_eq!(changed[0].op_type, "remove");
        assert_eq!(changed[0].line, "b");
        assert_eq!(changed[1].op_type, "add");
        assert_eq!(changed[1].line, "B");
    }

    #[test]
    fn identical_is_all_same() {
        let ops = compute_line_diff_inner("one\ntwo\n", "one\ntwo\n");
        assert!(ops.iter().all(|o| o.op_type == "same"));
    }
}
