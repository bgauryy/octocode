use super::octo::{match_deadline, parse_tree_with_deadline};
use crate::signatures::{MAX_PARSE_SIZE, languages};

/// Count ERROR and MISSING nodes with the selected grammar and parse
/// deadline; `None` for an oversized source, an unknown grammar, or a parse
/// cut by its deadline (no count is known).
pub fn count_syntax_errors(content: &str, selector: &str) -> Option<u32> {
    if content.len() > MAX_PARSE_SIZE {
        return None;
    }
    let entry = languages::select(selector).next()?;
    let tree = parse_tree_with_deadline(&entry.language, content, match_deadline(content)).ok()?;
    let mut count = 0u32;
    let mut cursor = tree.walk();
    loop {
        let node = cursor.node();
        if node.is_error() || node.is_missing() {
            count = count.saturating_add(1);
        }
        if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return Some(count);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syntax_checks_distinguish_valid_and_broken_source() {
        for (language, clean, broken) in [
            ("typescript", "const n = 1;", "const n = ;"),
            ("rust", "fn main() {}", "fn main( {}"),
            ("python", "def run():\n    return 1\n", "def run(:\n"),
        ] {
            assert_eq!(count_syntax_errors(clean, language), Some(0), "{language}");
            assert!(
                count_syntax_errors(broken, language).is_some_and(|count| count > 0),
                "{language}"
            );
        }
    }

    #[test]
    fn unsupported_and_oversized_inputs_have_no_count() {
        assert_eq!(count_syntax_errors("text", "unknown-language"), None);
        assert_eq!(
            count_syntax_errors(&"x".repeat(MAX_PARSE_SIZE + 1), "rust"),
            None
        );
    }
}
