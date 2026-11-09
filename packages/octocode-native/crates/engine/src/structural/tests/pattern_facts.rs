//! Facts a pattern states by itself, with no scan: the literal tokens that
//! spell a metavariable sigil ([`literal_sigils`], [`sigil_repair`]) and how
//! it parsed ([`pattern_shape`]).

use super::*;

fn tokens(pattern: &str, ext: &str) -> Vec<(String, SigilForm)> {
    context_tokens(WrittenPattern::Text(pattern), ext)
}

fn context_tokens(pattern: WrittenPattern<'_>, ext: &str) -> Vec<(String, SigilForm)> {
    literal_sigils(pattern, ext)
        .into_iter()
        .map(|sigil| (sigil.token, sigil.form))
        .collect()
}

fn lines(src: &str, ext: &str, rule: serde_json::Value) -> Vec<u32> {
    search(src, ext, None, Some(&rule))
        .expect("rule search")
        .iter()
        .map(|found| found.start_line)
        .collect()
}

const CALLS: &str = "fn f() {\n    search_files(a);\n    other(b);\n    search_text(c, d);\n}\n";

/// A sigil glued after other identifier characters is one literal
/// identifier. Its exact rewrite is a capture in the same place, which holds
/// a node of the token's kind spelled as the token (each capture one or more
/// characters): the calls it meant, and no path call, method on a `search_`
/// receiver or bare `search_` (RV-AST F2).
#[test]
fn a_glued_token_is_literal_and_its_repair_holds_exactly_its_nodes() {
    let pattern = "search_$X($$$A)";
    assert_eq!(
        tokens(pattern, "rs"),
        [("search_$X".to_owned(), SigilForm::Glued)]
    );
    assert!(lines(CALLS, "rs", serde_json::json!({"pattern": pattern})).is_empty());
    let repair = sigil_repair(WrittenPattern::Text(pattern), "rs").expect("an exact repair");
    assert_eq!(repair.pattern, "$X($$$A)");
    assert_eq!(
        repair.constraints,
        [serde_json::json!({"has":{"field":"function","kind":"identifier","regex":"^search_.+$"}})]
    );
    let rule = |repair: &SigilRepair| {
        let mut parts = vec![serde_json::json!({"pattern": repair.pattern})];
        parts.extend(repair.constraints.iter().cloned());
        serde_json::json!({"all": parts})
    };
    assert_eq!(lines(CALLS, "rs", rule(&repair)), [2, 4]);
    let near = "fn f() {\n    search_files(a);\n    search_mod::run(b);\n    search_x.go(c);\n    research_y(d);\n    search_(e);\n}\n";
    assert_eq!(lines(near, "rs", rule(&repair)), [2]);
    // A capture with literal text after it, and one whose name runs into it.
    let inner = sigil_repair(WrittenPattern::Text("get_$K_val()"), "rs");
    assert_eq!(
        inner, None,
        "`$K_` runs into `val`: two readings, no exact one"
    );
    let suffix = sigil_repair(WrittenPattern::Text("get_$Kval()"), "rs").expect("exact");
    assert_eq!(suffix.pattern, "$K()");
    assert_eq!(
        suffix.constraints[0]["has"]["regex"],
        serde_json::json!("^get_.+val$")
    );
}

/// A lower-case name keeps its `$` (or becomes `µµµargs` after `$$$`), which
/// no capture name spells: the repair upper-cases each whole token.
#[test]
fn a_lower_case_name_is_literal_and_its_repair_upper_cases_it() {
    assert_eq!(
        tokens("$x.unwrap()", "rs"),
        [("$x".to_owned(), SigilForm::LowerCase)]
    );
    let repair = sigil_repair(WrittenPattern::Text("$x.unwrap()"), "rs").expect("an exact repair");
    assert_eq!(repair.pattern, "$X.unwrap()");
    assert!(repair.constraints.is_empty());
    assert_eq!(
        tokens("foo($$$args)", "rs"),
        [("$$$args".to_owned(), SigilForm::LowerCase)]
    );
    assert_eq!(
        sigil_repair(WrittenPattern::Text("foo($$$args)"), "rs").map(|repair| repair.pattern),
        Some("foo($$$ARGS)".to_owned())
    );
    // Every whole occurrence, and no longer name that starts with it.
    assert_eq!(
        sigil_repair(WrittenPattern::Text("$x.f($xy, $x)"), "js").map(|repair| repair.pattern),
        Some("$X.f($XY, $X)".to_owned())
    );
}

/// A rewrite that would still spell a literal sigil, or a glued token that
/// does not lead the pattern, has no exact repair; the token is still named.
#[test]
fn a_sigil_with_no_exact_rewrite_has_no_repair() {
    assert_eq!(
        tokens("foo(search_$X)", "rs"),
        [("search_$X".to_owned(), SigilForm::Glued)]
    );
    assert_eq!(
        sigil_repair(WrittenPattern::Text("foo(search_$X)"), "rs"),
        None
    );
    // Inside a string, upper case is still not a whole node.
    assert_eq!(
        tokens(r#"log("cost $x")"#, "js"),
        [("$x".to_owned(), SigilForm::LowerCase)]
    );
    assert_eq!(
        sigil_repair(WrittenPattern::Text(r#"log("cost $x")"#), "js"),
        None
    );
}

/// Metavariables are no literal sigils, and a `$` before no name is none.
#[test]
fn metavariables_spell_no_literal_sigil() {
    for (pattern, ext) in [
        ("$X.unwrap($Y)", "rs"),
        ("$F($$$A)", "rs"),
        ("$_ == $$$", "js"),
        (r#"log("$")"#, "js"),
        ("struct $S;", "rs"),
    ] {
        assert_eq!(tokens(pattern, ext), [], "{pattern}");
        assert_eq!(
            sigil_repair(WrittenPattern::Text(pattern), ext),
            None,
            "{pattern}"
        );
    }
}

/// In JavaScript `$` is an identifier character: `$el` is a literal name the
/// matcher finds where the source spells it.
#[test]
fn a_javascript_dollar_name_is_literal_and_matches_its_spelling() {
    assert_eq!(
        tokens("$el.on($A)", "js"),
        [("$el".to_owned(), SigilForm::LowerCase)]
    );
    let src = "const $el = q();\n$el.on(x);\nother.on(y);\n";
    assert_eq!(
        lines(src, "js", serde_json::json!({"pattern": "$el.on($A)"})),
        [2]
    );
}

/// The shape names each named kind, field and metavariable, in one line.
#[test]
fn a_pattern_shape_is_its_named_kinds_fields_and_metavariables() {
    assert_eq!(
        pattern_shape(WrittenPattern::Text("$X.unwrap($Y)"), "rs").as_deref(),
        Some(
            r#"(call_expression function:(field_expression value:$X field:(field_identifier "unwrap")) arguments:(arguments $Y))"#
        )
    );
    // A glued token reads as the caller spelled it; captures of every form.
    assert_eq!(
        pattern_shape(WrittenPattern::Text("search_$X($$$A, $_)"), "rs").as_deref(),
        Some(
            r#"(call_expression function:(identifier "search_$X") arguments:(arguments $$$A $_))"#
        )
    );
    // An operator a field names is part of the shape.
    let shape = pattern_shape(WrittenPattern::Text("$A == $B"), "js").expect("shape");
    assert!(shape.contains(r#"operator:"==""#), "{shape}");
    assert!(!shape.contains('\n'), "{shape}");
    assert_eq!(
        pattern_shape(
            WrittenPattern::Context {
                context: "class A { $F() {} }",
                selector: "method_definition",
            },
            "js"
        )
        .as_deref(),
        Some(
            r#"(method_definition name:$F parameters:(formal_parameters) body:(statement_block))"#
        )
    );
    assert_eq!(pattern_shape(WrittenPattern::Text("fn ("), "rs"), None);
}

/// A `{context, selector}` pattern is read whole: a literal sigil in the
/// selected node or in its context is named, and the repair rewrites the
/// context.
#[test]
fn a_context_pattern_names_its_literal_sigils() {
    let selected = WrittenPattern::Context {
        context: "function f() { $x.on($A); }",
        selector: "call_expression",
    };
    assert_eq!(
        context_tokens(selected, "js"),
        [("$x".to_owned(), SigilForm::LowerCase)]
    );
    assert_eq!(
        sigil_repair(selected, "js").map(|repair| repair.pattern),
        Some("function f() { $X.on($A); }".to_owned())
    );
    let around = WrittenPattern::Context {
        context: "function $name() { $F($A); }",
        selector: "call_expression",
    };
    assert_eq!(
        context_tokens(around, "js"),
        [("$name".to_owned(), SigilForm::LowerCase)]
    );
    let plain = WrittenPattern::Context {
        context: "function $N() { $F($A); }",
        selector: "call_expression",
    };
    assert_eq!(context_tokens(plain, "js"), []);
}

/// RV-AST F5: the facts hold outside Rust and JavaScript: a glued token in
/// Python and Go, and a lower-case `$$$args` in Python.
#[test]
fn literal_sigils_hold_in_python_and_go() {
    for ext in ["py", "go"] {
        assert_eq!(
            tokens("search_$X($$$A)", ext),
            [("search_$X".to_owned(), SigilForm::Glued)],
            "{ext}"
        );
    }
    assert_eq!(
        tokens("foo($$$args)", "py"),
        [("$$$args".to_owned(), SigilForm::LowerCase)]
    );
    assert_eq!(
        sigil_repair(WrittenPattern::Text("foo($$$args)"), "py").map(|repair| repair.pattern),
        Some("foo($$$ARGS)".to_owned())
    );
    assert_eq!(tokens("$F($$$A)", "py"), []);
}

/// H1: a declaration's literal name reads as the caller wrote it, never with
/// the grammar's metavariable placeholder.
#[test]
fn a_declaration_name_reads_as_written() {
    let declaration = pattern_declaration("Foo_$X { a: $V }", "rs").expect("a declaration");
    assert_eq!(
        declaration.name,
        Some(("type_identifier".to_owned(), "Foo_$X".to_owned()))
    );
}
