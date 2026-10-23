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
    for path in ["a.cu", "a.cpp"] {
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
