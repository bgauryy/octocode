//! The longer spelling of a `::`-qualified pattern ([`pattern_qualifier`]):
//! which patterns have one, how it is spelled, and what it matches.

use super::*;

fn broader(pattern: &str, ext: &str) -> Option<String> {
    pattern_qualifier(pattern, ext).map(|qualified| qualified.broader)
}

fn match_lines(src: &str, ext: &str, pattern: &str) -> Vec<usize> {
    search(src, ext, Some(pattern), None)
        .expect("pattern search should succeed")
        .iter()
        .map(|found| found.start_line as usize)
        .collect()
}

/// A Rust call on a path, or a bare path, names its leftmost literal
/// segment, and the longer spelling puts one capture before it.
#[test]
fn a_rust_path_names_its_head_and_a_longer_spelling() {
    let call = pattern_qualifier("Mutex::new($X)", "rs").expect("a qualified call");
    assert_eq!(call.head, "Mutex");
    assert_eq!(call.broader, "$QUALIFIER::Mutex::new($X)");
    assert_eq!(
        broader("std::sync::Mutex::new($$$A)", "rs").as_deref(),
        Some("$QUALIFIER::std::sync::Mutex::new($$$A)")
    );
    assert_eq!(
        broader("Ordering::SeqCst", "rs").as_deref(),
        Some("$QUALIFIER::Ordering::SeqCst")
    );
    // The spelling keeps the caller's own text around the root.
    assert_eq!(
        broader("  Mutex::new( $X )  ", "rs").as_deref(),
        Some("  $QUALIFIER::Mutex::new( $X )  ")
    );
}

/// The longer spelling matches exactly the sites that spell a longer path
/// before the head (any depth), and none the pattern itself matches.
#[test]
fn the_longer_rust_spelling_matches_only_qualified_sites() {
    let src = "fn f() {\n    let a = Mutex::new(None);\n    let b = std::sync::Mutex::new(None);\n    let c = sync::Mutex::new(1);\n    let d = StdMutex::new(2);\n    let e = HashMap::new();\n}\n";
    assert_eq!(match_lines(src, "rs", "Mutex::new($X)"), vec![2]);
    let broader = broader("Mutex::new($X)", "rs").expect("broader");
    assert_eq!(match_lines(src, "rs", &broader), vec![3, 4]);
}

/// C++ and CUDA qualify with `::` too; the head is the leftmost scope.
#[test]
fn a_cpp_or_cuda_path_names_its_head_and_a_longer_spelling() {
    for ext in ["cpp", "cu"] {
        let qualified = pattern_qualifier("chrono::seconds($X)", ext).expect(ext);
        assert_eq!(qualified.head, "chrono", "{ext}");
        assert_eq!(
            qualified.broader, "$QUALIFIER::chrono::seconds($X)",
            "{ext}"
        );
    }
    let src =
        "void f() {\n  auto a = chrono::seconds(1);\n  auto b = std::chrono::seconds(2);\n}\n";
    assert_eq!(match_lines(src, "cpp", "chrono::seconds($X)"), vec![2]);
    let broader = broader("chrono::seconds($X)", "cpp").expect("broader");
    assert_eq!(match_lines(src, "cpp", &broader), vec![3]);
}

/// No longer spelling when the head is a capture or a path keyword, the
/// root is not a path, or the pattern is not a pattern of the grammar.
#[test]
fn patterns_without_a_literal_qualified_head_have_no_longer_spelling() {
    for pattern in [
        "$P::new($X)",
        "new($X)",
        "fn $N() {}",
        "$X.unwrap()",
        "crate::run($X)",
        "self::run($X)",
        "super::run($X)",
        "Mutex::new(",
    ] {
        assert_eq!(broader(pattern, "rs"), None, "{pattern}");
    }
    assert_eq!(broader("$Q::seconds($X)", "cpp"), None);
    assert_eq!(broader("<$T>", "tsx"), None);
}

/// `.`-separated member paths are not namespace qualifications: those
/// grammars get no longer spelling, even where `::` parses.
#[test]
fn member_path_grammars_have_no_longer_spelling() {
    for (pattern, ext) in [
        ("path.join($$$A)", "js"),
        ("path.join($$$A)", "py"),
        ("errors.New($X)", "go"),
        ("Collections.sort($X)", "java"),
        ("Console.WriteLine($X)", "cs"),
        ("global::System.Console.WriteLine($X)", "cs"),
    ] {
        assert_eq!(broader(pattern, ext), None, "{ext}: {pattern}");
    }
}

/// The added capture never reuses a name the pattern already binds.
#[test]
fn the_added_capture_takes_a_free_name() {
    assert_eq!(
        broader("Mutex::new($QUALIFIER)", "rs").as_deref(),
        Some("$QUALIFIER2::Mutex::new($QUALIFIER)")
    );
    assert_eq!(
        broader("Mutex::new($QUALIFIER, $QUALIFIER2)", "rs").as_deref(),
        Some("$QUALIFIER3::Mutex::new($QUALIFIER, $QUALIFIER2)")
    );
}
