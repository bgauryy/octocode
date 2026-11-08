//! B110: a pattern requires what the caller wrote, a `;` included, in every
//! grammar: a written `;` after the pattern's node (outside it, in the
//! statement or declaration around it) means the node ends there and is
//! terminated; an unwritten one asks for neither. A declaration reads the
//! same wherever the grammar declares it (a local or a class field).

use super::*;

fn lines(src: &str, ext: &str, pattern: &str) -> Vec<u32> {
    search(src, ext, Some(pattern), None)
        .unwrap_or_else(|error| panic!("{ext} `{pattern}`: {error:?}"))
        .iter()
        .map(|found| found.start_line)
        .collect()
}

/// F2: `struct $S;` spells a struct that ends at its name; `struct $S`
/// spells no end, so it matches the struct with a body too.
#[test]
fn a_written_semicolon_after_a_c_struct_requires_its_end() {
    let src = "struct S;\nstruct T { int a; };\n";
    for ext in ["c", "cpp"] {
        assert_eq!(lines(src, ext, "struct $S;"), [1], "{ext}");
        assert_eq!(lines(src, ext, "struct $S"), [1, 2], "{ext}");
    }
}

/// F3: `foo($X);` spells a call statement terminated by `;`; a call
/// without one, or inside another statement, is not it. `foo($X)` spells
/// the call alone, wherever it is.
#[test]
fn a_written_semicolon_after_a_call_requires_a_terminated_statement() {
    let ts = "foo(1);\nfoo(2)\nconst a = foo(3);\n";
    assert_eq!(lines(ts, "ts", "foo($X);"), [1]);
    assert_eq!(lines(ts, "ts", "foo($X)"), [1, 2, 3]);
    let rust = "fn f() {\n    foo(1);\n    let a = foo(3);\n    foo(2)\n}\n";
    assert_eq!(lines(rust, "rs", "foo($X);"), [2]);
    assert_eq!(lines(rust, "rs", "foo($X)"), [2, 3, 4]);
}

/// B112: a declaration a grammar nests in another node (a C#
/// `local_declaration_statement` or `field_declaration` holds a
/// `variable_declaration`) is a declaration too, so a pattern leaves its
/// modifiers unsaid; a modifier the pattern spells stays required.
#[test]
fn a_nested_declaration_leaves_its_modifiers_unsaid() {
    let csharp = "class K {\n  private int a = 1;\n  int c = 3;\n  void M() { const int d = 2; int b = 4; }\n}\n";
    assert_eq!(lines(csharp, "cs", "int $A = $B"), [2, 3, 4, 4]);
    assert_eq!(lines(csharp, "cs", "int $A = $B;"), [2, 3, 4, 4]);
    assert_eq!(lines(csharp, "cs", "const int $A = $B"), [4]);
}

/// B110-3/4: a C++ class member is spelled as at the top level, so a
/// pattern matches the member as it does the top-level declaration or
/// definition, with a written `;` or a written body.
#[test]
fn a_cpp_member_matches_its_top_level_spelling() {
    let src = "class C {\n  int m(int x);\n  int n(int x) {}\n};\nint p(int x);\nint q(int x) {}\n";
    assert_eq!(lines(src, "cpp", "int $F(int $X);"), [2, 5]);
    assert_eq!(lines(src, "cpp", "int $F(int $X) {}"), [3, 6]);
    assert_eq!(lines(src, "cpp", "int $F(int $X)"), [2, 3, 5, 6]);
}

/// B110-2: a capture in a declaration's name needs a name: an anonymous
/// struct has none.
#[test]
fn a_captured_name_needs_a_name() {
    let src = "struct { int q; } v;\nstruct T { int a; };\n";
    for ext in ["c", "cpp"] {
        assert_eq!(lines(src, ext, "struct $S"), [2], "{ext}");
    }
}

/// F4: a variable declaration matches a class field as it does a local.
#[test]
fn a_java_or_csharp_declaration_matches_fields_and_locals() {
    let java = "class K {\n  int a = 1;\n  void m() { int b = 2; }\n}\n";
    assert_eq!(lines(java, "java", "int $A = $B"), [2, 3]);
    assert_eq!(lines(java, "java", "int $A = $B;"), [2, 3]);
    let csharp = "class K {\n  int a = 1;\n  void M() { int b = 2; }\n}\n";
    assert_eq!(lines(csharp, "cs", "int $A = $B"), [2, 3]);
    assert_eq!(lines(csharp, "cs", "int $A = $B;"), [2, 3]);
}
