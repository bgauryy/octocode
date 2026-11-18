//! Declaration extents that block reads widen to.

use super::*;

/// Rust declaration extents (0-based lines) that block reads
/// widen to: whole fns, impl blocks, and their methods.
#[test]
fn rust_declaration_ranges_cover_whole_items() {
    let source = "pub fn first(value: usize) -> usize {\n    let mut total = 0;\n    for step in 0..value {\n        total += step;\n    }\n    total\n}\n\npub struct Point;\n\nimpl Point {\n    pub fn origin() -> Self {\n        Point\n    }\n}\n";
    let spans: Vec<(String, u32, u32)> = extract_declarations_document(source, "src/lib.rs")
        .expect("declarations")
        .declarations
        .into_iter()
        .map(|declaration| {
            (
                declaration.name,
                declaration.range.start.line,
                declaration.range.end.line,
            )
        })
        .collect();
    for expected in [("first", 0, 6), ("origin", 11, 13)] {
        assert!(
            spans
                .iter()
                .any(|(name, start, end)| (name.as_str(), *start, *end) == expected),
            "{expected:?} in {spans:?}"
        );
    }
    assert!(
        spans
            .iter()
            .any(|(_, start, end)| (*start, *end) == (10, 14)),
        "impl block 10-14 in {spans:?}"
    );
    let mut body = String::from(
        "pub fn first(value: usize) -> usize {
",
    );
    for line in 1..=18 {
        body.push_str(&format!(
            "    let step{line} = value;
"
        ));
    }
    body.push_str(
        "    value
}

pub fn second() -> usize {
    let short = 1;
    short
}
",
    );
    let facts = extract_declarations_document(&body, "src/lib.rs").expect("declarations");
    let ranges: Vec<(u32, u32)> = facts
        .declarations
        .iter()
        .map(|d| (d.range.start.line, d.range.end.line))
        .collect();
    assert_eq!(ranges, vec![(0, 20), (22, 25)], "{facts:?}");
}

/// `(name, 0-based first line, 0-based last line, 0-based name line, doc line)`
/// of every declaration of `source`.
fn extents(source: &str, path: &str) -> Vec<(String, u32, u32, u32, Option<u32>)> {
    extract_declarations_document(source, path)
        .expect("declarations")
        .declarations
        .into_iter()
        .map(|d| {
            (
                d.name,
                d.range.start.line,
                d.range.end.line,
                d.selection_range.start.line,
                d.doc_line,
            )
        })
        .collect()
}

/// A declaration wrapped by its header (`template <…>` in C++ and CUDA, the
/// `@decorator` lines in Python) spans the header: its range starts at the
/// header and its doc comment is the one above it, while the selection
/// stays on the name.
#[test]
fn wrapped_declaration_ranges_include_their_header() {
    let template = "// doc
template <typename T>
__global__ void
k(T x) {
}

template <typename T>
struct S {
  template <typename U>
  void m(U u) {}
};

void plain() {}
";
    let paths: &[&str] = &[
        #[cfg(feature = "tree-sitter-cuda")]
        "a.cu",
        #[cfg(feature = "tree-sitter-cpp")]
        "a.cpp",
    ];
    for &path in paths {
        let found = extents(template, path);
        let of = |name: &str| found.iter().find(|d| d.0 == name).expect(name).clone();
        let k = of("k");
        assert_eq!(
            (k.1, k.2, k.3, k.4),
            (1, 4, 3, Some(0)),
            "{path}: {found:?}"
        );
        assert_eq!(of("S").1, 6, "{path}: {found:?}");
        assert_eq!((of("m").1, of("m").3), (8, 9), "{path}: {found:?}");
        assert_eq!(of("plain").1, 12, "{path}: {found:?}");
    }
    let python = "# doc
@dec
@dec2(1)
def foo():
    pass

class A:
    @property
    def b(self):
        return 1
";
    let found = extents(python, "a.py");
    let of = |name: &str| found.iter().find(|d| d.0 == name).expect(name).clone();
    assert_eq!(
        (of("foo").1, of("foo").3, of("foo").4),
        (1, 3, Some(0)),
        "{found:?}"
    );
    assert_eq!((of("b").1, of("b").3), (7, 8), "{found:?}");
}

/// A Rust item's range starts at its first outer attribute, nested items
/// included; a doc comment above or between the attributes is still its doc
/// line; inner attributes belong to the file; the source byte range starts
/// with the line range.
#[test]
fn rust_outer_attributes_join_the_declaration_header() {
    let source = "#![allow(dead_code)]\n/// Alpha.\n#[derive(Debug)]\n#[repr(C)]\npub struct Alpha {\n    #[allow(unused)]\n    field: u8,\n}\n\n#[allow(clippy::all)]\n/// Beta.\npub fn beta() {}\n\nimpl Alpha {\n    #[inline]\n    // note\n    #[must_use]\n    pub fn gamma(&self) {}\n}\n\n#[inline] fn delta() {}\n";
    let spans = extents(source, "lib.rs");
    let find = |name: &str| {
        spans
            .iter()
            .find(|span| span.0 == name)
            .unwrap_or_else(|| panic!("{name} in {spans:?}"))
    };
    // (name, first line, last line, name line, doc line), 0-based.
    assert_eq!(*find("Alpha"), ("Alpha".into(), 2, 7, 4, Some(1)));
    assert_eq!(*find("beta"), ("beta".into(), 9, 11, 11, Some(10)));
    assert_eq!(*find("gamma"), ("gamma".into(), 14, 17, 17, Some(15)));
    assert_eq!(*find("delta"), ("delta".into(), 20, 20, 20, None));
    let facts = extract_declarations_document(source, "lib.rs").expect("declarations");
    for declaration in &facts.declarations {
        let byte = declaration.source_range.as_ref().expect("source range");
        let line_of = |at: u32| source[..at as usize].matches('\n').count() as u32;
        assert_eq!(
            line_of(byte.start_byte),
            declaration.range.start.line,
            "{}",
            declaration.name
        );
    }
}

/// The annotation scan is one pass over each parent's children: a file of
/// many leading comments, attributes and attributed items reads in linear
/// time, so a quadratic sibling search would never finish this.
#[test]
fn leading_annotations_are_found_in_one_pass_over_the_siblings() {
    let mut source = String::new();
    let items = 60_000;
    for index in 0..items {
        source.push_str(&format!("// {index}\n#[inline]\nfn f{index}() {{}}\n"));
    }
    let spans = extents(&source, "lib.rs");
    assert_eq!(spans.len(), items);
    let last = &spans[items - 1];
    let first_line = u32::try_from(3 * (items - 1) + 1).expect("line");
    assert_eq!((last.1, last.2), (first_line, first_line + 1));
    assert_eq!(last.4, Some(first_line - 1));
}

/// A run of attributes on one item folds each attribute once, however long
/// the run: the work is linear in the attributes, never their square.
#[test]
fn a_long_attribute_run_folds_each_attribute_once() {
    let attributes = 12_000;
    let mut source = "#[allow(dead_code)]\n".repeat(attributes);
    source.push_str("fn target() {}\n");
    let before = FOLDED_ATTRIBUTES.with(std::cell::Cell::get);
    let spans = extents(&source, "lib.rs");
    let folded = FOLDED_ATTRIBUTES.with(std::cell::Cell::get) - before;
    let attributes_u32 = u32::try_from(attributes).expect("count");
    assert_eq!(
        spans,
        vec![("target".into(), 0, attributes_u32, attributes_u32, None)]
    );
    assert!(
        folded <= attributes,
        "{folded} folds for {attributes} attributes"
    );
}

/// The doc rule is one for every language: the comment run above the header
/// is the doc, else the one between the header's first row and the item's own
/// line. A comment between a decorator, template header or attribute and its
/// item is the item's doc line.
#[test]
fn a_comment_between_the_header_and_the_item_is_its_doc_in_every_language() {
    let mut cases: Vec<(&str, &str, &str)> = vec![
        ("a.py", "@dec\n# note\ndef f():\n    pass\n", "f"),
        ("lib.rs", "#[inline]\n// note\nfn f() {}\n", "f"),
    ];
    #[cfg(feature = "tree-sitter-cpp")]
    cases.push((
        "a.cpp",
        "template <typename T>\n// note\nT f(T x) { return x; }\n",
        "f",
    ));
    for (path, source, name) in cases {
        let spans = extents(source, path);
        let found = spans
            .iter()
            .find(|span| span.0 == name)
            .unwrap_or_else(|| panic!("{name} in {spans:?} for {path}"));
        assert_eq!(found.4, Some(1), "{path}: {spans:?}");
    }
    // A header that already holds the comment (Java annotations sit inside
    // the declaration node) has it above its first row only.
    {
        let spans = extents(
            "class A {\n  // above\n  @Deprecated\n  void f() {}\n}\n",
            "A.java",
        );
        let found = spans.iter().find(|span| span.0 == "f").expect("f");
        assert_eq!((found.1, found.4), (2, Some(1)), "{spans:?}");
    }
}
