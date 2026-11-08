//! The Rust block scan ([`rust_block_after`]) against the grammar's own tree: at
//! the start of every line it says a block comment or a literal is open exactly
//! when a node of that kind spans the line start, until it declares a place in
//! doubt (`hidden`), after which it reads every mark and only the soup's
//! invariant holds it.

use super::*;
use tree_sitter::Node;

mod soup;

/// Every `.rs` file under `dir`, recursively.
fn rust_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("a readable directory") {
        let path = entry.expect("an entry").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// Whether a node of one of `kinds` holds `at` strictly inside it.
fn inside(root: Node<'_>, at: usize, kinds: &[&str]) -> bool {
    let mut node = root.descendant_for_byte_range(at, at);
    while let Some(found) = node {
        if kinds.contains(&found.kind()) && found.start_byte() < at && at < found.end_byte() {
            return true;
        }
        node = found.parent();
    }
    false
}

/// What the scan and the tree each say of every line start of `text`, as the
/// lines where they differ.
fn disagreements(text: &str, tree: &Tree) -> Vec<String> {
    let grammar = grammar_of("a.rs").expect("a Rust grammar");
    let mut state = BlockState::default();
    let mut start = 0;
    let mut wrong = Vec::new();
    for (number, line) in text.split_inclusive('\n').enumerate() {
        if state.hidden {
            break;
        }
        let in_block = inside(tree.root_node(), start, &["block_comment"]);
        let in_literal = inside(
            tree.root_node(),
            start,
            &["string_literal", "raw_string_literal"],
        );
        if (state.depth > 0) != in_block || (state.literal != Literal::None) != in_literal {
            wrong.push(format!(
                "line {}: scan block {} literal {}, tree block {in_block} literal {in_literal}: {line:?}",
                number + 1,
                state.depth > 0,
                state.literal != Literal::None,
            ));
        }
        state = block_after(line.trim_end_matches(['\n', '\r']), state, grammar);
        start += line.len();
    }
    wrong
}

fn parse_rust(text: &str) -> Option<Tree> {
    let grammar = grammar_of("a.rs").expect("a Rust grammar");
    crate::signatures::extractor::parse_until_stopped(text, &grammar.language, &|| false)
        .ok()
        .filter(|tree| !tree.root_node().has_error())
}

#[test]
fn the_rust_scan_agrees_with_the_tree_on_every_line_start_of_the_engine_sources() {
    let mut files = Vec::new();
    rust_files(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    let mut checked = 0;
    let mut wrong = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file).expect("a UTF-8 source");
        let Some(tree) = parse_rust(&text) else {
            continue;
        };
        checked += 1;
        wrong.extend(
            disagreements(&text, &tree)
                .into_iter()
                .map(|line| format!("{}: {line}", file.display())),
        );
    }
    assert!(checked > 100, "only {checked} sources were compared");
    assert!(
        wrong.is_empty(),
        "{} lines differ:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

/// A line comment that holds a block opener puts the Rust scan in doubt for
/// the rest of the text: it reads every mark, including those a literal holds.
#[test]
fn a_line_comment_holding_a_block_opener_puts_the_rust_scan_in_doubt() {
    let grammar = grammar_of("a.rs").expect("a Rust grammar");
    let plain = block_after("/// a doc \"quote", BlockState::default(), grammar);
    assert!(!plain.hidden && !plain.is_open());
    for line in [
        "/// a doc \"quote /* here",
        "/////*",
        "let x = 1; // a /* b",
    ] {
        let doubt = block_after(line, BlockState::default(), grammar);
        assert!(doubt.hidden && doubt.is_open(), "{line:?}");
        let closed = block_after("*/ let s = \"/*\";", doubt, grammar);
        assert!(
            closed.hidden && closed.is_open(),
            "{line:?}: a literal's mark counts"
        );
    }
}

#[test]
fn the_rust_scan_agrees_with_the_tree_on_literal_shapes() {
    let text = [
        "fn f() {",
        "let a = \"abc",
        "def\" /* open \"",
        "password: x */;",
        "let r = r#\"line \"quoted\"",
        "still raw\"# /* open",
        "x */;",
        "let c = br\"x\";",
        "let d = '\"'; // \"",
        "/* a /* nested */ \"quote",
        "*/ let e = \"/*\";",
        "let g = b'\"'; let g2 = '\\'';",
        "let h = \"/* not a comment\";",
        "let i = \"trailing \\",
        "  continued /* inside */\";",
        "}",
        "fn lifetime<'a>(x: &'a str) {}",
        // One escape per line: a second mispaired one would pair the stray
        // quote again and heal the state before the line ends.
        "fn escapes() { let t = ('\\x41','\"'); /* open",
        "see the password: x */ }",
        "fn unicode() { let u = ('\\u{41}','\"'); /* open",
        "see the password: x */ }",
        "",
    ]
    .join("\n");
    let tree = parse_rust(&text).expect("a text that parses");
    assert_eq!(disagreements(&text, &tree), Vec::<String>::new());
}
