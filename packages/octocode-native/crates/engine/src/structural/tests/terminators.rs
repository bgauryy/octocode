//! B88: a pattern written without its terminator spells neither a body nor
//! a `;`, so it matches the declaration with either; one written with the
//! `;` matches only the bodiless form. Compilation appends a `;` the caller
//! did not write when a fragment needs it to parse, and that `;` may make
//! the root a smaller kind than the bodied declaration (Rust
//! `function_signature_item` beside `function_item`, C `declaration` beside
//! `function_definition`).
//!
//! A `;` written after the pattern's node, and declarations in another place
//! (Java/C# fields), are B110's: `tests/written_terminators.rs`.

use super::*;

fn lines(src: &str, ext: &str, pattern: &str) -> Vec<u32> {
    search(src, ext, Some(pattern), None)
        .unwrap_or_else(|error| panic!("{ext} `{pattern}`: {error:?}"))
        .iter()
        .map(|found| found.start_line)
        .collect()
}

/// F1: the appended `;` makes the Rust root a `function_signature_item`, yet
/// the pattern spells no `;`: it matches the bodied function too. A written
/// `;` still selects the signature alone, and a written body the definition.
#[test]
fn a_rust_signature_without_its_semicolon_matches_the_bodied_function() {
    let src = "fn a(x: i32) {}\nfn b(x: i32);\nfn c(y: u8) {}\npub fn d(x: i32) -> u8 { 0 }\n";
    assert_eq!(lines(src, "rs", "fn $F($X: i32)"), [1, 2, 4]);
    assert_eq!(lines(src, "rs", "fn $F($X: i32);"), [2]);
    assert_eq!(lines(src, "rs", "fn $F($X: i32) { $$$ }"), [1, 4]);
}

/// The declaration a terminated pattern spells names no fuller kind it
/// already matches; a written `;` keeps the smaller form, whose fuller kind
/// it leaves out.
#[test]
fn only_a_written_semicolon_leaves_the_fuller_kind_out() {
    let unwritten = pattern_declaration("fn $N($$$A)", "rs").expect("unwritten");
    assert!(unwritten.fuller_kinds.is_empty(), "{unwritten:?}");
    let written = pattern_declaration("fn $N($$$A);", "rs").expect("written");
    assert_eq!(written.kind, "function_signature_item");
    assert_eq!(written.fuller_kinds, ["function_item"]);
}

/// C and C++: a function declaration without its `;` matches the
/// definition too; with it, only the declaration.
#[test]
fn a_c_family_prototype_without_its_semicolon_matches_the_definition() {
    let src = "int a(int x) { return x; }\nint b(int x);\nlong c(long y) { return y; }\n";
    for ext in ["c", "cpp"] {
        assert_eq!(lines(src, ext, "int $F(int $X)"), [1, 2], "{ext}");
        assert_eq!(lines(src, ext, "int $F(int $X);"), [2], "{ext}");
    }
}

/// TypeScript: an overload signature without its `;` matches the
/// implementation too; with it, only the signature.
#[test]
fn a_typescript_signature_without_its_semicolon_matches_the_implementation() {
    let src = "function a(x: number) {}\nfunction b(x: number);\nfunction c(y: string) {}\n";
    assert_eq!(lines(src, "ts", "function $F($X: number)"), [1, 2]);
    assert_eq!(lines(src, "ts", "function $F($X: number);"), [2]);
}

/// The bodied form counts for the declaration the pattern wraps too (an
/// export's), not only for its root.
#[test]
fn an_exported_signature_without_its_semicolon_matches_the_exported_implementation() {
    let src = "export function w() {}\nexport function v();\nfunction u() {}\n";
    assert_eq!(lines(src, "ts", "export function $F()"), [1, 2]);
    assert_eq!(lines(src, "ts", "export function $F();"), [2]);
}

/// The bodiless form reaches what the bodied form does: the grammar's longer
/// forms of the bodied declaration (a generator) too.
#[test]
fn a_signature_without_its_semicolon_reaches_the_longer_bodied_forms() {
    let src = "function a() {}\nfunction* gen() {}\n";
    assert_eq!(lines(src, "ts", "function $F() {}"), [1, 2]);
    assert_eq!(lines(src, "ts", "function $F()"), [1, 2]);
}

/// Java and C#: a method without its `;` matches the bodied method too;
/// with it, only the abstract one.
#[test]
fn a_java_or_csharp_method_without_its_semicolon_matches_the_bodied_one() {
    let java = "abstract class K {\n  void a(int x) {}\n  abstract void b(int x);\n  void c(long y) {}\n}\n";
    assert_eq!(lines(java, "java", "void $M(int $X)"), [2, 3]);
    assert_eq!(lines(java, "java", "void $M(int $X);"), [3]);
    let csharp = "abstract class K {\n  void A(int x) {}\n  abstract void B(int x);\n  void C(long y) {}\n}\n";
    assert_eq!(lines(csharp, "cs", "void $M(int $X)"), [2, 3]);
    assert_eq!(lines(csharp, "cs", "void $M(int $X);"), [3]);
}
