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
fn only_an_unknown_grammar_or_an_unfinished_parse_has_no_count() {
    assert_eq!(count_syntax_errors("text", "unknown-language"), None);
    // No size refusal: a source as large as a whole read admits is
    // counted; the parse's linear work bound is what stops a parse.
    let line = "fn a() {}\n";
    let large = line.repeat(crate::security::MAX_CONTENT_SIZE / line.len());
    assert_eq!(count_syntax_errors(&large, "rust"), Some(0));
}
