//! Patterns: compile errors, exact matching, prefiltered scopes and the
//! leads a miss or a repairable failure offers.

use super::*;

#[test]
fn structural_zero_is_empty_with_actionable_pattern_guidance() {
    let root = Fixture::new();
    let source = root.0.join("source.ts");
    std::fs::write(&source, "const answer = 42;\n").expect("source");
    let (paths, security) = simple_policy(&root.0);

    let missing = execute_row(
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":source,"pattern":"let $A = $B"}),
        &paths,
        &security,
        &NeverCancel,
    )
    .expect("structural zero");
    assert_eq!(missing["status"], "empty", "{missing}");
    assert!(
        !missing["isPartial"].as_bool().unwrap_or(false),
        "{missing}"
    );
    // The coverage is stated by fields; the node shape is the viewSyntax lead.
    assert!(missing.get("hints").is_none(), "{missing}");
    assert_eq!(
        missing["next"]["viewSyntax"]["query"]["queries"][0]["operation"], "syntaxTree",
        "{missing}"
    );

    // Trailing punctuation the pattern omits is not required (ast-grep
    // smart strictness).
    for pattern in ["const $A = $B;", "const $A = $B"] {
        let found = execute_row(
            json!({"operation":"match","mainGoal":"test","reasoning":"test","path":source,"pattern":pattern}),
            &paths,
            &security,
            &NeverCancel,
        )
        .expect("structural match");
        assert_eq!(found["matchCount"], 1, "{found}");
        assert!(found.get("status").is_none(), "{found}");
    }
}

#[test]
fn an_unparseable_pattern_reports_untagged_text() {
    let root = Fixture::new();
    let source = root.0.join("a.rs");
    std::fs::write(&source, "fn a() {}\n").expect("a");
    for path in [source.clone(), root.0.clone()] {
        let error = run(
            &root.0,
            json!({"operation":"match","mainGoal":"test","reasoning":"test","path":path,"language":"rust","pattern":"neu("}),
        )
        .expect_err("invalid pattern");
        assert_eq!(error.code, ErrorCode::InvalidPattern);
        assert!(!error.message.starts_with('['), "{}", error.message);
    }
}

#[test]
fn directory_pattern_that_compiles_in_no_file_is_an_error_not_empty() {
    let root = Fixture::new();
    for name in ["a.rs", "b.rs"] {
        std::fs::write(root.0.join(name), "fn main() {}\n").expect("file");
    }
    let error = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":"rust","pattern":"fn ???"}),
    )
    .expect_err("uncompilable pattern must fail loudly");
    assert_eq!(error.code, ErrorCode::InvalidPattern);
}

#[test]
fn patterns_without_metavariables_offer_no_capture_expansion() {
    let root = Fixture::new();
    let source = root.0.join("calls.ts");
    std::fs::write(&source, "oldCall(one);\n").expect("source");
    let out = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":source,"pattern":"oldCall(one)"}),
    )
    .expect("match");
    assert_eq!(
        out["files"][0]["matches"][0],
        json!({"line":1,"column":1,"value":"oldCall(one)"}),
        "{out}"
    );
    assert!(
        out.get("next")
            .is_none_or(|next| next.get("expandCaptures").is_none()),
        "{out}"
    );
}

/// A pattern is the answer as written: one that omits a part (Rust
/// visibility, a return type) matches only items spelled without it, with
/// no silent broader rule, and equals its `rule: {pattern}` form.
#[test]
fn a_pattern_matches_exactly_as_written() {
    let root = Fixture::new();
    let rust = root.0.join("items.rs");
    std::fs::write(
        &rust,
        "pub fn a() -> u8 { 1 }\npub(crate) fn b() {}\nfn c() {}\n",
    )
    .expect("rust");
    let typed = root.0.join("typed.ts");
    std::fs::write(
        &typed,
        "export function typed(a: number): number { return a; }\nexport function untyped(b) { return b; }\n",
    )
    .expect("typed");
    let matched = |path: &PathBuf, query: serde_json::Value| {
        let mut query = query;
        query["operation"] = json!("match");
        query["path"] = json!(path);
        run(&root.0, query).expect("match")
    };
    for (path, pattern, count) in [
        (&rust, "fn $N() { $$$B }", 1),
        (&rust, "pub fn $N() -> $R { $$$B }", 1),
        (&typed, "export function $N($$$A) { $$$B }", 1),
    ] {
        let out = matched(path, json!({"pattern": pattern}));
        assert_eq!(out["matchCount"], count, "{pattern}: {out}");
        assert!(
            out["diagnostics"]
                .as_array()
                .into_iter()
                .flatten()
                .all(|d| d["code"] != "structural.pattern.relaxed"),
            "{pattern}: {out}"
        );
        let rule = matched(path, json!({"rule": {"pattern": pattern}}));
        assert_eq!(out["files"], rule["files"], "{pattern}: {out}");
    }
}

/// An exact declaration pattern that matches nothing while the scope holds
/// declarations of its kind gets a diagnostic counting them and a rule lead
/// that lists them (named as the pattern names it); an expression pattern,
/// or a scope without the kind, gets neither.
#[test]
fn an_exact_declaration_miss_counts_the_kind_and_leads_to_its_rule() {
    let root = Fixture::new();
    let dir = root.0.join("src");
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(
        dir.join("items.rs"),
        "pub(crate) fn a() -> u8 { 1 }\npub(crate) fn b() -> u8 { 2 }\nstruct S;\n",
    )
    .expect("rust");
    std::fs::write(dir.join("more.rs"), "pub fn run() -> u8 { 3 }\n").expect("rust");
    let matched = |path: &PathBuf, pattern: &str| {
        run(
            &root.0,
            json!({"operation":"match","path":path,"pattern":pattern}),
        )
        .expect("match")
    };
    let miss = matched(&dir, "fn $NAME($$$ARGS) -> $R { $$$BODY }");
    assert_eq!(miss["matchCount"], 0, "{miss}");
    let note = miss["diagnostics"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|d| d["code"] == "structural.pattern.exact")
        .unwrap_or_else(|| panic!("exact-miss diagnostic: {miss}"));
    assert!(
        note["message"]
            .as_str()
            .is_some_and(|m| m.contains("3 function_item node(s) in 2 file(s)")),
        "{note}"
    );
    let lead = &miss["next"]["broadenSearch"]["query"]["queries"][0];
    assert_eq!(lead["rule"], json!({"kind":"function_item"}), "{miss}");
    assert!(lead.get("pattern").is_none(), "{lead}");
    crate::contracts::validate_query("astSearch", lead.clone()).expect("a valid lead row");
    let mut follow = lead.clone();
    follow["mainGoal"] = json!("test");
    let listed = run(&root.0, follow).expect("lead runs");
    assert_eq!(listed["matchCount"], 3, "{listed}");
    // A named pattern lists only its name.
    let named = matched(&dir.join("more.rs"), "fn run() -> u8 { 3 }");
    let lead = &named["next"]["broadenSearch"]["query"]["queries"][0];
    assert_eq!(
        lead["rule"],
        json!({"kind":"function_item","has":{"kind":"identifier","regex":"^run$"}}),
        "{named}"
    );
    // An expression pattern is no declaration: no count, no lead.
    let call = matched(&dir, "$X.unwrap()");
    assert!(call["next"].get("broadenSearch").is_none(), "{call}");
    // A kind the scope lacks: nothing to list.
    let none = matched(&dir, "trait $T { $$$B }");
    assert!(none["next"].get("broadenSearch").is_none(), "{none}");
}

/// A pattern the grammar cannot parse fails before any prefilter: a
/// directory where no file holds the anchor must not report `complete` and
/// empty for `foo(`.
#[test]
fn invalid_pattern_is_an_error_even_when_every_file_is_prefiltered() {
    let root = Fixture::new();
    root.write("a.ts", "const value = 1;\n");
    let error = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":"typescript","pattern":"foo("}),
    )
    .expect_err("an unparseable pattern must fail loudly");
    // The response stage attaches the repair hint for this code.
    assert_eq!(error.code, ErrorCode::InvalidPattern, "{error:?}");
}

/// Matches inside a `macro_rules!` template body are listed (no
/// silent omission behind `complete:true`).
#[test]
fn macro_rules_template_bodies_are_listed() {
    let root = Fixture::new();
    root.write("m.rs", "macro_rules! get {\n    ($e:expr) => {\n        config.value.unwrap()\n    };\n}\nfn main() {\n    other.unwrap();\n}\n");
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"language":"rust","pattern":"$X.unwrap()"}),
    )
    .expect("match");
    let lines = out["files"][0]["matches"]
        .as_array()
        .map(|rows| rows.iter().map(|row| lean_row(row).0).collect::<Vec<_>>())
        .unwrap_or_default();
    assert_eq!(lines, [3, 7], "{out}");
}

/// An empty match counts the files it parsed and leads to one bounded page
/// of the first one's syntax tree, at the line that spells the pattern's
/// anchor, where the pattern's node shape can be compared.
#[test]
fn empty_match_leads_to_syntax_tree() {
    let root = Fixture::new();
    root.write("a.rs", "fn gamma() {}\n\nfn alpha() {\n    beta(1);\n}\n");
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"language":"rust","pattern":"beta($A, $B)"}),
    )
    .expect("match");
    assert_eq!(out["status"], "empty", "{out}");
    assert_eq!(out["filesScanned"], 1, "{out}");
    let lead = &out["next"]["viewSyntax"];
    assert_eq!(lead["tool"], "astSearch", "{out}");
    let row = &lead["query"]["queries"][0];
    assert_eq!(row["operation"], "syntaxTree", "{out}");
    assert!(
        row["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("a.rs")),
        "{out}"
    );
    assert_eq!(row["line"], 4, "{out}");
    assert_eq!(row["pageSize"], 40, "{out}");
    crate::contracts::validate_query("astSearch", row.clone()).expect("a valid syntaxTree row");
    let tree = run(&root.0, row.clone()).expect("syntax tree");
    assert!(
        tree["nodes"][0]
            .as_str()
            .is_some_and(|node| node.contains("expression_statement 4:")),
        "{tree}"
    );
}

/// Repairable failures carry their one lead and no advice text: a rule
/// that does not compile against a file leads to its syntax tree; a
/// language no file under a directory has leads to the query in the grammar
/// the directory does hold (a repair never removes a field the run needs).
#[test]
fn repairable_failures_carry_one_lead() {
    let root = Fixture::new();
    let file = root.0.join("a.rs");
    std::fs::write(&file, "fn a() {}\n").expect("a");
    let error = run(
        &root.0,
        json!({"operation":"match","path":file,"rule":{"kind":"not_a_real_kind"}}),
    )
    .expect_err("unknown kind");
    assert_eq!(error.code, ErrorCode::InvalidPattern, "{error:?}");
    assert!(error.hints.is_empty(), "{error:?}");
    let next = error.next.as_ref().expect("lead");
    assert_eq!(
        next["viewSyntax"]["query"]["queries"][0]["operation"], "syntaxTree",
        "{next}"
    );

    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"language":"python","pattern":"print($A)"}),
    )
    .expect("no python file");
    let repair = &out["next"]["repair"]["query"]["queries"][0];
    assert_eq!(repair["language"], "rust", "{out}");
    assert_eq!(repair["pattern"], "print($A)", "{out}");
    assert_eq!(
        out["hints"],
        json!(["No python file under this path was parsed."]),
        "{out}"
    );
}
