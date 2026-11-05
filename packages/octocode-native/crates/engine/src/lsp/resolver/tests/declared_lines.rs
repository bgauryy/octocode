//! The lines a walk of only the nodes spelling a name finds it declared on are
//! the lines the whole file's graph facts name it on.
use crate::lsp::resolver::code_occurrences;
use crate::signatures::extract_graph_facts;

/// The whole file's answer, in order.
fn from_facts(file: &str, source: &str, name: &str) -> Vec<usize> {
    let mut lines = extract_graph_facts(source, file)
        .expect("facts")
        .declarations
        .iter()
        .filter(|declaration| declaration.name == name)
        .map(|declaration| declaration.selection_range.start.line as usize)
        .collect::<Vec<_>>();
    lines.sort_unstable();
    lines
}

fn narrow(file: &str, source: &str, name: &str) -> Option<Vec<usize>> {
    let occurrences = code_occurrences(file, source, name).expect("occurrences");
    let mut lines = occurrences.declared_lines(file, source, name)?;
    lines.sort_unstable();
    Some(lines)
}

fn same(file: &str, source: &str, name: &str, expected: &[usize]) {
    assert_eq!(from_facts(file, source, name), expected, "facts: {file}");
    assert_eq!(
        narrow(file, source, name).as_deref(),
        Some(expected),
        "narrow: {file}"
    );
}

#[test]
fn every_grammar_declares_the_name_where_its_facts_do() {
    same(
        "a.rs",
        "pub struct S { pub origin: u8 }\npub fn origin() {}\nmod m {\n    pub fn origin() { origin(); }\n}\nimpl S {\n    fn origin(&self) {}\n}\n",
        "origin",
        &[1, 3, 6],
    );
    same(
        "a.py",
        "def origin():\n    pass\nclass C:\n    def origin(self):\n        return origin()\n",
        "origin",
        &[0, 3],
    );
    same(
        "a.go",
        "package p\nfunc origin() {}\ntype T struct{ origin int }\nconst (\n\torigin2 = 1\n)\nfunc use() { origin() }\n",
        "origin",
        &[1],
    );
    same(
        "a.c",
        "int origin(void);\nint origin(void) { return 0; }\nint use(void) { return origin(); }\n",
        "origin",
        &[0, 1],
    );
    same(
        "A.java",
        "class A {\n    int origin() { return origin(); }\n    int origin;\n}\n",
        "origin",
        &[1],
    );
}

#[test]
fn a_rust_test_keeps_its_name_and_a_comment_declares_nothing() {
    same(
        "a.rs",
        "// fn origin() {}\n#[test]\nfn origin() {}\nconst S: &str = \"fn origin() {}\";\n",
        "origin",
        &[2],
    );
}

/// A graph holds only what compiles: a disabled branch names nothing.
#[test]
fn a_disabled_preprocessor_branch_declares_nothing() {
    same(
        "a.c",
        "#if 0\nint origin(void) { return 1; }\n#else\nint origin(void) { return 2; }\n#endif\nint use(void) { return origin(); }\n",
        "origin",
        &[3],
    );
}

/// A Go file no build includes is no graph at all.
#[test]
fn a_build_excluded_go_file_declares_nothing() {
    same(
        "a.go",
        "//go:build ignore\n\npackage p\n\nfunc origin() { origin() }\n",
        "origin",
        &[],
    );
}

/// The file's other macros decide which bodies the whole walk reads, so a body
/// declaring the name is left to it.
#[test]
fn a_macro_body_declaring_the_name_is_left_to_the_whole_walk() {
    let items = "make_items! {\n    pub fn origin() {}\n}\nfn other() { origin(); }\n";
    assert_eq!(narrow("a.rs", items, "origin"), None);
    assert_eq!(from_facts("a.rs", items, "origin"), [1]);
    let expression = "fn f() { assert!({ fn origin() {} origin(); true }); }\n";
    assert_eq!(narrow("a.rs", expression, "origin"), None);
    assert_eq!(from_facts("a.rs", expression, "origin"), [0]);
}

/// A body that only uses the name, or does not hold it, changes nothing.
#[test]
fn a_macro_body_declaring_nothing_keeps_the_narrow_answer() {
    same(
        "a.rs",
        "make_items! { fn other() {} }\nfn origin() {}\n",
        "origin",
        &[1],
    );
    same(
        "a.rs",
        "fn origin() {}\nfn f() { assert!(origin() == ()); vec![origin()]; }\n",
        "origin",
        &[0],
    );
    same(
        "a.rs",
        "make_items! { fn other() { origin(); } }\nfn origin() {}\n",
        "origin",
        &[1],
    );
    same(
        "a.rs",
        "fn f() { assert!(ready!(origin())); }\nfn origin() {}\n",
        "origin",
        &[1],
    );
}

/// A shadowed macro's body is not read by the whole walk: declaring the name
/// there leaves the answer to it, and using the name there changes nothing.
#[test]
fn a_shadowed_macro_body_agrees_with_the_whole_walk() {
    let declares = "macro_rules! assert { ($($t:tt)*) => {}; }\nfn f() { assert!({ fn origin() {} true }); }\nfn g() { origin(); }\n";
    assert_eq!(from_facts("a.rs", declares, "origin"), Vec::<usize>::new());
    assert_eq!(narrow("a.rs", declares, "origin"), None);
    same(
        "a.rs",
        "macro_rules! assert { ($($t:tt)*) => {}; }\nfn origin() {}\nfn f() { assert!(origin()); }\n",
        "origin",
        &[1],
    );
}

/// The whole walk stops at a count of item bodies: past it, a body's items go
/// unread, which a walk of only the bodies holding the name cannot see.
#[test]
fn item_bodies_past_the_whole_walks_count_are_left_to_it() {
    let mut source = "make_items! { fn other() {} }\n".repeat(300);
    source.push_str("make_items! { fn origin() {} }\nfn f() { origin(); }\n");
    assert_eq!(from_facts("a.rs", &source, "origin"), Vec::<usize>::new());
    assert_eq!(narrow("a.rs", &source, "origin"), None);
}

#[cfg(feature = "tree-sitter-cpp")]
#[test]
fn a_googletest_file_is_left_to_the_whole_walk() {
    let source = "#include <gtest/gtest.h>\nTEST(Suite, origin) {\n  origin();\n}\n";
    assert_eq!(narrow("a.cpp", source, "origin"), None);
    same("b.cpp", "void origin() {\n  origin();\n}\n", "origin", &[0]);
}

/// A `.h` the C grammar cannot read is read as C++ for its occurrences, while
/// the facts keep the grammar they pick for the header; either way one answer.
#[cfg(feature = "tree-sitter-cpp")]
#[test]
fn a_header_keeps_the_grammar_of_its_facts() {
    same(
        "a.h",
        "class A {\n public:\n  int origin();\n};\nint use(A a) { return a.origin(); }\n",
        "origin",
        &[2],
    );
    same("b.h", "int origin(void);\n", "origin", &[0]);
}

/// oxc owns these files, and a name that is no one identifier is not narrowed.
#[test]
fn oxc_files_and_other_names_are_left_to_the_whole_walk() {
    assert_eq!(
        narrow("a.ts", "export function origin() {}\n", "origin"),
        None
    );
    assert_eq!(narrow("a.rs", "fn origin() {}\n", "a b"), None);
}
