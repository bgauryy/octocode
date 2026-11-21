//! A literal name anywhere in a pattern is matched as written, so the
//! sites that spell a path before it are left out (AST-1: `impl ToolOutput
//! for $T {}` against `impl crate::x::ToolOutput for B {}`). The rows stay
//! as written; the first page counts the longer spellings and leads to the
//! rule that lists every spelling it found.
use crate::ast_search::test_rows::run;
use octocode_runtime_base::test_support::{Fixture, checks};
use octocode_test_support::json_fixture;
use serde_json::{Value, json};

const CODE: &str = "structural.pattern.qualified";

/// The AST-1 fixture (`plucky-seahorse/fx/ref.rs`): one bare and one
/// qualified implementation of the same trait.
const REF_RS: &str = "pub trait ToolOutput {}\nstruct A;\nimpl ToolOutput for A {}\nstruct B;\nimpl crate::x::ToolOutput for B {}\n";

/// The qualified notes of a result: its diagnostics beside matches, and on
/// an empty page the hint that says the same (as `{message}`).
fn qualified_notes(out: &Value) -> Vec<Value> {
    out["diagnostics"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|diagnostic| diagnostic["code"] == CODE)
        .cloned()
        .chain(
            qualified_hints(out)
                .into_iter()
                .map(|hint| json!({"message": hint})),
        )
        .collect()
}

fn qualified_hints(out: &Value) -> Vec<&str> {
    out["hints"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|hint| hint.starts_with("The pattern matches the path as written"))
        .collect()
}

/// The broadenSearch lead's rows.
fn lead_rows(out: &Value) -> Vec<Value> {
    checks::find_related_call(out, "astSearch", |input| {
        input["queries"][0]["rule"].get("any").is_some()
    })
    .and_then(|call| call["input"]["queries"].as_array().cloned())
    .unwrap_or_default()
}

fn matched(root: &std::path::Path, path: &std::path::Path, pattern: &str) -> Value {
    run(
        root,
        json!({"operation":"match","path":path,"rule":{"pattern":pattern}}),
    )
    .expect("match")
}

/// The lead's rows run, with their match counts summed.
fn lead_count(root: &std::path::Path, rows: &[Value]) -> u64 {
    rows.iter()
        .map(|row| {
            octocode_runtime_base::contracts::validate_query("astSearch", row.clone())
                .expect("a valid lead row");
            let listed = run(root, row.clone()).expect("lead runs");
            assert!(qualified_notes(&listed).is_empty(), "{listed}");
            listed["matchCount"].as_u64().expect("count")
        })
        .sum()
}

/// AST-1 on its fixture, over a directory and over the file itself: one row
/// as written, a note naming the qualified implementation, and a lead that
/// lists both.
#[test]
fn a_bare_trait_name_names_its_qualified_implementations() {
    let root = Fixture::new();
    root.write("ref.rs", REF_RS);
    for path in [root.0.clone(), root.0.join("ref.rs")] {
        let out = matched(&root.0, &path, "impl ToolOutput for $T {}");
        assert_eq!(out["matchCount"], 1, "{out}");
        let notes = qualified_notes(&out);
        assert_eq!(notes.len(), 1, "{out}");
        let message = notes[0]["message"].as_str().expect("message");
        // A file scope's note places the spelling by line: the file is the
        // request's own path.
        let first = if path.is_file() {
            "(first: line 5)"
        } else {
            "ref.rs:5)"
        };
        assert!(
            message.contains("1 more match(es) in 1 file(s)")
                && message.contains("`ToolOutput`")
                && message.contains(first),
            "{message}"
        );
        let rows = lead_rows(&out);
        assert_eq!(rows.len(), 1, "{out}");
        assert_eq!(
            rows[0]["rule"],
            json_fixture(
                r#"{
                    "any": [
                        {"pattern": "impl ToolOutput for $T {}"},
                        {"pattern": "impl $QUALIFIER::ToolOutput for $T {}"}
                    ]
                }"#
            ),
            "{out}"
        );
        assert_eq!(lead_count(&root.0, &rows), 2, "{out}");
    }
}

/// Two literal names in one pattern: each site may qualify either or both,
/// and the lead spells exactly the combinations the scope holds, so it lists
/// every implementation the note counts. A name that only ends like one of
/// the pattern's (`MyDisplay`) is no spelling of it.
#[test]
fn each_qualified_combination_found_is_counted_and_listed() {
    let root = Fixture::new();
    root.write(
        "a.rs",
        "struct Foo;\nimpl Display for Foo {}\nimpl fmt::Display for Foo {}\nimpl Display for crate::m::Foo {}\nimpl std::fmt::Display for m::Foo {}\nimpl MyDisplay for Foo {}\nimpl Display for Bar {}\n",
    );
    let out = matched(&root.0, &root.0, "impl Display for Foo {}");
    assert_eq!(out["matchCount"], 1, "{out}");
    let notes = qualified_notes(&out);
    assert_eq!(notes.len(), 1, "{out}");
    let message = notes[0]["message"].as_str().expect("message");
    assert!(
        message.contains("3 more match(es) in 1 file(s)")
            && message.contains("`Display`")
            && message.contains("`Foo`")
            && message.contains("a.rs:3"),
        "{message}"
    );
    let rows = lead_rows(&out);
    assert_eq!(rows.len(), 1, "{out}");
    assert_eq!(
        rows[0]["rule"],
        json_fixture(
            r#"{
                "any": [
                    {"pattern": "impl Display for Foo {}"},
                    {"pattern": "impl $QUALIFIER::Display for Foo {}"},
                    {"pattern": "impl Display for $QUALIFIER::Foo {}"},
                    {
                        "pattern": "impl $QUALIFIER::Display for $QUALIFIER2::Foo {}"
                    }
                ]
            }"#
        ),
        "{out}"
    );
    assert_eq!(lead_count(&root.0, &rows), 4, "{out}");
}

/// A type named inside a larger pattern (a parameter's type) is matched as
/// written too: the note names the qualified parameter types.
#[test]
fn a_nested_type_name_counts_its_qualified_spellings() {
    let root = Fixture::new();
    root.write(
        "a.rs",
        "fn a(c: Config) {}\nfn b(c: crate::config::Config) {}\nfn c(c: OtherConfig) {}\n",
    );
    let out = matched(&root.0, &root.0, "fn $N(c: Config) {}");
    assert_eq!(out["matchCount"], 1, "{out}");
    let notes = qualified_notes(&out);
    assert_eq!(notes.len(), 1, "{out}");
    let message = notes[0]["message"].as_str().expect("message");
    assert!(
        message.contains("1 more match(es) in 1 file(s)")
            && message.contains("`Config`")
            && message.contains("a.rs:2"),
        "{message}"
    );
    assert_eq!(lead_count(&root.0, &lead_rows(&out)), 2, "{out}");
}

/// The note counts exactly what its lead returns: a reference, pointer or
/// trait-object type around a qualified path is another type, not a longer
/// spelling of the parameter's type, so only `m::Foo` counts.
#[test]
fn only_sites_the_lead_returns_are_counted() {
    let root = Fixture::new();
    root.write(
        "a.rs",
        "fn u(a: &m::Foo) {}\nfn u3(a: *const m::Foo) {}\nfn u4(a: dyn m::Foo) {}\nfn u5(a: m::Foo) {}\nfn u6(a: Foo) {}\n",
    );
    let out = matched(&root.0, &root.0, "fn $N($A: Foo) {}");
    assert_eq!(out["matchCount"], 1, "{out}");
    let notes = qualified_notes(&out);
    assert_eq!(notes.len(), 1, "{out}");
    let message = notes[0]["message"].as_str().expect("message");
    assert!(
        message.contains("1 more match(es) in 1 file(s)") && message.contains("a.rs:4"),
        "{message}"
    );
    assert_eq!(lead_count(&root.0, &lead_rows(&out)), 2, "{out}");
}

/// A page without rows says both why the pattern as written matched none
/// and which longer spellings it left out, each with its own lead.
#[test]
fn an_empty_page_explains_its_miss_and_counts_the_longer_spellings() {
    let root = Fixture::new();
    root.write(
        "a.rs",
        "pub trait ToolOutput {}\nstruct A;\nstruct B;\nimpl crate::x::ToolOutput for B {}\nimpl Other for A { fn f() {} }\n",
    );
    let out = matched(&root.0, &root.0, "impl ToolOutput for $T {}");
    assert_eq!(out["status"], "empty", "{out}");
    let explained = out["diagnostics"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|diagnostic| diagnostic["code"] == "structural.pattern.exact");
    assert!(explained, "{out}");
    let exact_lead = checks::find_related_call(&out, "astSearch", |input| {
        input["queries"][0]["rule"].get("kind").is_some()
    });
    assert!(exact_lead.is_some(), "{out}");
    assert_eq!(qualified_notes(&out).len(), 1, "{out}");
    assert_eq!(lead_count(&root.0, &lead_rows(&out)), 1, "{out}");
}

/// A spelling run that did not finish leaves a file's longer spellings
/// unknown (`None`), never zero; a file without candidates needs no run.
#[test]
fn an_unfinished_spelling_run_leaves_the_count_unknown() {
    let pattern = "fn $N($A: Foo) {}";
    let qualified =
        octocode_engine::structural::pattern_qualifier(pattern, "rs").expect("a qualifier");
    let rule = json!({"pattern": pattern});
    let skeleton = |source: &str| {
        octocode_engine::structural::search_detailed(
            source,
            "a.rs",
            Some("rs"),
            Some(qualified.skeleton()),
            None,
        )
        .matches
    };
    let candidates = skeleton("fn u(a: m::Foo) {}\n");
    assert_eq!(
        super::longer_spellings((&rule, ""), &qualified, &candidates, &mut |_| None),
        None
    );
    let bare = skeleton("fn u(a: Foo) {}\n");
    let unrun = super::longer_spellings((&rule, ""), &qualified, &bare, &mut |_| {
        panic!("no candidate needs a spelling run")
    });
    assert_eq!(unrun, Some(Vec::new()));
}

/// A directory probe names each file it could not probe (here, one the
/// scan kept no source for) and keeps the other files' hits and count.
#[test]
fn a_file_without_a_source_is_unprobed_and_the_rest_still_count() {
    let root = Fixture::new();
    let text = "fn u(a: m::Foo) {}\nfn v(a: n::Foo) {}\n";
    root.write("a.rs", text);
    root.write("b.rs", text);
    let file = |name: &str, source| super::super::types::ScannedFile {
        path: name.to_owned(),
        parser: "rs".to_owned(),
        matches: octocode_engine::structural::FileMatches::Held(Vec::new()),
        diagnostics: Vec::new(),
        status: "ok".to_owned(),
        recovered: false,
        recovered_bodies: Vec::new(),
        source,
    };
    let digest = octocode_engine::digest::sha256(text.as_bytes());
    let scan = super::super::types::Scan {
        files: vec![
            file("a.rs", Some((root.0.join("a.rs"), digest))),
            file("b.rs", None),
        ],
        ..Default::default()
    };
    let scope = super::super::types::Scope {
        canonical: root.0,
        is_file: false,
        language: None,
        inferred: None,
        extensions: None,
        parsers: Default::default(),
        compiled: Vec::new(),
        inline: None,
    };
    let pattern = "fn $N($A: Foo) {}";
    let rule = json!({"pattern": pattern});
    let probe = super::directory_probe(
        &scope,
        &scan,
        &rule,
        &[(String::new(), pattern.to_owned())],
        &octocode_runtime_base::execution::cancel::NeverCancel,
    )
    .expect("a probe");
    assert_eq!(probe.unprobed, ["b.rs"]);
    assert_eq!(probe.count, 2);
    assert_eq!(probe.hits.get("a.rs"), Some(&1));
    assert_eq!(probe.hits.len(), 1);
}

/// The note keeps the count it has and says which files' longer spellings
/// are unknown; its lead takes the next numbered `broadenSearch` when an
/// empty page's own explanation holds the first. With no hit at all, the
/// note says only what is unknown and leads nowhere.
#[test]
fn the_note_states_unprobed_files_and_keeps_its_count() {
    let query: octocode_contract_types::AstSearchQueryMatch0 =
        serde_json::from_value(json_fixture(
            r#"{
                "operation": "match",
                "path": "/repo",
                "rule": {
                    "any": [{"pattern": "Foo"}, {"pattern": "$QUALIFIER::Foo"}]
                }
            }"#,
        ))
        .expect("a typed row");
    let miss = |first: Option<&str>, queries| super::QualifiedMiss {
        names: vec!["Foo".to_owned()],
        count: 2,
        floor: false,
        files: 1,
        first: first.map(str::to_owned),
        unprobed: Some((3, "b.rs".to_owned())),
        queries,
        view: None,
    };
    let explained = json_fixture(r#"{"kind": "related", "tool": "astSearch"}"#);
    let mut out = json!({"calls": {"broadenSearch": explained}});
    miss(Some("a.rs:4"), vec![query]).attach(&mut out);
    let notes = qualified_notes(&out);
    let message = notes[0]["message"].as_str().expect("message");
    assert!(
        message.contains("2 more match(es) in 1 file(s)")
            && message.contains("(first: a.rs:4)")
            && message.contains("in 3 file(s) are unknown")
            && message.contains("(first: b.rs)"),
        "{message}"
    );
    assert_eq!(out["calls"]["broadenSearch"], explained, "{out}");
    let second = octocode_runtime_base::id::numbered("broadenSearch", 1);
    assert!(
        out["calls"][&second]["input"]["queries"].is_array(),
        "{out}"
    );

    let mut out = json!({});
    miss(None, Vec::new()).attach(&mut out);
    let message = qualified_notes(&out)[0]["message"]
        .as_str()
        .expect("message")
        .to_owned();
    assert!(
        message.contains("in 3 file(s) are unknown") && !message.contains("more match"),
        "{message}"
    );
    assert!(out.get("calls").is_none(), "{out}");
}

/// B88: a pattern written without the body (`impl ToolOutput for $T`) leaves
/// the body out, so it lists the bare implementation and the note counts the
/// qualified one, as the bodied pattern does: never 0 rows without a note.
#[test]
fn a_bodyless_impl_matches_and_names_its_qualified_implementations() {
    let root = Fixture::new();
    root.write("ref.rs", REF_RS);
    let out = matched(&root.0, &root.0, "impl ToolOutput for $T");
    assert_eq!(out["matchCount"], 1, "{out}");
    let notes = qualified_notes(&out);
    assert_eq!(notes.len(), 1, "{out}");
    let message = notes[0]["message"].as_str().expect("message");
    assert!(message.contains("ref.rs:5"), "{message}");
    assert_eq!(lead_count(&root.0, &lead_rows(&out)), 2, "{out}");
}

/// B88: a TypeScript type annotation names its type as written, so a site
/// that qualifies it (`ns.Bar`) is left out; the note counts it and the lead
/// lists it, on a page with rows and on one without.
#[test]
fn a_typescript_annotation_names_its_qualified_types() {
    let root = Fixture::new();
    root.write(
        "a.ts",
        "const a: Bar = make();\nconst b: ns.Bar = make();\nconst c: MyBar = make();\n",
    );
    root.write("only/b.ts", "const b: ns.Bar = make();\n");
    let out = matched(&root.0, &root.0.join("a.ts"), "const $V: Bar = $E");
    assert_eq!(out["matchCount"], 1, "{out}");
    let notes = qualified_notes(&out);
    assert_eq!(notes.len(), 1, "{out}");
    let message = notes[0]["message"].as_str().expect("message");
    assert!(
        message.contains("1 more match(es) in 1 file(s)")
            && message.contains("`Bar`")
            && message.contains("(first: line 2)"),
        "{message}"
    );
    assert_eq!(lead_count(&root.0, &lead_rows(&out)), 2, "{out}");
    let empty = matched(&root.0, &root.0.join("only"), "const $V: Bar = $E");
    assert_eq!(empty["status"], "empty", "{empty}");
    assert_eq!(qualified_notes(&empty).len(), 1, "{empty}");
    assert_eq!(lead_count(&root.0, &lead_rows(&empty)), 1, "{empty}");
}

/// No qualified spelling in scope: no note and no lead.
#[test]
fn a_scope_without_qualified_spellings_adds_nothing() {
    let root = Fixture::new();
    root.write(
        "a.rs",
        "pub trait ToolOutput {}\nstruct A;\nimpl ToolOutput for A {}\nimpl MyToolOutput for A {}\n",
    );
    let out = matched(&root.0, &root.0, "impl ToolOutput for $T {}");
    assert_eq!(out["matchCount"], 1, "{out}");
    assert!(qualified_notes(&out).is_empty(), "{out}");
    assert!(lead_rows(&out).is_empty(), "{out}");
}

/// A bare call pattern matches a call of the bare name, so over code that
/// only calls the name on a receiver it is empty: the empty page says in one
/// hint how many such calls its files hold, and its lead lists them. The
/// same in each grammar, over a directory and over the file itself.
#[test]
fn an_empty_bare_call_names_the_calls_on_a_receiver() {
    for (file, source, first) in [
        (
            "calls.rs",
            "fn f(x: &S) {\n    x.sanitize_text(1);\n    self.inner.sanitize_text(2);\n    other(3);\n}\n",
            "calls.rs:2",
        ),
        (
            "calls.ts",
            "function f(x: S) {\n  x.sanitizeText(1);\n  this.inner.sanitizeText(2);\n  other(3);\n}\n",
            "calls.ts:2",
        ),
        (
            "calls.py",
            "def f(x):\n    x.sanitize_text(1)\n    x.inner.sanitize_text(2)\n    other(3)\n",
            "calls.py:2",
        ),
    ] {
        let name = if file.ends_with(".ts") {
            "sanitizeText"
        } else {
            "sanitize_text"
        };
        let root = Fixture::new();
        root.write(file, source);
        for path in [root.0.clone(), root.0.join(file)] {
            let out = matched(&root.0, &path, &format!("{name}($$$A)"));
            assert_eq!(out["status"], "empty", "{out}");
            assert!(
                out["diagnostics"].to_string().find(CODE).is_none(),
                "said once, as a hint: {out}"
            );
            let said = qualified_hints(&out);
            assert_eq!(said.len(), 1, "{out}");
            let first = if path.is_file() {
                "(first: line 2)"
            } else {
                first
            };
            assert!(
                said[0].contains("2 more match(es) in 1 file(s)")
                    && said[0].contains(&format!("`{name}`"))
                    && said[0].contains(first),
                "{}",
                said[0]
            );
            let rows = lead_rows(&out);
            assert_eq!(rows.len(), 1, "{out}");
            assert_eq!(lead_count(&root.0, &rows), 2, "{out}");
        }
    }
}

/// Beside matches the note stays a diagnostic: bare calls are listed, the
/// receiver calls are counted, and the lead lists both.
#[test]
fn a_matched_bare_call_counts_the_calls_on_a_receiver() {
    let root = Fixture::new();
    root.write(
        "calls.rs",
        "fn f(x: &S) {\n    run(0);\n    x.run(1);\n    crate::a::run(2);\n}\n",
    );
    let out = matched(&root.0, &root.0, "run($$$A)");
    assert_eq!(out["matchCount"], 1, "{out}");
    let notes = qualified_notes(&out);
    assert_eq!(notes.len(), 1, "{out}");
    let message = notes[0]["message"].as_str().expect("message");
    assert!(
        message.contains("2 more match(es) in 1 file(s)"),
        "{message}"
    );
    let rows = lead_rows(&out);
    assert_eq!(lead_count(&root.0, &rows), 3, "{out}");
}

fn rule_matched(root: &std::path::Path, rule: &Value) -> Value {
    run(root, json!({"operation":"match","path":root,"rule":rule})).expect("match")
}

/// D1: a bare call pattern nested in a relation misses its calls on a
/// receiver as the top-level one does: the empty page says so in the same
/// hint, and its lead rewrites that nested pattern in place, the rest of the
/// rule unchanged. The note counts exactly what the lead adds.
#[test]
fn a_nested_bare_call_names_the_calls_on_a_receiver() {
    let root = Fixture::new();
    root.write(
        "a.rs",
        "fn a(x: Option<u8>) -> u8 {\n    x.unwrap()\n}\nfn b(x: Option<u8>) -> u8 {\n    x.unwrap_or(0)\n}\nmod m {\n    fn c(x: Option<u8>) -> u8 {\n        x.unwrap()\n    }\n}\nfn e(s: &S) -> u8 {\n    s.inner.unwrap()\n}\n",
    );
    let has = json_fixture(r#"{"pattern": "unwrap()", "stopBy": "end"}"#);
    let rule = json!({
        "kind":"function_item",
        "has": has,
        "not":{"inside":{"kind":"mod_item","stopBy":"end"}}
    });
    let out = rule_matched(&root.0, &rule);
    assert_eq!(out["status"], "empty", "{out}");
    let said = qualified_hints(&out);
    assert_eq!(said.len(), 1, "{out}");
    assert!(
        said[0].contains("2 more match(es) in 1 file(s)")
            && said[0].contains("`unwrap`")
            && said[0].contains("a.rs:1"),
        "{}",
        said[0]
    );
    let rows = lead_rows_at(&out, "/has");
    assert_eq!(rows.len(), 1, "{out}");
    let mut expected = rule.clone();
    expected["has"] = json_fixture(
        r#"{
            "any": [
                {"pattern": "unwrap()"},
                {"pattern": "$QUALIFIER.unwrap()"}
            ],
            "stopBy": "end"
        }"#,
    );
    assert_eq!(rows[0]["rule"], expected, "{out}");
    assert_eq!(lead_count(&root.0, &rows), 2, "{out}");
}

/// The lead of a rule whose `any` holds the pattern at `at`.
fn lead_rows_at(out: &Value, at: &str) -> Vec<Value> {
    checks::find_related_call(out, "astSearch", |input| {
        input["queries"][0]["rule"]
            .pointer(at)
            .is_some_and(|holder| holder.get("any").is_some())
    })
    .and_then(|call| call["input"]["queries"].as_array().cloned())
    .unwrap_or_default()
}

/// A pattern under `not` that misses its longer spellings excludes less, so
/// it can never empty a result: no note and no lead, whatever the code holds.
#[test]
fn a_negated_bare_call_gets_no_qualified_note() {
    let root = Fixture::new();
    root.write(
        "a.rs",
        "fn a(x: Option<u8>) -> u8 {\n    x.unwrap()\n}\nfn b() {}\n",
    );
    for rule in [
        json_fixture(
            r#"{
                "kind": "function_item",
                "not": {"has": {"pattern": "unwrap()", "stopBy": "end"}}
            }"#,
        ),
        json_fixture(
            r#"{
                "kind": "function_item",
                "all": [{"not": {"has": {"pattern": "unwrap()", "stopBy": "end"}}}]
            }"#,
        ),
    ] {
        let out = rule_matched(&root.0, &rule);
        assert!(qualified_notes(&out).is_empty(), "{rule}: {out}");
        assert!(lead_rows_at(&out, "/not/has").is_empty(), "{rule}: {out}");
        assert!(
            lead_rows_at(&out, "/all/0/not/has").is_empty(),
            "{rule}: {out}"
        );
    }
}

/// Each pattern of an `any` is rewritten in place with its own spellings,
/// with captures of its own; alternatives add their own matches, so the
/// count is exactly what the lead adds.
#[test]
fn each_pattern_of_an_any_lists_its_own_longer_spellings() {
    let root = Fixture::new();
    root.write(
        "a.rs",
        "fn f(x: &S, y: &S) {\n    x.run(1);\n    y.stop(2);\n    stop(3);\n}\n",
    );
    let rule = json_fixture(
        r#"{
            "any": [{"pattern": "run($$$A)"}, {"pattern": "stop($$$A)"}]
        }"#,
    );
    let out = rule_matched(&root.0, &rule);
    assert_eq!(out["matchCount"], 1, "{out}");
    let notes = qualified_notes(&out);
    assert_eq!(notes.len(), 1, "{out}");
    let message = notes[0]["message"].as_str().expect("message");
    assert!(
        message.contains(": 2 more match(es) in 1 file(s)")
            && message.contains("`run`")
            && message.contains("`stop`")
            && message.contains("a.rs:2"),
        "{message}"
    );
    let rows = lead_rows_at(&out, "/any/0");
    assert_eq!(rows.len(), 1, "{out}");
    assert_eq!(
        rows[0]["rule"],
        json_fixture(
            r#"{
                "any": [
                    {
                        "any": [
                            {"pattern": "run($$$A)"},
                            {"pattern": "$QUALIFIER.run($$$A)"}
                        ]
                    },
                    {
                        "any": [
                            {"pattern": "stop($$$A)"},
                            {"pattern": "$QUALIFIER2.stop($$$A)"}
                        ]
                    }
                ]
            }"#
        ),
        "{out}"
    );
    assert_eq!(lead_count(&root.0, &rows), 3, "{out}");
}

/// Two patterns in one match: each is probed with the other as written, so a
/// match that needs both qualified at once is returned by the lead but not
/// counted. The note then gives its count as a floor.
#[test]
fn two_nested_patterns_count_a_floor_of_what_the_lead_adds() {
    let root = Fixture::new();
    root.write(
        "a.rs",
        "fn f(x: &S) {\n    x.run(1);\n    stop(2);\n}\nfn g(y: &S) {\n    run(1);\n    y.stop(2);\n}\nfn h(x: &S, y: &S) {\n    x.run(1);\n    y.stop(2);\n}\n",
    );
    let rule = json_fixture(
        r#"{
            "kind": "function_item",
            "all": [
                {"has": {"pattern": "run($$$A)", "stopBy": "end"}},
                {"has": {"pattern": "stop($$$B)", "stopBy": "end"}}
            ]
        }"#,
    );
    let out = rule_matched(&root.0, &rule);
    assert_eq!(out["status"], "empty", "{out}");
    let said = qualified_hints(&out);
    assert_eq!(said.len(), 1, "{out}");
    assert!(
        said[0].contains("at least 2 more match(es) in 1 file(s)"),
        "{}",
        said[0]
    );
    let rows = lead_rows_at(&out, "/all/0/has");
    assert_eq!(rows.len(), 1, "{out}");
    assert!(rows[0]["rule"].pointer("/all/1/has/any").is_some(), "{out}");
    assert_eq!(lead_count(&root.0, &rows), 3, "{out}");
}

/// The receiver-call fixture: two bare calls and one on a receiver.
const CALLS_TS: &str = "function f(client: C) {\n  runTask(\"inside\");\n  client.runTask(\"receiver\");\n}\nrunTask(\"outside\");\n";

/// A lead asks the call's question again with every spelling, so it keeps
/// the summary view the call asked for: counts asked, counts offered.
#[test]
fn a_lead_keeps_the_view_the_call_asked_for() {
    let root = Fixture::new();
    root.write("calls.ts", CALLS_TS);
    for view in ["countMatches", "files"] {
        let out = run(
            &root.0,
            json!({"operation":"match","path":root.0,"rule":{"pattern":"runTask($$$A)"},"view":view}),
        )
        .expect("match");
        let rows = lead_rows(&out);
        assert_eq!(rows.len(), 1, "{out}");
        assert_eq!(rows[0]["view"], view, "{out}");
        octocode_runtime_base::contracts::validate_query("astSearch", rows[0].clone())
            .expect("a valid lead row");
        let listed = run(&root.0, rows[0].clone()).expect("lead runs");
        let file = &listed["files"][0];
        assert!(file.get("matches").is_none(), "{listed}");
        if view == "countMatches" {
            assert_eq!(file["matchCount"], 3, "{listed}");
        }
    }
}

/// The note beside matches states what it found; its recovery, the same on
/// every such note, is stated once by the output reference under its code.
/// The note over a file scope places the first spelling by line, the file
/// being the request's own path.
#[test]
fn a_constant_recovery_is_stated_by_the_reference() {
    let root = Fixture::new();
    root.write("calls.ts", CALLS_TS);
    let out = matched(&root.0, &root.0.join("calls.ts"), "runTask($$$A)");
    let notes = out["diagnostics"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|diagnostic| diagnostic["code"] == CODE)
        .collect::<Vec<_>>();
    assert_eq!(notes.len(), 1, "{out}");
    assert!(notes[0].get("recovery").is_none(), "{out}");
    let message = notes[0]["message"].as_str().expect("message");
    assert!(
        message.contains("(first: line 3)") && !message.contains("calls.ts"),
        "{message}"
    );
}

/// A single file that spells no pattern name after a separator holds no
/// longer spelling: the probe reads that from the file's text, as a
/// directory scope's probe does, and scans the file no second time. A file
/// that spells one is still probed and counted.
#[test]
fn a_file_that_spells_no_name_after_a_separator_is_not_scanned_again() {
    let root = Fixture::new();
    let file = root.0.join("calls.rs");
    let scans = || super::super::exact_miss::RULE_SCANS.with(std::cell::Cell::get);
    std::fs::write(&file, "fn f() {\n    run(0);\n    run(1);\n}\n").expect("source");
    let before = scans();
    let out = matched(&root.0, &file, "run($$$A)");
    assert_eq!(out["matchCount"], 2, "{out}");
    assert!(qualified_notes(&out).is_empty(), "{out}");
    assert_eq!(scans() - before, 0, "no probe scan: {out}");
    std::fs::write(&file, "fn f(x: &S) {\n    run(0);\n    x.run(1);\n}\n").expect("source");
    let out = matched(&root.0, &file, "run($$$A)");
    assert_eq!(out["matchCount"], 1, "{out}");
    let notes = qualified_notes(&out);
    assert_eq!(notes.len(), 1, "{out}");
    let message = notes[0]["message"].as_str().expect("message");
    assert!(message.contains("1 more match(es)"), "{message}");
}
