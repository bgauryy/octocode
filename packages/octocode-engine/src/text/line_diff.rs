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
    // The public line contract splits on LF. Similar's built-in tokenizer also
    // splits on bare CR, which would invent extra lines inside source content.
    let old_lines: Vec<_> = old_text.split_inclusive('\n').collect();
    let new_lines: Vec<_> = new_text.split_inclusive('\n').collect();
    let diff = TextDiff::from_slices(&old_lines, &new_lines);
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
    use proptest::prelude::*;

    #[test]
    fn bare_cr_stays_inside_a_line_and_crlf_preserves_cr() {
        let ops = compute_line_diff_inner("", "first\rsecond\r\nlast");
        assert_eq!(
            ops.iter().map(|op| op.line.as_str()).collect::<Vec<_>>(),
            ["first\rsecond\r", "last"]
        );
        assert!(ops.iter().all(|op| op.op_type == "add"));
    }

    proptest! {
        #[test]
        fn edit_script_reconstructs_both_inputs(
            old in prop::collection::vec("[^\n]{0,16}", 0..32),
            new in prop::collection::vec("[^\n]{0,16}", 0..32),
        ) {
            let text = |lines: &[String]| {
                lines.iter().map(|line| format!("{line}\n")).collect::<String>()
            };
            let ops = compute_line_diff_inner(&text(&old), &text(&new));
            let reconstructed_old: Vec<_> = ops.iter()
                .filter(|op| op.op_type != "add")
                .map(|op| op.line.clone())
                .collect();
            let reconstructed_new: Vec<_> = ops.iter()
                .filter(|op| op.op_type != "remove")
                .map(|op| op.line.clone())
                .collect();
            prop_assert_eq!(reconstructed_old, old);
            prop_assert_eq!(reconstructed_new, new);
        }
    }

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
