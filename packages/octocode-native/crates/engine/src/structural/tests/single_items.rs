//! B121: a single capture in a list binds one item; another item of that
//! list after the pattern's end fails the match, with or without a closing
//! token to end it.

use super::*;

fn lines(src: &str, ext: &str, pattern: &str) -> Vec<u32> {
    search(src, ext, Some(pattern), None)
        .unwrap_or_else(|error| panic!("{ext} `{pattern}`: {error:?}"))
        .iter()
        .map(|found| found.start_line)
        .collect()
}

/// A list with no closing token (Python imports, `global`) ends where the
/// pattern does only when no further item follows.
#[test]
fn a_single_capture_binds_one_item_of_an_open_list() {
    let py = "from m import a\nfrom m import a, b\nimport a\nimport a, b\n";
    assert_eq!(lines(py, "py", "from $M import $X"), [1]);
    assert_eq!(lines(py, "py", "from $M import $$$X"), [1, 2]);
    assert_eq!(lines(py, "py", "import $X"), [3]);
    let ts = "let a = 1;\nlet b = 1, c = 2;\n";
    assert_eq!(lines(ts, "ts", "let $A = 1"), [1]);
}

/// A declaration that declares several names is not the one-name pattern:
/// the further item sits mid-list (Go `name`, C `declarator`) or in a
/// one-item list the capture alone spells (Go's `expression_list`).
#[test]
fn a_single_capture_binds_one_declared_name() {
    let go = "package p\nfunc f() {\n  var a int\n  var p, q int\n  a := g()\n  b, c := g()\n}\n";
    assert_eq!(lines(go, "go", "var $A int"), [3]);
    assert_eq!(lines(go, "go", "$A := $C"), [5]);
    // Items of other kinds, a separator between them, are still more items.
    let assign = "package p\nfunc f() {\n  a = g()\n  s.x, b = g()\n}\n";
    assert_eq!(lines(assign, "go", "$A = $C"), [3]);
    let c = "int a = 1;\nint b = 1, c = 2;\n";
    assert_eq!(lines(c, "c", "int $A = $B;"), [1]);
}

/// A declaration's members are a set (the contract: a declaration pattern
/// "requires what it spells and ignores the rest"): one spelled member finds
/// the type among others, whether the list is the `body` (Rust, C) or held
/// through a wrapper (Go's `struct_type`), and enum variants alike.
#[test]
fn a_spelled_member_is_found_among_others() {
    let rs = "struct S { a: u8 }\nstruct T { a: u8, b: u8 }\nenum E { A }\nenum F { A, B }\n";
    assert_eq!(lines(rs, "rs", "struct $S { $A: u8 }"), [1, 2]);
    assert_eq!(lines(rs, "rs", "enum $E { A }"), [3, 4]);
    let c = "struct S { int a; };\nstruct T { int a; int b; };\nenum E { A };\nenum F { A, B };\n";
    assert_eq!(lines(c, "c", "struct $S { int $A; }"), [1, 2]);
    assert_eq!(lines(c, "c", "enum $E { A }"), [3, 4]);
    let go = "package p\ntype S struct { a int }\ntype T struct {\n  a int\n  b int\n}\n";
    assert_eq!(lines(go, "go", "type $S struct { $A int }"), [2, 3]);
}

/// Match arms and call arguments are no declaration's members: they stay
/// exact.
#[test]
fn arms_and_arguments_stay_exact() {
    let rs =
        "fn f() {\n  match x { A => 1, }\n  match x { A => 1, B => 2, }\n  g(a);\n  g(a, b);\n}\n";
    assert_eq!(lines(rs, "rs", "match $X { A => 1, }"), [2]);
    assert_eq!(lines(rs, "rs", "g($A)"), [4]);
}

/// A list a closing token ends already held one item per capture: Rust use
/// lists, TS import specifiers and call arguments.
#[test]
fn a_single_capture_binds_one_item_of_a_closed_list() {
    let rs = "use a::{b};\nuse a::{b, c};\n";
    assert_eq!(lines(rs, "rs", "use a::{$X};"), [1]);
    let ts = "import { a } from 'm';\nimport { a, b } from 'm';\nf(a);\nf(a, b);\n";
    assert_eq!(lines(ts, "ts", "import { $X } from 'm'"), [1]);
    assert_eq!(lines(ts, "ts", "f($A)"), [3]);
}
