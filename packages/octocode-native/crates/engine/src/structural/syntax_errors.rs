use super::octo::parse_tree_with_deadline;
use crate::signatures::extractor::whole_file_deadline;
use crate::signatures::languages;

/// Count the recovered syntax errors with the selected grammar: each
/// outermost ERROR span once and each MISSING node outside one; `None` for
/// an unknown grammar, or a parse its deadline or work bound cut (no count
/// is known).
pub fn count_syntax_errors(content: &str, selector: &str) -> Option<u32> {
    let entry = languages::select(selector).next()?;
    let tree = parse_tree_with_deadline(&entry.language, content, whole_file_deadline()).ok()?;
    Some(error_spans(tree.root_node()))
}

/// The outermost ERROR and MISSING nodes under `root`: a clean tree costs
/// one check, and the walk enters only subtrees that hold an error.
fn error_spans(root: tree_sitter::Node<'_>) -> u32 {
    if !root.has_error() {
        return 0;
    }
    let mut count = 0u32;
    let mut cursor = root.walk();
    loop {
        let node = cursor.node();
        let descend = if node.is_error() || node.is_missing() {
            count = count.saturating_add(1);
            false
        } else {
            node.has_error()
        };
        if descend && cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return count;
            }
        }
    }
}

#[cfg(test)]
mod tests;
