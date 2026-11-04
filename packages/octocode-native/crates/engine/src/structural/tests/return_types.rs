//! The annotated spelling of a pattern ([`pattern_with_return_types`]): which
//! patterns have one in each grammar, how it is spelled, and what it matches.

use super::*;

fn annotated(pattern: &str, ext: &str) -> Option<String> {
    pattern_with_return_types(pattern, ext)
}

fn match_lines(src: &str, ext: &str, pattern: &str) -> Vec<usize> {
    search(src, ext, Some(pattern), None)
        .expect("pattern search should succeed")
        .iter()
        .map(|found| found.start_line as usize)
        .collect()
}

/// Every grammar with an optional return type: the pattern as written matches
/// only the function without one, its annotated spelling only the function
/// with one. No table of languages says so: each grammar's parse does.
#[test]
fn each_grammar_with_an_optional_return_type_spells_its_own() {
    let cases: &[(&str, &str, &str, &str)] = &[
        (
            "ts",
            "function $N($$$) { $$$ }",
            "function $N($$$): $R { $$$ }",
            "function a() {}\nfunction b(x: number): Promise<void> { return p; }\n",
        ),
        (
            "tsx",
            "function $N($$$) { $$$ }",
            "function $N($$$): $R { $$$ }",
            "function a() {}\nfunction b(): JSX.Element { return <div />; }\n",
        ),
        (
            "rs",
            "fn $N($$$) { $$$ }",
            "fn $N($$$) -> $R { $$$ }",
            "fn a() {}\nfn b(x: u8) -> Result<u8, E> { Ok(x) }\n",
        ),
        (
            "py",
            "def $N($$$):\n    $$$",
            "def $N($$$) -> $R:\n    $$$",
            "def a():\n    pass\ndef b(x) -> int:\n    return x\n",
        ),
        (
            "go",
            "func $N($$$) { $$$ }",
            "func $N($$$) $R { $$$ }",
            "package p\nfunc a() {}\nfunc b(x int) (int, error) { return x, nil }\n",
        ),
        (
            "scala",
            "def $N($$$) = $B",
            "def $N($$$): $R = $B",
            "object O {\n  def a(x: Int) = x\n  def b(x: Int): Int = x\n}\n",
        ),
    ];
    for (ext, pattern, spelled, src) in cases {
        let found = annotated(pattern, ext).unwrap_or_else(|| panic!("{ext}: {pattern}"));
        assert_eq!(&found, spelled, "{ext}");
        let plain = match_lines(src, ext, pattern);
        let typed = match_lines(src, ext, &found);
        assert_eq!(plain.len(), 1, "{ext}: {pattern} {plain:?}");
        assert_eq!(typed.len(), 1, "{ext}: {found} {typed:?}");
        assert!(plain[0] < typed[0], "{ext}: {plain:?} {typed:?}");
    }
}

/// The function need not be the root: exports, assignments, classes and
/// decorators keep their shape around it, and each outermost function takes
/// its own capture.
#[test]
fn a_function_inside_the_pattern_takes_the_capture_where_it_stands() {
    for (pattern, ext, spelled) in [
        (
            "export function $N($$$) { $$$ }",
            "ts",
            "export function $N($$$): $R { $$$ }",
        ),
        (
            "export default function $N($$$) { $$$ }",
            "ts",
            "export default function $N($$$): $R { $$$ }",
        ),
        (
            "const $F = ($$$) => { $$$ }",
            "ts",
            "const $F = ($$$): $R => { $$$ }",
        ),
        (
            "export const $F = async ($$$) => $B",
            "ts",
            "export const $F = async ($$$): $R => $B",
        ),
        (
            "class $C { $M($$$) { $$$ } }",
            "ts",
            "class $C { $M($$$): $R { $$$ } }",
        ),
        (
            "class $C { $A($$$) { $$$ }\n $B($$$) { $$$ } }",
            "ts",
            "class $C { $A($$$): $R { $$$ }\n $B($$$): $R2 { $$$ } }",
        ),
        (
            "impl $T { fn $N($$$) { $$$ } }",
            "rs",
            "impl $T { fn $N($$$) -> $R { $$$ } }",
        ),
        ("g(|$A| { $$$ })", "rs", "g(|$A| -> $R { $$$ })"),
        (
            "func ($T) $N($$$) { $$$ }",
            "go",
            "func ($T) $N($$$) $R { $$$ }",
        ),
    ] {
        assert_eq!(
            annotated(pattern, ext).as_deref(),
            Some(spelled),
            "{pattern}"
        );
    }
}

/// An exported, arrow and method function with a return type are what the
/// spelled patterns find, whatever the type's own shape.
#[test]
fn the_capture_takes_any_return_type() {
    let src = "export function a(): Map<string, number[]> { return m; }\nexport const b = (x: number): number => x;\nclass C {\n  run(): void {}\n}\n";
    let lines = |pattern: &str| {
        let spelled = annotated(pattern, "ts").expect("spelled");
        match_lines(src, "ts", &spelled)
    };
    assert_eq!(lines("export function $N($$$) { $$$ }"), vec![1]);
    assert_eq!(lines("export const $F = ($$$) => $B"), vec![2]);
    assert_eq!(lines("class $C { $M($$$) { $$$ } }"), vec![3]);
}

/// A pattern that spells a return type is its own answer, and the added
/// capture never reuses a name the pattern binds.
#[test]
fn a_spelled_return_type_stays_and_the_capture_is_free() {
    for (pattern, ext) in [
        ("function $N($$$): $R { $$$ }", "ts"),
        ("function $N(): void {}", "ts"),
        ("fn $N($$$) -> $T { $$$ }", "rs"),
        ("def $N($$$) -> int:\n    $$$", "py"),
        ("func $N($$$) error { $$$ }", "go"),
    ] {
        assert_eq!(annotated(pattern, ext), None, "{ext}: {pattern}");
    }
    assert_eq!(
        annotated("function $R($$$) { $$$ }", "ts").as_deref(),
        Some("function $R($$$): $R2 { $$$ }")
    );
}

/// Grammars whose return type is required or absent parse no annotation, and
/// a pattern without parameters has no slot.
#[test]
fn nothing_to_annotate_has_no_spelling() {
    for (pattern, ext) in [
        ("function $N($$$) { $$$ }", "js"),
        ("const $F = ($$$) => $B", "js"),
        ("void $N($$$) { $$$ }", "java"),
        ("int $N($$$) { $$$ }", "c"),
        ("int $N($$$) { $$$ }", "cpp"),
        ("public int $N($$$) { $$$ }", "cs"),
        ("foo($X)", "ts"),
        ("$X.unwrap()", "rs"),
        ("$X => $Y", "ts"),
        ("class $C { $$$ }", "ts"),
        ("<$T>", "tsx"),
        ("$K: $V", "ts"),
        ("function ???", "ts"),
        ("function $N($$$) { $$$ }", "zzz"),
    ] {
        assert_eq!(annotated(pattern, ext), None, "{ext}: {pattern}");
    }
}
