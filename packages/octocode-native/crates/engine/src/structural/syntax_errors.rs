use super::octo::parse_tree_with_deadline;
use crate::signatures::extractor::whole_file_deadline;
use crate::signatures::{MAX_PARSE_SIZE, languages};

/// Count the recovered syntax errors with the selected grammar: each
/// outermost ERROR span once and each MISSING node outside one; `None` for
/// an oversized source, an unknown grammar, or a parse cut by its deadline
/// (no count is known).
pub fn count_syntax_errors(content: &str, selector: &str) -> Option<u32> {
    if content.len() > MAX_PARSE_SIZE {
        return None;
    }
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

    /// Every node, counting the ERROR and MISSING nodes no ERROR encloses.
    fn outermost_by_full_walk(node: tree_sitter::Node<'_>, inside_error: bool) -> u32 {
        let own = u32::from(!inside_error && (node.is_error() || node.is_missing()));
        let mut cursor = node.walk();
        let children = node
            .children(&mut cursor)
            .map(|child| outermost_by_full_walk(child, inside_error || node.is_error()))
            .sum::<u32>();
        own + children
    }

    /// A recovered parse counts each outermost error span once, never the
    /// ERROR nodes nested inside it, whatever the shape of the damage.
    #[test]
    fn nested_error_nodes_count_as_one_span() {
        for (language, broken) in [
            (
                "typescript",
                "const n = ;\nfunction f( { if (}) }\nclass { ]]] }",
            ),
            ("rust", "fn main( {}\nimpl for { fn ( }\nstruct ;;; {{"),
            (
                "c",
                "int f( { class X { int y; }; }\ntemplate <typename T> T g(T);",
            ),
            ("python", "def run(:\n    return [1,\nclass :\n"),
        ] {
            let entry = languages::select(language).next().expect(language);
            let tree = parse_tree_with_deadline(&entry.language, broken, whole_file_deadline())
                .expect("parse");
            let expected = outermost_by_full_walk(tree.root_node(), false);
            assert!(expected > 0, "{language}");
            assert_eq!(
                count_syntax_errors(broken, language),
                Some(expected),
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
