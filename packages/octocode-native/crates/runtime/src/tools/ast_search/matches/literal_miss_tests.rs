//! An empty match names the literal tokens of its pattern that look like
//! metavariables (with a repair call when the rewrite is exact) and states how
//! each pattern parsed; a result with matches says neither.

use crate::tools::ast_search::test_rows::run;
use crate::tools::test_support::{Fixture, checks};
use serde_json::{Value, json};

const CALLS: &str =
    "fn f() {\n    search_files(a);\n    other(b);\n    search_text(c, d);\n    a.unwrap();\n}\n";

fn notes(out: &Value, code: &str) -> Vec<Value> {
    out["diagnostics"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| d["code"] == code)
        .cloned()
        .collect()
}

fn row(path: &std::path::Path, rule: Value) -> Value {
    json!({"operation":"match","path":path,"rule":rule})
}

/// The row of the related astSearch call whose rule `accepts`, checked
/// against the query contract.
fn repair_row(out: &Value, accepts: impl Fn(&Value) -> bool) -> Value {
    let call = checks::related_call(out, "astSearch", |input| {
        accepts(&input["queries"][0]["rule"])
    });
    let lead = call["input"]["queries"][0].clone();
    crate::contracts::validate_query("astSearch", lead.clone()).expect("a valid repair row");
    lead
}

fn rust_tree() -> (Fixture, std::path::PathBuf) {
    let root = Fixture::new();
    let dir = root.0.join("src");
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(dir.join("a.rs"), CALLS).expect("rust");
    (root, dir)
}

/// RFC validation 1: `search_$X` is one literal identifier. The row names it,
/// and the repair (a capture in the same place, of the same kind and
/// spelling) holds the calls.
#[test]
fn a_glued_sigil_is_named_and_its_repair_matches() {
    let (root, dir) = rust_tree();
    for path in [dir.join("a.rs"), dir.clone()] {
        let out = run(&root.0, row(&path, json!({"pattern":"search_$X($$$A)"}))).expect("empty");
        assert_eq!(out["status"], "empty", "{out}");
        let named = notes(&out, "structural.pattern.literalSigil");
        assert_eq!(named.len(), 1, "{out}");
        assert_eq!(named[0]["severity"], "warning", "{out}");
        assert_eq!(named[0]["stage"], "match", "{out}");
        assert!(
            named[0]["message"]
                .as_str()
                .is_some_and(|m| m.starts_with("`search_$X` is literal text")),
            "{out}"
        );
        assert!(
            named[0]["recovery"]
                .as_str()
                .is_some_and(|r| r.contains(r#"{"all":[{"pattern":"$X($$$A)"},{"has":{"field":"function","kind":"identifier","regex":"^search_.+$"}}]}"#)),
            "{out}"
        );
        let lead = repair_row(&out, |rule| rule["all"].is_array());
        assert_eq!(lead["rule"]["all"][0]["pattern"], "$X($$$A)", "{lead}");
        assert_eq!(
            lead["rule"]["all"][1]["has"]["regex"], "^search_.+$",
            "{lead}"
        );
        assert_eq!(run(&root.0, lead).expect("repair")["matchCount"], 2);
    }
}

/// RFC validation 2: `$x` keeps its `$` and names no capture; the repair
/// upper-cases it, nested in a relation too.
#[test]
fn a_lower_case_sigil_is_named_and_its_repair_upper_cases_it() {
    let (root, dir) = rust_tree();
    let file = dir.join("a.rs");
    let out = run(&root.0, row(&file, json!({"pattern":"$x.unwrap()"}))).expect("empty");
    let named = notes(&out, "structural.pattern.literalSigil");
    assert_eq!(named.len(), 1, "{out}");
    assert!(
        named[0]["message"]
            .as_str()
            .is_some_and(|m| m.starts_with("`$x`")),
        "{out}"
    );
    assert!(
        named[0]["recovery"]
            .as_str()
            .is_some_and(|r| r.contains("`$X`")),
        "{out}"
    );
    let lead = repair_row(&out, |rule| rule["pattern"] == "$X.unwrap()");
    assert_eq!(run(&root.0, lead).expect("repair")["matchCount"], 1);

    let nested = json!({"kind":"function_item","has":{"pattern":"$x.unwrap()","stopBy":"end"}});
    let out = run(&root.0, row(&file, nested)).expect("nested");
    assert_eq!(
        notes(&out, "structural.pattern.literalSigil").len(),
        1,
        "{out}"
    );
    let lead = repair_row(&out, |rule| rule["has"]["pattern"] == "$X.unwrap()");
    assert_eq!(lead["rule"]["kind"], "function_item", "{lead}");
    assert_eq!(run(&root.0, lead).expect("repair")["matchCount"], 1);
}

/// A glued token that does not lead the pattern has no exact rewrite: the
/// token is named, with no repair call.
#[test]
fn a_sigil_with_no_exact_rewrite_is_named_without_a_call() {
    let (root, dir) = rust_tree();
    let out = run(
        &root.0,
        row(&dir.join("a.rs"), json!({"pattern":"f(search_$X)"})),
    )
    .expect("empty");
    assert_eq!(
        notes(&out, "structural.pattern.literalSigil").len(),
        1,
        "{out}"
    );
    let rewritten = checks::find_related_call(&out, "astSearch", |input| {
        input["queries"][0]["rule"]["pattern"]
            .as_str()
            .is_some_and(|pattern| pattern != "f(search_$X)")
            || input["queries"][0]["rule"]["all"].is_array()
    });
    assert!(rewritten.is_none(), "{out}");
}

/// RFC validation 3: in JavaScript `$el` is a name the source spells; it
/// matches, and a result with matches names no sigil and states no shape.
#[test]
fn a_javascript_dollar_name_that_matches_says_nothing() {
    let root = Fixture::new();
    let file = root.0.join("a.js");
    std::fs::write(&file, "const $el = q();\n$el.on(x);\n").expect("js");
    let out = run(&root.0, row(&file, json!({"pattern":"$el.on($A)"}))).expect("match");
    assert_eq!(out["matchCount"], 1, "{out}");
    assert_eq!(
        notes(&out, "structural.pattern.literalSigil"),
        Vec::<Value>::new()
    );
    assert_eq!(notes(&out, "structural.pattern.shape"), Vec::<Value>::new());
}

/// RFC validation 4: an empty result states how each pattern parsed, nested
/// ones included; a result with matches does not.
#[test]
fn an_empty_result_states_each_pattern_shape() {
    let (root, dir) = rust_tree();
    let file = dir.join("a.rs");
    for path in [file.clone(), dir.clone()] {
        let out = run(&root.0, row(&path, json!({"pattern":"$X.unwrap($Y)"}))).expect("empty");
        assert_eq!(out["status"], "empty", "{out}");
        let shapes = notes(&out, "structural.pattern.shape");
        assert_eq!(shapes.len(), 1, "{out}");
        assert_eq!(shapes[0]["severity"], "info", "{out}");
        assert_eq!(
            shapes[0]["message"],
            r#"Pattern `$X.unwrap($Y)` parsed as (call_expression function:(field_expression value:$X field:(field_identifier "unwrap")) arguments:(arguments $Y))."#,
            "{out}"
        );
    }
    let nested = json!({"any":[
        {"pattern":"$X.expect($Y)"},
        {"inside":{"pattern":"loop { $$$B }","stopBy":"end"},"pattern":"$X.expect($Y)"}
    ]});
    let out = run(&root.0, row(&file, nested)).expect("nested");
    assert_eq!(out["status"], "empty", "{out}");
    let shapes = notes(&out, "structural.pattern.shape");
    assert_eq!(shapes.len(), 2, "one per distinct pattern: {out}");
    assert!(
        shapes.iter().any(|s| s["message"]
            .as_str()
            .is_some_and(|m| m.contains("loop_expression"))),
        "{out}"
    );

    let found = run(&root.0, row(&file, json!({"pattern":"$X.unwrap()"}))).expect("match");
    assert_eq!(found["matchCount"], 1, "{found}");
    assert_eq!(
        notes(&found, "structural.pattern.shape"),
        Vec::<Value>::new()
    );
    assert_eq!(
        notes(&found, "structural.pattern.literalSigil"),
        Vec::<Value>::new()
    );
}

/// A directory of two grammars with no `parser`: the empty result still
/// explains itself in each grammar the scan read. A finding every grammar
/// shares is stated once; one that differs is stated per grammar.
#[test]
fn a_mixed_grammar_directory_explains_its_empty_result_per_grammar() {
    let (root, dir) = rust_tree();
    std::fs::write(dir.join("b.ts"), "search_files(a);\nother(b);\n").expect("ts");
    let out = run(&root.0, row(&dir, json!({"pattern":"search_$X($$$A)"}))).expect("empty");
    assert_eq!(out["status"], "empty", "{out}");
    assert!(out.get("inferredLanguage").is_none(), "two grammars: {out}");
    let named = notes(&out, "structural.pattern.literalSigil");
    assert_eq!(named.len(), 1, "both grammars agree: {out}");
    assert!(
        named[0]["message"]
            .as_str()
            .is_some_and(|m| m.starts_with("`search_$X` is literal text")),
        "{out}"
    );
    let lead = repair_row(&out, |rule| rule["all"].is_array());
    assert!(lead.get("parser").is_none(), "{lead}");
    assert_eq!(run(&root.0, lead).expect("repair")["matchCount"], 3);

    let out = run(&root.0, row(&dir, json!({"pattern":"$X.unwrap($Y)"}))).expect("empty");
    assert_eq!(out["status"], "empty", "{out}");
    let mut shapes = notes(&out, "structural.pattern.shape")
        .into_iter()
        .map(|d| d["message"].as_str().expect("message").to_owned())
        .collect::<Vec<_>>();
    shapes.sort();
    assert_eq!(shapes.len(), 2, "the grammars read it differently: {out}");
    assert!(
        shapes[0].starts_with("In Rust: ") && shapes[0].contains("field_expression"),
        "{shapes:?}"
    );
    assert!(
        shapes[1].starts_with("In TypeScript: ") && shapes[1].contains("member_expression"),
        "{shapes:?}"
    );
}

/// A `{context, selector}` pattern is read for literal sigils too, and its
/// repair rewrites the context.
#[test]
fn a_context_pattern_names_its_literal_sigils() {
    let root = Fixture::new();
    let file = root.0.join("a.js");
    std::fs::write(&file, "function f() { a.on(1); }\n").expect("js");
    let pattern = json!({"context":"function f() { $x.on($A); }","selector":"call_expression"});
    let out = run(&root.0, row(&file, json!({"pattern": pattern}))).expect("empty");
    assert_eq!(out["status"], "empty", "{out}");
    let named = notes(&out, "structural.pattern.literalSigil");
    assert_eq!(named.len(), 1, "{out}");
    assert!(
        named[0]["message"]
            .as_str()
            .is_some_and(|m| m.starts_with("`$x`")),
        "{out}"
    );
    let lead = repair_row(&out, |rule| {
        rule["pattern"]["context"] == "function f() { $X.on($A); }"
    });
    assert_eq!(
        lead["rule"]["pattern"]["selector"], "call_expression",
        "{lead}"
    );
    assert_eq!(run(&root.0, lead).expect("repair")["matchCount"], 1);
}

/// The lines of an answer's match rows, file by file.
fn match_lines(out: &Value) -> Vec<u64> {
    out["files"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|file| file["matches"].as_array().cloned().unwrap_or_default())
        .filter_map(|row| row["line"].as_u64())
        .collect()
}

/// RV-AST F1: a `$el` the source spells is a real name, not a literal sigil.
/// The empty answer comes from a neighbor-only `has`, and the relation lead
/// still says so: the sigil note never stops the explainers.
#[test]
fn a_dollar_name_the_source_spells_leaves_the_relation_lead() {
    let root = Fixture::new();
    let file = root.0.join("a.js");
    std::fs::write(
        &file,
        "function f() {\n  if (ok) {\n    $el.on(x);\n  }\n}\n",
    )
    .expect("js");
    let rule = json!({"kind":"function_declaration","has":{"pattern":"$el.on($A)"}});
    let out = run(&root.0, row(&file, rule)).expect("empty");
    assert_eq!(out["status"], "empty", "{out}");
    assert_eq!(
        notes(&out, "structural.pattern.literalSigil"),
        Vec::<Value>::new(),
        "{out}"
    );
    assert_eq!(notes(&out, "structural.rule.neighbor").len(), 1, "{out}");
    let lead = repair_row(&out, |rule| rule["has"]["stopBy"] == "end");
    assert_eq!(lead["rule"]["has"]["pattern"], "$el.on($A)", "{lead}");
    assert_eq!(run(&root.0, lead).expect("lead")["matchCount"], 1);
}

/// RV-AST F1: a literal sigil under `not` cannot make a result empty (it
/// excludes nothing), so it is not named, and the relation lead stays.
#[test]
fn a_sigil_under_not_is_not_named_and_the_relation_lead_stays() {
    let root = Fixture::new();
    let file = root.0.join("a.rs");
    std::fs::write(
        &file,
        "fn f() {\n    if ok {\n        a.expect(1);\n    }\n}\n",
    )
    .expect("rs");
    let rule = json!({
        "kind":"function_item",
        "has":{"pattern":"$X.expect($Y)"},
        "not":{"pattern":"$y.unwrap()"}
    });
    let out = run(&root.0, row(&file, rule)).expect("empty");
    assert_eq!(out["status"], "empty", "{out}");
    assert_eq!(
        notes(&out, "structural.pattern.literalSigil"),
        Vec::<Value>::new(),
        "{out}"
    );
    assert_eq!(notes(&out, "structural.rule.neighbor").len(), 1, "{out}");
}

/// RV-AST F2: the glued repair matches exactly what the token meant: a call
/// whose callee is one identifier starting `search_` with more after it. A
/// path call, a method on a `search_` receiver and a bare `search_` are not
/// matches; a call returned by a call is not either (JavaScript), and Python
/// reads the same way (F5).
#[test]
fn a_glued_repair_matches_exactly_what_the_token_meant() {
    let root = Fixture::new();
    for (name, source, pattern, expected) in [
        (
            "a.rs",
            "fn f() {\n    search_files(a);\n    search_mod::run(b);\n    search_x.go(c);\n    research_y(d);\n    search_(e);\n}\n",
            "search_$X($$$A)",
            vec![2],
        ),
        (
            "b.js",
            "search_a(b)(c);\nsearch_x.go(c);\n",
            "search_$X($$$A)",
            vec![1],
        ),
        (
            "c.py",
            "search_files(a)\nsearch_mod.run(b)\nsearch_(e)\n",
            "search_$X($$$A)",
            vec![1],
        ),
    ] {
        let file = root.0.join(name);
        std::fs::write(&file, source).expect("source");
        let out = run(&root.0, row(&file, json!({"pattern": pattern}))).expect("empty");
        assert_eq!(out["status"], "empty", "{name}: {out}");
        assert_eq!(
            notes(&out, "structural.pattern.literalSigil").len(),
            1,
            "{name}: {out}"
        );
        let lead = repair_row(&out, |rule| {
            rule.get("all").is_some() || rule.get("regex").is_some()
        });
        let found = run(&root.0, lead.clone()).expect("repair");
        assert_eq!(match_lines(&found), expected, "{name}: {lead} → {found}");
    }
}

/// RV-AST F6: a token counts as a real name only where a parsed node's whole
/// text is the token. A comment, a doc comment, a string or a longer name
/// that spells it leaves the note (and its repair) in place; a Rust macro
/// body's `$x` is a metavariable of that macro, so it is not a real name.
#[test]
fn only_a_whole_node_spelling_the_token_makes_it_a_real_name() {
    let root = Fixture::new();
    for (name, source, pattern, named) in [
        (
            "c1.rs",
            format!("{CALLS}// try search_$X here\n"),
            "search_$X($$$A)",
            1,
        ),
        (
            "c2.rs",
            format!("{CALLS}const P: &str = \"search_$X($$$A)\";\n"),
            "search_$X($$$A)",
            1,
        ),
        (
            "c3.rs",
            format!("{CALLS}const Q: &str = \"search_$X\";\n"),
            "search_$X($$$A)",
            1,
        ),
        (
            "c4.rs",
            format!("{CALLS}/// e.g. `$x.unwrap()`\nfn g() {{}}\n"),
            "$x.unwrap()",
            1,
        ),
        (
            "c5.js",
            "function f() {\n  a.unwrap();\n  $xs.go();\n}\n".to_owned(),
            "$x.unwrap()",
            1,
        ),
        (
            "c6.rs",
            "macro_rules! m { ($x:expr) => { $x }; }\nfn f() { a.unwrap(); }\n".to_owned(),
            "$x.unwrap()",
            1,
        ),
    ] {
        let file = root.0.join(name);
        std::fs::write(&file, source).expect("source");
        let out = run(&root.0, row(&file, json!({"pattern": pattern}))).expect("empty");
        assert_eq!(out["status"], "empty", "{name}: {out}");
        assert_eq!(
            notes(&out, "structural.pattern.literalSigil").len(),
            named,
            "{name}: {out}"
        );
    }
}

/// RV-AST F7: the sigil note sits beside the explainers, not in place of
/// them: a lower-case sigil in one `any` branch and a real branch under a
/// neighbor-only `has` get both the note and the relation lead.
#[test]
fn the_sigil_note_sits_beside_the_relation_lead() {
    let root = Fixture::new();
    let file = root.0.join("b.rs");
    std::fs::write(&file, "fn f() {\n    if ok {\n        a.on(x);\n    }\n}\n").expect("rs");
    let rule = json!({"kind":"function_item","has":{"any":[{"pattern":"$x.foo()"},{"pattern":"$X.on($A)"}]}});
    let out = run(&root.0, row(&file, rule)).expect("empty");
    assert_eq!(out["status"], "empty", "{out}");
    assert_eq!(
        notes(&out, "structural.pattern.literalSigil").len(),
        1,
        "{out}"
    );
    assert_eq!(notes(&out, "structural.rule.neighbor").len(), 1, "{out}");
}

/// H1: no output spells a grammar's metavariable placeholder: pattern text
/// that reaches a message reads as the caller wrote it (`Foo_$X`, not the
/// Rust expando).
#[test]
fn no_message_spells_the_metavariable_placeholder() {
    let root = Fixture::new();
    let file = root.0.join("s.rs");
    std::fs::write(&file, "fn f() {\n    let x = Foo_bar { a: 1 };\n}\n").expect("rs");
    let out = run(&root.0, row(&file, json!({"pattern":"Foo_$X { a: $V }"}))).expect("empty");
    assert_eq!(out["status"], "empty", "{out}");
    let text = out.to_string();
    assert!(
        !text.contains('\u{b5}') && !text.contains('\u{10000}'),
        "{text}"
    );
    let exact = notes(&out, "structural.pattern.exact");
    assert!(
        exact.iter().any(|note| note.to_string().contains("Foo_$X")),
        "{out}"
    );
}

/// B240 N1: a macro's `$x` is that macro's metavariable, not a name a pattern
/// could match, so a macro in another file of the scan leaves the note for a
/// pattern whose `$x` is literal; a JavaScript `$x` identifier still counts.
#[test]
fn a_macro_metavariable_in_the_scan_does_not_silence_the_note() {
    let root = Fixture::new();
    std::fs::write(
        root.0.join("macro.rs"),
        "macro_rules! m { ($x:expr) => { $x }; }\n",
    )
    .expect("macro");
    std::fs::write(root.0.join("calls.rs"), "fn f() { a.unwrap(); }\n").expect("calls");
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"rule":{"pattern":"$x.unwrap()"}}),
    )
    .expect("empty");
    assert_eq!(out["status"], "empty", "{out}");
    assert_eq!(
        notes(&out, "structural.pattern.literalSigil").len(),
        1,
        "{out}"
    );
}
