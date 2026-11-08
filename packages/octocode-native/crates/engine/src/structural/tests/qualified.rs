//! The longer spellings of a pattern ([`pattern_qualifier`]): which patterns
//! have them, how they are spelled, and what they match.

use super::*;

/// The spelling that qualifies every name of the pattern.
fn broader(pattern: &str, ext: &str) -> Option<String> {
    pattern_qualifier(pattern, ext)
        .map(|qualified| qualified.spelling(&(0..qualified.names().len()).collect::<Vec<_>>()))
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
    assert_eq!(call.names(), ["Mutex"]);
    assert_eq!(call.spelling(&[0]), "$QUALIFIER::Mutex::new($X)");
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
        assert_eq!(qualified.names(), ["chrono"], "{ext}");
        assert_eq!(
            qualified.spelling(&[0]),
            "$QUALIFIER::chrono::seconds($X)",
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
    assert_eq!(broader("$F($X)", "rs"), None);
    assert_eq!(broader("<$T>", "tsx"), None);
}

/// A call on a bare name is spelled as written, so a call that qualifies the
/// name (`crate::security::scan::redact_lines(..)`) is left out. The longer
/// spelling puts one capture and the grammar's own separator before the name.
#[test]
fn a_bare_callee_names_itself_and_a_longer_spelling() {
    let call = pattern_qualifier("redact_lines($$$A)", "rs").expect("a bare call");
    assert_eq!(call.names(), ["redact_lines"]);
    assert_eq!(call.spelling(&[0]), "$QUALIFIER::redact_lines($$$A)");
    assert_eq!(
        broader("  run( $X )  ", "rs").as_deref(),
        Some("  $QUALIFIER::run( $X )  ")
    );
    assert_eq!(
        broader("seconds($X)", "cpp").as_deref(),
        Some("$QUALIFIER::seconds($X)")
    );
    for (ext, spelled) in [
        ("ts", "$QUALIFIER.format($$$A)"),
        ("tsx", "$QUALIFIER.format($$$A)"),
        ("js", "$QUALIFIER.format($$$A)"),
        ("py", "$QUALIFIER.format($$$A)"),
        ("go", "$QUALIFIER.format($$$A)"),
        ("cs", "$QUALIFIER.format($$$A)"),
    ] {
        let qualified = pattern_qualifier("format($$$A)", ext).expect(ext);
        assert_eq!(qualified.names(), ["format"], "{ext}");
        assert_eq!(qualified.spelling(&[0]), spelled, "{ext}");
    }
}

/// The longer spelling of a bare callee matches the calls that qualify the
/// name (any depth, any path keyword), and none the pattern itself matches,
/// neither a method of that name in a `::` grammar nor another name.
#[test]
fn the_longer_bare_spelling_matches_only_qualified_calls() {
    let rust = "fn f() {\n    run(1);\n    crate::a::run(2);\n    scan::run(3);\n    x.run(4);\n    runner(5);\n    let g = crate::a::run;\n}\n";
    assert_eq!(match_lines(rust, "rs", "run($X)"), vec![2]);
    let longer = broader("run($X)", "rs").expect("broader");
    assert_eq!(match_lines(rust, "rs", &longer), vec![3, 4]);
    let ts = "function f() {\n  run(1);\n  ns.run(2);\n  this.a.run(3);\n  runner(4);\n}\n";
    assert_eq!(match_lines(ts, "ts", "run($X)"), vec![2]);
    let longer = broader("run($X)", "ts").expect("broader");
    assert_eq!(match_lines(ts, "ts", &longer), vec![3, 4]);
}

/// A call with no `function` to qualify (a Java invocation), a bare name that
/// is not a call, and a callee that is not a plain name get none.
#[test]
fn what_is_not_a_bare_call_has_no_longer_bare_spelling() {
    assert_eq!(broader("format($X)", "java"), None);
    assert_eq!(broader("Ordering", "rs"), None);
    assert_eq!(broader("format", "ts"), None);
    assert_eq!(broader("this.format($X)", "ts"), None);
    assert_eq!(broader("$F($X)", "ts"), None);
}

/// `.`-separated member paths are not namespace qualifications: those
/// grammars get no longer spelling for a path pattern, even where `::` parses.
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
    // A longer capture name that starts with the free one does not take it.
    assert_eq!(
        broader("Mutex::new($QUALIFIERS)", "rs").as_deref(),
        Some("$QUALIFIER::Mutex::new($QUALIFIERS)")
    );
}

/// A name inside a larger pattern (AST-1: the trait of an `impl`, a
/// parameter's type) is matched as written too, so it has longer spellings;
/// each name is spelled longer on its own or together with the others.
#[test]
fn names_inside_a_larger_rust_pattern_have_longer_spellings() {
    let trait_impl = pattern_qualifier("impl ToolOutput for $T {}", "rs").expect("impl");
    assert_eq!(trait_impl.names(), ["ToolOutput"]);
    assert_eq!(trait_impl.skeleton(), "impl $QUALIFIER for $T {}");
    assert_eq!(
        trait_impl.spelling(&[0]),
        "impl $QUALIFIER::ToolOutput for $T {}"
    );
    let both = pattern_qualifier("impl Display for Foo {}", "rs").expect("impl");
    assert_eq!(both.names(), ["Display", "Foo"]);
    assert_eq!(both.skeleton(), "impl $QUALIFIER for $QUALIFIER2 {}");
    assert_eq!(both.spelling(&[1]), "impl Display for $QUALIFIER::Foo {}");
    assert_eq!(
        both.spelling(&[0, 1]),
        "impl $QUALIFIER::Display for $QUALIFIER2::Foo {}"
    );
    let typed = pattern_qualifier("fn $N(c: Config) {}", "rs").expect("fn");
    assert!(typed.names().contains(&"Config"), "{typed:?}");
}

/// Each spelling matches exactly the sites that qualify its names, and the
/// skeleton's captures tell which names a site qualifies: none of another
/// name that only ends like one of the pattern's.
#[test]
fn the_skeleton_tells_which_names_a_site_qualifies() {
    let src = "impl Display for Foo {}\nimpl fmt::Display for Foo {}\nimpl Display for crate::m::Foo {}\nimpl std::fmt::Display for m::Foo {}\nimpl MyDisplay for Foo {}\nimpl Display for Bar {}\n";
    let qualified = pattern_qualifier("impl Display for Foo {}", "rs").expect("impl");
    assert_eq!(match_lines(src, "rs", "impl Display for Foo {}"), vec![1]);
    assert_eq!(match_lines(src, "rs", &qualified.spelling(&[0])), vec![2]);
    assert_eq!(match_lines(src, "rs", &qualified.spelling(&[1])), vec![3]);
    assert_eq!(
        match_lines(src, "rs", &qualified.spelling(&[0, 1])),
        vec![4]
    );
    let read: Vec<(usize, Option<Vec<usize>>)> =
        search(src, "rs", Some(qualified.skeleton()), None)
            .expect("skeleton search")
            .iter()
            .map(|found| {
                (
                    found.start_line as usize,
                    qualified.qualified_names(&found.metavars),
                )
            })
            .collect();
    assert_eq!(
        read,
        vec![
            (1, None),
            (2, Some(vec![0])),
            (3, Some(vec![1])),
            (4, Some(vec![0, 1])),
            (5, None),
            (6, None),
        ]
    );
}

/// B88: a pattern compilation terminates for context (`impl X for $T` parses
/// as `impl X for $T;`) keeps its longer spellings, spelled as written, and
/// they match the bodied implementations that qualify the trait.
#[test]
fn a_bodyless_impl_has_longer_spellings_as_written() {
    let qualified = pattern_qualifier("impl ToolOutput for $T", "rs").expect("impl");
    assert_eq!(qualified.names(), ["ToolOutput"]);
    assert_eq!(qualified.skeleton(), "impl $QUALIFIER for $T");
    let longer = qualified.spelling(&[0]);
    assert_eq!(longer, "impl $QUALIFIER::ToolOutput for $T");
    let src =
        "impl ToolOutput for A { fn f() {} }\nimpl crate::x::ToolOutput for B { fn f() {} }\n";
    assert_eq!(match_lines(src, "rs", &longer), vec![2]);
}

/// B88: a type name in an annotation of a `.` grammar is qualified by a
/// namespace or module, never by a receiver (a type is no value), so it has
/// a longer spelling wherever the grammar reads `$Q.Bar` as a type in the
/// same place: a variable's annotation, a parameter's, a return type's.
#[test]
fn a_type_name_in_a_member_grammar_annotation_has_a_longer_spelling() {
    let annotated = pattern_qualifier("const $V: Bar = $E", "ts").expect("annotation");
    assert_eq!(annotated.names(), ["Bar"]);
    assert_eq!(annotated.skeleton(), "const $V: $QUALIFIER = $E");
    assert_eq!(annotated.spelling(&[0]), "const $V: $QUALIFIER.Bar = $E");
    let src = "const a: Bar = make();\nconst b: ns.Bar = make();\nconst c: a.b.Bar = make();\nconst d: MyBar = make();\nconst e: ns.MyBar = make();\n";
    assert_eq!(match_lines(src, "ts", "const $V: Bar = $E"), vec![1]);
    assert_eq!(
        match_lines(src, "ts", &annotated.spelling(&[0])),
        vec![2, 3]
    );
    assert_eq!(
        broader("function $F($A: Bar): Baz { $$$ }", "ts").as_deref(),
        Some("function $F($A: $QUALIFIER.Bar): $QUALIFIER2.Baz { $$$ }")
    );
    // A declared type name is no reference to a type: nothing qualifies it.
    assert_eq!(broader("class Bar {}", "ts"), None);
    assert_eq!(broader("interface Bar {}", "ts"), None);
}

/// Only a source that spells a name after the separator can hold a longer
/// spelling; a longer name that ends like it is none.
#[test]
fn only_a_source_with_a_separated_name_may_qualify_it() {
    let qualified = pattern_qualifier("impl ToolOutput for $T {}", "rs").expect("impl");
    assert!(qualified.may_qualify("impl crate::x::ToolOutput for B {}"));
    assert!(qualified.may_qualify("impl x ::\n  ToolOutput for B {}"));
    assert!(!qualified.may_qualify("impl ToolOutput for A {}"));
    assert!(!qualified.may_qualify("impl x::ToolOutputs for A {}"));
    assert!(!qualified.may_qualify("impl x::MyToolOutput for A {}"));
    let call = pattern_qualifier("format($$$A)", "ts").expect("ts");
    assert!(call.may_qualify("ns.format(1)"));
    assert!(!call.may_qualify("format(1)"));
}
