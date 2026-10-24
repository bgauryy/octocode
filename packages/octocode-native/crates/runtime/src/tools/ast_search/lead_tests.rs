//! Every lead astSearch hands out validates against the contract and runs
//! without an error: a hint that loops into another error is a bug.
use super::{AstSearchQuery, execute_ast};
use crate::execution::cancel::NeverCancel;
use crate::tools::test_support::{Fixture, simple_policy};
use serde_json::{Value, json};

fn leads(out: &Value) -> Vec<(String, Value)> {
    crate::tools::test_support::checks::leads(out)
        .into_iter()
        .filter_map(|lead| {
            if lead.tool == "astSearch" {
                Some((lead.name, lead.query))
            } else {
                crate::contracts::validate_built(&lead.tool, lead.query)
                    .unwrap_or_else(|error| panic!("related {} invalid: {error:?}", lead.tool));
                None
            }
        })
        .collect()
}

/// Runs `row`, then follows every lead it names (and theirs) to `depth`,
/// asserting each validates and succeeds. Returns the calls it ran.
fn follow(root: &std::path::Path, row: Value, depth: usize) -> usize {
    let (paths, security) = simple_policy(root);
    let mut pending = vec![("start".to_owned(), json!({"queries":[row]}))];
    let mut ran = 0;
    for _ in 0..=depth {
        let mut next_pending = Vec::new();
        for (name, input) in pending.drain(..) {
            crate::contracts::validate_built("astSearch", input.clone())
                .unwrap_or_else(|error| panic!("lead {name} invalid {input}: {:?}", error.issues));
            for row in input["queries"].as_array().expect("rows") {
                let query: AstSearchQuery = serde_json::from_value(row.clone()).expect("typed row");
                ran += 1;
                match execute_ast(&query, &paths, &security, &NeverCancel, None) {
                    Ok(out) => {
                        assert!(out.get("errorCode").is_none(), "{name} -> {out}");
                        next_pending.extend(leads(&out));
                    }
                    Err(error) => {
                        panic!("lead {name} {row} failed: {error:?}")
                    }
                }
            }
        }
        pending = next_pending;
    }
    ran
}

/// A directory with no file of the requested language leads to a query that
/// runs; removing a language a directory needs is not that.
#[test]
fn empty_language_lead_runs() {
    let root = Fixture::new();
    root.write("a.rs", "fn a() { hit(1); }\n");
    root.write("b.ts", "hit(2);\n");
    let ran = follow(
        &root.0,
        json!({"operation":"match","path":root.0,"parser":"python","rule":{"pattern":"hit($A)"}}),
        2,
    );
    assert!(ran >= 1);
}

/// Per-file inference covers more grammars than a repair batch could hold.
#[test]
fn per_file_inference_covers_more_grammars_than_a_repair_batch() {
    let root = Fixture::new();
    for (name, source) in [
        ("a.rs", "fn a() { hit(1); }\n"),
        ("b.ts", "hit(2);\n"),
        ("c.py", "hit(3)\n"),
        ("d.js", "hit(4);\n"),
        ("e.java", "class E { void e() { hit(5); } }\n"),
        ("f.go", "package f\nfunc f() { hit(6) }\n"),
        ("g.cs", "class G { void G() { hit(7); } }\n"),
        ("h.cpp", "void h() { hit(8); }\n"),
    ] {
        std::fs::write(root.0.join(name), source).expect("source");
    }
    let (paths, security) = simple_policy(&root.0);
    let query: AstSearchQuery = serde_json::from_value(
        json!({"operation":"match","path":root.0,"rule":{"pattern":"hit($A)"}}),
    )
    .expect("row");
    let out =
        execute_ast(&query, &paths, &security, &NeverCancel, None).expect("per-file inference");
    assert_eq!(out["matchCount"], 8, "{out}");
    assert_eq!(out["files"].as_array().map(Vec::len), Some(8), "{out}");
    assert!(out["next"].get("repair").is_none(), "{out}");
}

/// A battery of real astSearch calls: every lead any of them hands out (page,
/// scan, capture, syntax-tree and text leads, to three hops) validates and
/// runs without error.
#[test]
fn every_lead_of_a_battery_of_calls_validates_and_runs() {
    let root = Fixture::new();
    for index in 0..6 {
        let calls = (0..4)
            .map(|call| format!("hit({index}{call});\n"))
            .collect::<String>();
        root.write(
            &format!("f{index}.ts"),
            format!("export class C{index} extends Base {{ run() {{ {calls} }} }}\n{calls}"),
        );
    }
    let dir = root.0.to_string_lossy().into_owned();
    let file = root.0.join("f0.ts").to_string_lossy().into_owned();
    let rows = [
        json!({"operation":"match","path":dir,"parser":"typescript","rule":{"pattern":"hit($A)"}}),
        json!({"operation":"match","path":dir,"parser":"typescript","rule":{"pattern":"hit($A)"},"maxFiles":2}),
        json!({"operation":"match","path":dir,"parser":"typescript","rule":{"pattern":"hit($A)"},"includeCaptures":false}),
        json!({"operation":"match","path":dir,"parser":"typescript","rule":{"pattern":"nothing($A)"}}),
        json!({"operation":"match","path":dir,"parser":"typescript","rule":{"pattern":"hit($A)"}}),
        json!({"operation":"symbols","path":dir}),
        json!({"operation":"symbols","path":dir,"symbolFilter":{"terms":["C1"]},"maxFiles":2}),
        json!({"operation":"symbols","path":file}),
        json!({"operation":"syntaxTree","path":file}),
    ];
    for row in rows {
        follow(&root.0, row, 3);
    }
}

/// Files an empty scan skipped (ignored, hidden, default-excluded) lead to a
/// query that finds them.
#[test]
fn leads_over_skipped_files_validate_and_find_them() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join(".git")).expect("git dir");
    root.write(".gitignore", "gen/\n");
    for dir in ["gen", ".cfg", "node_modules/pkg"] {
        std::fs::create_dir_all(root.0.join(dir)).expect("dir");
        std::fs::write(
            root.0.join(dir).join("a.py"),
            "def skipped():\n    hit(1)\n",
        )
        .expect("source");
    }
    let row =
        json!({"operation":"match","path":root.0,"parser":"python","rule":{"pattern":"hit($A)"}});
    let ran = follow(&root.0, row, 2);
    assert!(ran >= 2, "the empty call and its lead both ran");
}

/// Runs one astSearch row and returns its (first) result value.
fn run(root: &std::path::Path, row: Value) -> Value {
    let (paths, security) = simple_policy(root);
    let query: AstSearchQuery = serde_json::from_value(row).expect("typed row");
    execute_ast(&query, &paths, &security, &NeverCancel, None).expect("runs")
}

/// A diagnosis respects the query's `include`: `.spin/` holds python too, but
/// the filter is `.github/**`, so the hint names `.github/` alone, and the
/// repair does not offer languages that occur only outside the filter.
#[test]
fn skipped_hint_respects_the_query_include() {
    let root = Fixture::new();
    for dir in [".spin", ".github"] {
        std::fs::create_dir_all(root.0.join(dir)).expect("dir");
        std::fs::write(root.0.join(dir).join("a.py"), "class E(Exception): pass\n")
            .expect("source");
    }
    root.write("lib.cpp", "void f() {}\n");
    root.write("app.js", "function f() {}\n");
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"include":[".github/**"],"parser":"python","rule":{"pattern":"class $N(Exception): $$$B"}}),
    );
    let hint = out["hints"].to_string();
    assert!(hint.contains("hidden .github/"), "{hint}");
    assert!(!hint.contains(".spin"), "{hint}");
    assert!(out["next"].get("repair").is_none(), "{out}");
    let lead = &super::test_rows::skipped_call(&out)
        .unwrap_or_else(|| panic!("missing skipped-scope lead: {out}"))["input"]["queries"][0];
    assert_eq!(lead["includeHidden"], json!(true), "{out}");
    assert_eq!(lead["include"], json!([".github/**"]), "{out}");
}

/// Hidden files outside the query's `include` are not a reason for an empty
/// result: no lead, no hidden claim.
#[test]
fn skipped_files_outside_the_include_are_not_named() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join(".spin")).expect("dir");
    std::fs::create_dir_all(root.0.join("src")).expect("dir");
    root.write(".spin/a.py", "def a(): pass\n");
    root.write("src/b.js", "function b() {}\n");
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"include":["src/**"],"parser":"python","rule":{"pattern":"def $N(): pass"}}),
    );
    let hint = out["hints"][0].as_str().expect("hint");
    assert!(!hint.contains("skipped as"), "{hint}");
    assert!(super::test_rows::skipped_call(&out).is_none(), "{out}");
}

/// An empty match states what it searched as `filesScanned`, for a pattern
/// and for a rule alike, with no hint repeating it (why a compiled query
/// can match nothing is the tool description's). No file spells the
/// query's anchor (the prefilter skipped every one unparsed), so no file is
/// a near miss: no syntax tree of an unrelated file is offered.
#[test]
fn an_empty_match_reports_the_files_it_parsed() {
    let root = Fixture::new();
    for name in ["a.ts", "b.ts", "c.ts"] {
        std::fs::write(root.0.join(name), "export const a = 1;\n").expect("source");
    }
    for query in [
        json!({"rule":{"pattern":"zzNoSuch($$$A)"}}),
        json!({"rule":{"pattern":"zzNoSuch($$$A)"}}),
    ] {
        let mut row = json!({"operation":"match","path":root.0,"parser":"typescript"});
        row.as_object_mut()
            .expect("object")
            .extend(query.as_object().expect("object").clone());
        let out = run(&root.0, row);
        assert_eq!(out["status"], "empty", "{out}");
        assert_eq!(out["filesScanned"], json!(3), "{out}");
        assert!(out.get("hints").is_none(), "{out}");
        assert!(out["next"].get("viewSyntax").is_none(), "{out}");
    }
}

/// The live miss (2026-10-24): `fn failure(..) -> $R {..}` over a directory
/// with no function of that name led to page 1 of an unrelated file whose
/// comment spelled `failure`. A declaration pattern with no declaration
/// of its kind and name offers the rule listing its kind instead.
#[test]
fn an_empty_declaration_without_a_near_miss_broadens_to_its_kind() {
    let root = Fixture::new();
    root.write(
        "a.rs",
        "// failure modes
fn other() -> u8 { 1 }
",
    );
    root.write(
        "b.rs",
        "fn second(x: u8) -> u8 { x }
",
    );
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"rule":{"pattern":"fn failure($$$ARGS) -> $R { $$$BODY }"}}),
    );
    assert_eq!(out["status"], "empty", "{out}");
    assert!(out["next"].get("viewSyntax").is_none(), "{out}");
    let broaden = &crate::tools::test_support::checks::related_call(&out, "astSearch", |_| true)["input"]
        ["queries"][0];
    assert_eq!(broaden["rule"], json!({"kind":"function_item"}), "{out}");
    assert!(broaden.get("pattern").is_none(), "{out}");
    let listed = run(&root.0, broaden.clone());
    assert_eq!(listed["matchCount"], 2, "{listed}");
}

/// An empty declaration pattern's syntax-tree lead opens a file holding a
/// declaration of the pattern's kind, at that declaration's line, not the
/// first file the scan read.
#[test]
fn an_empty_declaration_match_leads_to_a_tree_of_that_kind() {
    let root = Fixture::new();
    root.write("a.rs", "const A: u8 = 1;\n");
    root.write("b.rs", "// one\nfn helper() -> u8 { 1 }\n");
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"rule":{"pattern":"fn $N() -> u128 { $$$ }"}}),
    );
    assert_eq!(out["status"], "empty", "{out}");
    let tree = &crate::tools::test_support::checks::related_call(&out, "astSearch", |input| {
        input["queries"][0]["operation"] == "syntaxTree"
    })["input"]["queries"][0];
    assert!(
        tree["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("b.rs")),
        "{out}"
    );
    assert_eq!(tree["line"], 2, "{out}");
}

/// A match that found something still names the walks the defaults skipped
/// for files of its grammar, with the lead that includes them: the matches
/// listed are not the scope's matches.
#[test]
fn a_match_with_hits_names_the_skipped_walks_and_leads_to_them() {
    let root = Fixture::new();
    root.write("a.ts", "hit(1);\n");
    root.write("dist/b.ts", "hit(2);\n");
    let request = json!({"operation":"match","path":root.0,"parser":"typescript","rule":{"pattern":"hit($A)"}});
    let out = run(&root.0, request);
    assert_eq!(out["matchCount"], json!(1), "{out}");
    let hints = out["hints"].to_string();
    assert!(
        hints.contains("1 typescript file behind skipped paths (dist/)"),
        "{hints}"
    );
    let lead = &super::test_rows::skipped_call(&out)
        .unwrap_or_else(|| panic!("missing skipped-scope lead: {out}"));
    assert_eq!(lead["tool"], "astSearch", "{out}");
    let retry = &lead["input"]["queries"][0];
    assert_eq!(retry["defaultExcludes"], json!(false), "{out}");
    let found = run(&root.0, retry.clone());
    assert_eq!(found["matchCount"], json!(2), "{found}");
    assert!(found.get("warnings").is_none(), "{found}");
    assert!(super::test_rows::skipped_call(&found).is_none(), "{found}");
}

/// Nothing skipped, nothing said: no notice and no lead beside the hits.
#[test]
fn a_match_with_hits_and_nothing_skipped_has_no_skipped_notice() {
    let root = Fixture::new();
    root.write("a.ts", "hit(1);\n");
    root.write("dist/readme.md", "hit(2);\n");
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"parser":"typescript","rule":{"pattern":"hit($A)"}}),
    );
    assert_eq!(out["matchCount"], json!(1), "{out}");
    assert!(out.get("warnings").is_none(), "{out}");
    assert!(super::test_rows::skipped_call(&out).is_none(), "{out}");
}
