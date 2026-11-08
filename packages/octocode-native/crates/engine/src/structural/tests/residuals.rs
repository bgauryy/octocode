//! B118: the parts a pattern leaves out that carry no meaning of their own
//! (a lifetime, a leading separator) and the ones that do (a variadic
//! `...`, a bare `*` marker).

use super::*;

fn lines(src: &str, ext: &str, pattern: &str) -> Vec<u32> {
    search(src, ext, Some(pattern), None)
        .unwrap_or_else(|error| panic!("{ext} `{pattern}`: {error:?}"))
        .iter()
        .map(|found| found.start_line)
        .collect()
}

/// A reference type's lifetime annotates it, so `&str` reads `&'a str`;
/// `&mut` changes what the reference is, so it stays required.
#[test]
fn a_lifetime_is_left_unsaid_and_mut_is_not() {
    let src = "fn g(a: &'static str) {}\nfn h<'a>(a: &'a str) {}\nfn k(a: &str) {}\nfn m(a: &mut str) {}\nfn n(a: String) {}\n";
    assert_eq!(lines(src, "rs", "fn $N($A: &str) {}"), [1, 2, 3]);
    let lets =
        "fn f() {\n    let a: &'static str = x;\n    let b: &mut u8 = y;\n    let c: &u8 = z;\n}\n";
    assert_eq!(lines(lets, "rs", "let $A: &$T = $B;"), [2, 4]);
}

/// A leading separator separates nothing: `| None` is the arm `None`, and
/// `| A | B` the or-pattern `A | B`. A leading token that is no separator
/// of its node (`-x`) keeps its meaning.
#[test]
fn a_leading_separator_is_left_unsaid() {
    let src = "fn m(x: E) -> u8 {\n    match x {\n        | None => 0,\n        | A | B => 1,\n        _ => 2,\n    }\n}\n";
    let arm = |context: &str| {
        let rule = json!({"pattern": {"context": context, "selector": "match_arm"}});
        search(src, "rs", None, Some(&rule))
            .unwrap_or_else(|error| panic!("`{context}`: {error:?}"))
            .iter()
            .map(|found| found.start_line)
            .collect::<Vec<_>>()
    };
    assert_eq!(arm("match x { None => $A, }"), [3]);
    assert_eq!(arm("match x { A | B => $E, }"), [4]);
    let negated = "fn f() {\n    foo(-x);\n    foo(x);\n}\n";
    assert_eq!(lines(negated, "rs", "foo(x)"), [3]);
}

/// A variadic parameter's `...` changes what it takes: a plain parameter
/// pattern is no variadic one, in Go as in Java.
#[test]
fn a_variadic_marker_is_required() {
    let src = "package p\n\nfunc f(a int, rest ...string) {}\nfunc g(a int, b string) {}\n";
    assert_eq!(lines(src, "go", "func $F($A int, $B string) {}"), [4]);
    assert_eq!(lines(src, "go", "func $F($A int, $R ...string) {}"), [3]);
}

/// A single capture binds a part, never a bare punctuation marker: Python's
/// keyword-only `*` is no parameter `$B` names.
#[test]
fn a_capture_does_not_bind_a_bare_marker() {
    let src = "def h(*, k):\n    pass\ndef j(a, b):\n    pass\n";
    assert_eq!(lines(src, "py", "def $F($A, $B): $$$"), [3]);
    assert_eq!(lines(src, "py", "def $F(*, $K): $$$"), [1]);
}
