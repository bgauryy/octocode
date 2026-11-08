//! B108: what a pattern leaves unsaid is read by what the candidate
//! declares, and a literal name reads the same in every spelling the
//! grammar gives that slot.

use super::*;

fn lines(src: &str, ext: &str, pattern: &str) -> Vec<u32> {
    search(src, ext, Some(pattern), None)
        .unwrap_or_else(|error| panic!("{ext} `{pattern}`: {error:?}"))
        .iter()
        .map(|found| found.start_line)
        .collect()
}

/// A bodiless pattern spells no part that makes it a declaration, but the
/// candidate is one: the generics the pattern leaves out are unsaid.
#[test]
fn a_bodiless_pattern_leaves_the_generics_unsaid() {
    let src = "impl<T> ToolOutput for Y<T> {}\nimpl ToolOutput for Z {}\nimpl Other for W {}\n";
    assert_eq!(lines(src, "rs", "impl ToolOutput for $T"), [1, 2]);
    assert_eq!(lines(src, "rs", "impl ToolOutput for $T { $$$ }"), [1, 2]);
}

/// A token that changes what a declaration means is spelled, never unsaid:
/// `impl Tr for $T` is no negative impl (`impl !Tr for V`), while `unsafe`,
/// a modifier, is left unsaid.
#[test]
fn a_meaning_token_is_never_unsaid() {
    let src =
        "impl<T> !Tr for V<T> {}\nimpl !Tr for V {}\nunsafe impl Tr for W {}\nimpl Tr for X {}\n";
    assert_eq!(lines(src, "rs", "impl Tr for $T"), [3, 4]);
    assert_eq!(lines(src, "rs", "impl Tr for $T { $$$ }"), [3, 4]);
    assert_eq!(lines(src, "rs", "impl !Tr for $T {}"), [1, 2]);
}

/// A body holds its statements as the pattern spells them: none of them is
/// an unsaid part of the declaration around it (C, C++, Java as Rust, TS
/// and C# read it).
#[test]
fn a_body_holds_its_statements_as_spelled() {
    let c = "void a() { foo(); int x = 1; }\nvoid b() { int y = 2; foo(); }\nvoid c() { foo(); }\nvoid d() { int z = 3; }\n";
    for ext in ["c", "cpp"] {
        assert_eq!(lines(c, ext, "void $N() { int $B = $C; }"), [4], "{ext}");
        assert_eq!(
            lines(c, ext, "void $N() { $$$A; int $B = $C; }"),
            [1, 4],
            "{ext}"
        );
    }
    let java = "class K {\n  void a() { foo(); int x = 1; }\n  void b() { int y = 2; foo(); }\n  void c() { foo(); }\n  void d() { int z = 3; }\n}\n";
    assert_eq!(lines(java, "java", "void $N() { int $B = $C; }"), [5]);
    assert_eq!(
        lines(java, "java", "void $N() { $$$A; int $B = $C; }"),
        [2, 5]
    );
}

/// A literal name matches the same name the grammar spells with another
/// kind in the same slot (a C++ member's `field_identifier` for the
/// top-level `identifier`), and no other name.
#[test]
fn a_literal_name_matches_its_member_spelling() {
    let src = "class C {\n  int m(int x);\n  int n(int x);\n};\nint m(int x);\n";
    assert_eq!(lines(src, "cpp", "int m(int $X);"), [2, 5]);
}
