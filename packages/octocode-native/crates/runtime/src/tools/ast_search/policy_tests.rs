use super::execute_ast;
use crate::{
    policy::path::{PathPolicy, PathPolicyConfig},
    security::ContentSecurity,
    tools::local_fetch::CancellationCheck,
};
use serde_json::json;
use std::path::PathBuf;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "octocode-ast-policy-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).expect("fixture");
        Self(root)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct Active;
impl CancellationCheck for Active {
    fn check(&self) -> Result<(), String> {
        Ok(())
    }
}

#[test]
fn descendant_policy_precedes_discovery_totals_and_line_reads() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".aws")).expect("sensitive directory");
    std::fs::write(root.0.join(".aws/credentials"), "hidden\n".repeat(50)).expect("secret");
    // Built-in sensitive file name: ignored by the path policy, not by config.
    std::fs::write(root.0.join("terraform.tfstate"), "ignored\n".repeat(70)).expect("ignored");
    std::fs::write(root.0.join("visible.rs"), "pub fn visible() {\n}\n").expect("source");
    let paths = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.0.clone()),
        ..Default::default()
    })
    .expect("policy");
    let security = ContentSecurity::new();
    let symbols = execute_ast(
        json!({"operation":"symbols","reasoning":"test","path":root.0}),
        &paths,
        &security,
        &Active,
    )
    .expect("symbols");
    assert_eq!(symbols["filesScanned"], 1);
    assert_eq!(symbols["filesSkipped"], 0);
    assert_eq!(symbols["declarations"][0]["name"], "visible");

    std::fs::write(root.0.join(".aws/hidden.ts"), "oldCall(secret);\n").expect("hidden ast");
    std::fs::write(root.0.join(".env.ts"), "oldCall(ignored);\n").expect("ignored ast");
    std::fs::write(root.0.join("visible.ts"), "oldCall(visible);\n").expect("visible ast");
    let matches = execute_ast(
        json!({
            "operation":"match","reasoning":"test","path":root.0,"langType":"typescript",
            "pattern":"oldCall($A)","hidden":true
        }),
        &paths,
        &security,
        &Active,
    )
    .expect("structural matches");
    assert_eq!(matches["stats"]["totalStructuralMatches"], 1);
    assert_eq!(matches["files"].as_array().expect("files").len(), 1);
    let root_name = root.0.file_name().expect("root name").to_string_lossy();
    assert_eq!(
        matches["files"][0]["path"],
        format!("{root_name}/visible.ts")
    );
    assert!(!matches.to_string().contains("hidden.ts"));
    assert!(!matches.to_string().contains(".env.ts"));
}

#[test]
fn structural_zero_is_empty_with_actionable_pattern_guidance() {
    let root = Fixture::new();
    let source = root.0.join("source.ts");
    std::fs::write(&source, "const answer = 42;\n").expect("source");
    let paths = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.0.clone()),
        ..Default::default()
    })
    .expect("policy");
    let security = ContentSecurity::new();

    let missing = execute_ast(
        json!({"operation":"match","reasoning":"test","path":source,"pattern":"const $A = $B"}),
        &paths,
        &security,
        &Active,
    )
    .expect("structural zero");
    assert_eq!(missing["status"], "empty", "{missing}");
    assert_eq!(missing["complete"], true, "{missing}");
    let guidance = missing["diagnostics"][0]["message"]
        .as_str()
        .expect("no-match guidance");
    assert!(guidance.contains("trailing semicolons"), "{guidance}");
    assert!(guidance.contains("operation:\"syntaxTree\""), "{guidance}");

    let found = execute_ast(
        json!({"operation":"match","reasoning":"test","path":source,"pattern":"const $A = $B;"}),
        &paths,
        &security,
        &Active,
    )
    .expect("structural match");
    assert_eq!(found["stats"]["totalStructuralMatches"], 1, "{found}");
    assert!(found.get("status").is_none(), "{found}");
}

#[test]
fn directory_prefilter_skips_are_aggregated_once() {
    let root = Fixture::new();
    for name in ["one.ts", "two.ts", "three.ts"] {
        std::fs::write(root.0.join(name), "const value = 1;\n").expect("source");
    }
    let paths = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.0.clone()),
        ..Default::default()
    })
    .expect("policy");
    let security = ContentSecurity::new();
    let result = execute_ast(
        json!({
            "operation":"match","reasoning":"test",
            "path":root.0,
            "langType":"typescript",
            "pattern":"missingCall($A);"
        }),
        &paths,
        &security,
        &Active,
    )
    .expect("structural zero");
    let prefilter = result["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "structural.prefilter.skipped")
        .collect::<Vec<_>>();
    assert_eq!(prefilter.len(), 1, "{result}");
    assert!(
        prefilter[0]["message"]
            .as_str()
            .is_some_and(|message| message.contains("3 file(s)")),
        "{result}"
    );
    assert_eq!(result["status"], "empty", "{result}");
}

#[test]
fn cancellation_interrupts_descendant_traversal() {
    struct AfterRoot(std::sync::atomic::AtomicUsize);
    impl CancellationCheck for AfterRoot {
        fn check(&self) -> Result<(), String> {
            if self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed) >= 2 {
                Err("stop traversal".into())
            } else {
                Ok(())
            }
        }
    }
    let root = Fixture::new();
    std::fs::write(root.0.join("source.rs"), "source").expect("source");
    let paths = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.0.clone()),
        ..Default::default()
    })
    .expect("policy");
    let security = ContentSecurity::new();
    let error = execute_ast(
        json!({
            "operation":"match","reasoning":"test","path":root.0,"langType":"rust",
            "pattern":"source"
        }),
        &paths,
        &security,
        &AfterRoot(std::sync::atomic::AtomicUsize::new(0)),
    )
    .expect_err("cancel structural walk");
    assert_eq!(error.code, "ast.execution.cancelled");
}

// Regression: astSearch `match` over a directory must not silently drop files
// past `maxFiles`. When the candidate scan is truncated, the result must carry
// an explicit truncation signal (a `structural.scan.truncated` diagnostic and a
// top-level `truncated`/`complete:false`) rather than reporting a bounded set as
// if it were the whole corpus.
#[test]
fn match_directory_scan_truncation_is_surfaced_not_silent() {
    let root = Fixture::new();
    // Three candidate files, each with the matchable identifier `source`.
    for name in ["a.rs", "b.rs", "c.rs"] {
        std::fs::write(root.0.join(name), "pub fn source() {}\n").expect("source file");
    }
    let paths = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.0.clone()),
        ..Default::default()
    })
    .expect("policy");
    let security = ContentSecurity::new();
    let out = execute_ast(
        json!({
            "operation":"match","reasoning":"test","path":root.0,"langType":"rust",
            "pattern":"pub fn source() {}","maxFiles":1
        }),
        &paths,
        &security,
        &Active,
    )
    .expect("match runs");

    assert_eq!(
        out["truncated"],
        json!(true),
        "capped scan must set truncated=true; got {out}"
    );
    assert_eq!(
        out["complete"],
        json!(false),
        "capped scan is not complete; got {out}"
    );
    let diagnostics = out["diagnostics"].as_array().cloned().unwrap_or_default();
    assert!(
        diagnostics
            .iter()
            .any(|d| d["code"] == json!("structural.scan.truncated")),
        "expected a structural.scan.truncated diagnostic; got {out}"
    );
}

fn simple_policy(root: &std::path::Path) -> (PathPolicy, ContentSecurity) {
    let paths = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.to_path_buf()),
        ..Default::default()
    })
    .expect("policy");
    let security = ContentSecurity::new();
    (paths, security)
}

#[test]
fn match_continuation_rejects_stale_snapshot() {
    let root = Fixture::new();
    for name in ["a.rs", "b.rs", "c.rs", "d.rs"] {
        std::fs::write(root.0.join(name), "pub fn source() {}\n").expect("file");
    }
    let (paths, security) = simple_policy(&root.0);
    let page1 = execute_ast(
        json!({"operation":"match","reasoning":"test","path":root.0,"langType":"rust","pattern":"pub fn source() {}","pageSize":2}),
        &paths,
        &security,
        &Active,
    )
    .expect("page1");
    let snapshot = page1["snapshot"].as_str().expect("snapshot").to_string();
    std::fs::write(root.0.join("e.rs"), "pub fn source() {}\n").expect("mutate corpus");
    let page2 = execute_ast(
        json!({"operation":"match","reasoning":"test","path":root.0,"langType":"rust","pattern":"pub fn source() {}","pageSize":2,"page":2,"snapshot":snapshot}),
        &paths,
        &security,
        &Active,
    )
    .expect("page2");
    assert_eq!(
        page2["errorCode"],
        json!("ast.snapshot.changed"),
        "got {page2}"
    );
}

fn run(
    root: &std::path::Path,
    query: serde_json::Value,
) -> Result<serde_json::Value, super::AstError> {
    let (paths, security) = simple_policy(root);
    execute_ast(query, &paths, &security, &Active)
}

#[test]
fn directory_pattern_that_compiles_in_no_file_is_an_error_not_empty() {
    let root = Fixture::new();
    for name in ["a.rs", "b.rs"] {
        std::fs::write(root.0.join(name), "fn main() {}\n").expect("file");
    }
    let error = run(
        &root.0,
        json!({"operation":"match","reasoning":"test","path":root.0,"langType":"rust","pattern":"fn ???"}),
    )
    .expect_err("uncompilable pattern must fail loudly");
    assert_eq!(error.code, "structural.query.compileFailed");
}

#[test]
fn match_pagination_limits_next_match_page_to_the_current_file_page() {
    let root = Fixture::new();
    std::fs::write(root.0.join("a.rs"), "fn a() { hit(1); }\n").expect("a");
    std::fs::write(root.0.join("b.rs"), "fn b() { hit(1); hit(2); hit(3); }\n").expect("b");
    let base = json!({
        "operation":"match","reasoning":"test","path":root.0,"langType":"rust","pattern":"hit($A)",
        "pageSize":1,"maxMatchesPerFile":2
    });
    let page1 = run(&root.0, base.clone()).expect("page1");
    assert_eq!(page1["files"][0]["totalMatchRows"], 1, "{page1}");
    assert!(
        page1["next"].get("nextMatchPage").is_none(),
        "page 1 has no truncated file, so no nextMatchPage: {page1}"
    );
    let next = page1["next"]["nextPage"]["query"].clone();
    let page2 = run(&root.0, next).expect("page2");
    assert_eq!(page2["files"][0]["returnedMatchRows"], 2, "{page2}");
    assert_eq!(
        page2["complete"], false,
        "unreturned matches remain: {page2}"
    );
    let deeper = page2["next"]["nextMatchPage"]["query"].clone();
    assert_eq!(deeper["matchPage"], 2, "{page2}");
    assert_eq!(deeper["page"], 2, "{page2}");
    let page2b = run(&root.0, deeper.clone()).expect("page2 matchPage2");
    assert_eq!(page2b["files"][0]["returnedMatchRows"], 1, "{page2b}");

    // A nextPage continuation always restarts per-file match pagination.
    let mut mid = base;
    mid["matchPage"] = json!(2);
    mid["snapshot"] = page1["snapshot"].clone();
    let mid = run(&root.0, mid).expect("page1 matchPage2");
    assert_eq!(mid["next"]["nextPage"]["query"]["matchPage"], 1, "{mid}");
}

#[test]
fn single_file_with_unreturned_matches_is_not_complete() {
    let root = Fixture::new();
    let source = root.0.join("many.rs");
    std::fs::write(&source, "fn m() { hit(1); hit(2); hit(3); }\n").expect("source");
    let out = run(
        &root.0,
        json!({"operation":"match","reasoning":"test","path":source,"pattern":"hit($A)","maxMatchesPerFile":2}),
    )
    .expect("match");
    assert_eq!(out["complete"], false, "{out}");
    assert!(out["next"]["nextMatchPage"].is_object(), "{out}");
}

#[test]
fn lang_type_is_validated_and_intersected_with_include() {
    let root = Fixture::new();
    std::fs::write(root.0.join("a.ts"), "oldCall(x);\n").expect("ts");
    std::fs::write(root.0.join("b.py"), "oldCall(x)\n").expect("py");
    let unknown = run(
        &root.0,
        json!({"operation":"match","reasoning":"test","path":root.0,"langType":"klingon","pattern":"oldCall($A)"}),
    )
    .expect_err("unknown langType");
    assert_eq!(unknown.code, "ast.language.unsupported");

    let out = run(
        &root.0,
        json!({
            "operation":"match","reasoning":"test","path":root.0,"langType":"typescript",
            "include":["*.ts","*.py"],"pattern":"oldCall($A)"
        }),
    )
    .expect("intersected include");
    let files = out["files"].as_array().expect("files");
    assert_eq!(files.len(), 1, "{out}");
    assert!(
        files[0]["path"]
            .as_str()
            .is_some_and(|p| p.ends_with("a.ts")),
        "{out}"
    );

    let mismatch = run(
        &root.0,
        json!({"operation":"match","reasoning":"test","path":root.0.join("b.py"),"langType":"typescript","pattern":"oldCall($A)"}),
    )
    .expect_err("single file outside langType");
    assert_eq!(mismatch.code, "ast.language.mismatch");

    std::fs::write(root.0.join("notes.txt"), "oldCall(x)\n").expect("txt");
    let unsupported = run(
        &root.0,
        json!({"operation":"match","reasoning":"test","path":root.0.join("notes.txt"),"pattern":"oldCall($A)"}),
    )
    .expect("unsupported single file");
    assert_eq!(unsupported["complete"], false, "{unsupported}");
    assert_ne!(unsupported["status"], "empty", "{unsupported}");
}

#[test]
fn dot_prefixed_lang_type_selects_only_that_extension() {
    let root = Fixture::new();
    std::fs::write(root.0.join("a.ts"), "oldCall(x);\n").expect("ts");
    std::fs::write(root.0.join("b.mts"), "oldCall(y);\n").expect("mts");

    let exact = run(
        &root.0,
        json!({"operation":"match","reasoning":"test","path":root.0,"langType":".ts","pattern":"oldCall($A)"}),
    )
    .expect("exact extension");
    let files = exact["files"].as_array().expect("files");
    assert_eq!(files.len(), 1, "{exact}");
    assert!(
        files[0]["path"]
            .as_str()
            .is_some_and(|p| p.ends_with("a.ts"))
    );

    let family = run(
        &root.0,
        json!({"operation":"match","reasoning":"test","path":root.0,"langType":"typescript","pattern":"oldCall($A)"}),
    )
    .expect("language family");
    assert_eq!(
        family["files"].as_array().expect("files").len(),
        2,
        "{family}"
    );
}

#[test]
fn cpp_header_can_use_explicit_cpp_grammar_without_changing_h_default() {
    let root = Fixture::new();
    let header = root.0.join("widget.h");
    std::fs::write(
        &header,
        "template <typename T> class Widget { public: T value; };\n",
    )
    .expect("header");

    let selected = run(
        &root.0,
        json!({"operation":"match","reasoning":"test","path":header,"langType":"cpp","rule":"kind: class_specifier"}),
    )
    .expect("explicit C++ match");
    assert_eq!(selected["stats"]["totalStructuralMatches"], 1, "{selected}");

    let selected_directory = run(
        &root.0,
        json!({"operation":"match","reasoning":"test","path":root.0,"langType":"cpp","include":["*.h"],"rule":"kind: class_specifier"}),
    )
    .expect("explicit C++ directory match");
    assert_eq!(
        selected_directory["stats"]["totalStructuralMatches"], 1,
        "{selected_directory}"
    );

    let selected_directory_default = run(
        &root.0,
        json!({"operation":"match","reasoning":"test","path":root.0,"langType":"cpp","rule":"kind: class_specifier"}),
    )
    .expect("C++ directory includes ambiguous headers");
    assert_eq!(
        selected_directory_default["stats"]["totalStructuralMatches"], 1,
        "{selected_directory_default}"
    );

    let tree = run(
        &root.0,
        json!({"operation":"syntaxTree","reasoning":"test","path":header,"langType":"cpp"}),
    )
    .expect("explicit C++ tree");
    assert_eq!(tree["isPartial"], false, "{tree}");
    let default_tree = run(
        &root.0,
        json!({"operation":"syntaxTree","reasoning":"test","path":header}),
    )
    .expect(".h defaults to C");
    assert_eq!(default_tree["isPartial"], true, "{default_tree}");
    assert!(
        tree["nodes"]
            .as_array()
            .is_some_and(|nodes| nodes.iter().any(|node| node["kind"] == "class_specifier")),
        "{tree}"
    );

    let symbols = run(
        &root.0,
        json!({"operation":"symbols","reasoning":"test","path":header,"langType":"cpp"}),
    )
    .expect("explicit C++ symbols");
    assert!(
        symbols["declarations"]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|row| row["name"] == "Widget")),
        "{symbols}"
    );

    let wrong_file = root.0.join("wrong.c");
    std::fs::write(&wrong_file, "int value;\n").expect("C file");
    let mismatch = run(
        &root.0,
        json!({"operation":"match","reasoning":"test","path":wrong_file,"langType":"cpp","rule":"kind: declaration"}),
    )
    .expect_err("C source is not an ambiguous header");
    assert_eq!(mismatch.code, "ast.language.mismatch");
}

#[test]
fn directory_symbols_use_path_scoped_header_parser_and_preserve_it_in_next_page() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("include")).expect("include directory");
    std::fs::create_dir_all(root.0.join("legacy")).expect("legacy directory");
    std::fs::write(
        root.0.join("include/widget.h"),
        "namespace Space { class Widget {}; }\n",
    )
    .expect("cpp header");
    std::fs::write(root.0.join("legacy/plain.h"), "struct Plain { int x; };\n").expect("c header");
    let result = run(&root.0, json!({"operation":"symbols","reasoning":"test","path":root.0,"languageGlobs":{"cpp":["include/**/*.h"]},"pageSize":1})).expect("directory symbols");
    assert_eq!(result["filesScanned"], 2, "{result}");
    assert_eq!(
        result["next"]["nextPage"]["query"]["languageGlobs"]["cpp"][0],
        "include/**/*.h"
    );
    let next_query = result["next"]["nextPage"]["query"].clone();
    let next = run(&root.0, next_query).expect("next page");
    let names = [
        result["declarations"][0]["name"].as_str().unwrap_or(""),
        next["declarations"][0]["name"].as_str().unwrap_or(""),
    ];
    assert!(names.contains(&"Widget"), "{result} {next}");
    assert!(names.contains(&"Plain"), "{result} {next}");
}

#[test]
fn match_content_length_bounds_each_match_value() {
    let root = Fixture::new();
    let source = root.0.join("long.rs");
    let args = (0..200)
        .map(|i| format!("arg{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    std::fs::write(&source, format!("fn m() {{ call({args}); }}\n")).expect("source");
    let value_len = |length: Option<u32>| {
        let mut query =
            json!({"operation":"match","reasoning":"test","path":source,"pattern":"call($$$A)"});
        if let Some(length) = length {
            query["matchContentLength"] = json!(length);
        }
        let out = run(&root.0, query).expect("match");
        out["files"][0]["matches"][0]["value"]
            .as_str()
            .expect("value")
            .chars()
            .count()
    };
    assert_eq!(value_len(Some(40)), 40);
    assert!(value_len(Some(5_000)) > 300);
    assert_eq!(value_len(None), 500);
}

#[test]
fn unknown_symbol_kinds_are_rejected_and_source_limits_are_errors() {
    let root = Fixture::new();
    let source = root.0.join("lib.rs");
    std::fs::write(&source, "pub fn visible() {}\n").expect("source");
    let error = run(
        &root.0,
        json!({"operation":"symbols","reasoning":"test","path":source,"kinds":["functoin"]}),
    )
    .expect_err("unknown kind");
    assert_eq!(error.code, "ast.symbols.invalidKind");
    let ok = run(
        &root.0,
        json!({"operation":"symbols","reasoning":"test","path":source,"kinds":["function"]}),
    )
    .expect("known kind");
    assert_eq!(ok["declarations"][0]["name"], "visible");

    let large = root.0.join("large.rs");
    std::fs::write(&large, "// x\n".repeat(250_001)).expect("large");
    for query in [
        json!({"operation":"symbols","reasoning":"test","path":large}),
        json!({"operation":"syntaxTree","reasoning":"test","path":large}),
    ] {
        let out = run(&root.0, query).expect("limit row");
        assert_eq!(out["errorCode"], "ast.source.limit", "{out}");
        assert_eq!(out["status"], "error", "{out}");
    }
}

#[test]
fn single_file_symbols_are_compact_and_path_free() {
    let root = Fixture::new();
    let source = root.0.join("shapes.rs");
    std::fs::write(
        &source,
        "struct A;\nimpl A {\n    fn run() {}\n}\nstruct B; impl B { fn run() {} }\n",
    )
    .expect("source");
    let out = run(
        &root.0,
        json!({"operation":"symbols","reasoning":"test","path":source}),
    )
    .expect("symbols");
    let text = out.to_string();
    let root_text = root.0.to_string_lossy();
    assert!(
        !text.contains(root_text.as_ref()),
        "absolute path leaked: {out}"
    );
    assert!(out.get("complete").is_none(), "{out}");
    assert_eq!(out["isPartial"], false, "{out}");
    let rows = out["declarations"].as_array().expect("declarations");
    for row in rows {
        for dropped in ["path", "range", "selectionRange"] {
            assert!(row.get(dropped).is_none(), "{dropped} in {row}");
        }
    }
    let impl_a = rows
        .iter()
        .find(|r| r["kind"] == "impl" && r["line"] == 2)
        .expect("impl A");
    assert_eq!(impl_a["endLine"], 4, "{impl_a}");
    let run_a = rows
        .iter()
        .find(|r| r["name"] == "run" && r["line"] == 3)
        .expect("run in impl A");
    assert_eq!(run_a["character"], 7, "{run_a}");
    assert_eq!(run_a["id"], "run@3:7", "{run_a}");
    assert!(run_a.get("endLine").is_none(), "{run_a}");
    assert_eq!(run_a["parent"], impl_a["id"], "{out}");
    // Same name on one line stays distinguishable by column.
    let line5_runs = rows
        .iter()
        .filter(|r| r["name"] == "run" && r["line"] == 5)
        .count();
    assert_eq!(line5_runs, 1, "{out}");
    let mut ids = rows
        .iter()
        .filter_map(|r| r["id"].as_str())
        .collect::<Vec<_>>();
    let total = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), total, "ids must be unique: {out}");
    let notes = out["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .filter(|d| {
            d["message"]
                .as_str()
                .is_some_and(|m| m.contains("syntax-only"))
        })
        .count();
    assert_eq!(notes, 1, "{out}");
}

#[test]
fn directory_symbols_keep_row_paths_and_emit_the_syntax_note_once() {
    let root = Fixture::new();
    std::fs::write(root.0.join("a.rs"), "fn a() {}\n").expect("a");
    std::fs::write(root.0.join("b.rs"), "fn b() {}\n").expect("b");
    let out = run(
        &root.0,
        json!({"operation":"symbols","reasoning":"test","path":root.0}),
    )
    .expect("symbols");
    let rows = out["declarations"].as_array().expect("declarations");
    assert_eq!(rows.len(), 2, "{out}");
    assert!(rows.iter().all(|r| r["path"].is_string()), "{out}");
    let diagnostics = out["diagnostics"].as_array().expect("diagnostics");
    assert_eq!(diagnostics.len(), 1, "{out}");
    assert!(diagnostics[0].get("path").is_none(), "{out}");
}

#[test]
fn match_rows_emit_captures_once_and_omit_single_line_end() {
    let root = Fixture::new();
    let source = root.0.join("calls.ts");
    std::fs::write(&source, "oldCall(one);\noldCall(\n  two\n);\n").expect("source");
    let out = run(
        &root.0,
        json!({"operation":"match","reasoning":"test","path":source,"pattern":"oldCall($A)"}),
    )
    .expect("match");
    let matches = out["files"][0]["matches"].as_array().expect("matches");
    assert_eq!(matches.len(), 2, "{out}");
    for m in matches {
        assert!(m.get("metavars").is_none(), "duplicate capture map: {m}");
    }
    let single = &matches[0];
    assert!(single.get("endLine").is_none(), "{single}");
    assert_eq!(single["metavarRanges"]["A"][0]["text"], "one", "{single}");
    assert!(
        single["metavarRanges"]["A"][0].get("endLine").is_none(),
        "{single}"
    );
    assert_eq!(matches[1]["endLine"], 4, "{}", matches[1]);
}

#[test]
fn list_captures_default_to_one_span_row_and_expand_on_request() {
    let root = Fixture::new();
    let source = root.0.join("body.rs");
    std::fs::write(
        &source,
        "fn a() -> u8 {\n    let x = 1;\n    let y = 2;\n    x + y\n}\n",
    )
    .expect("source");
    let query = json!({"operation":"match","reasoning":"test","path":source,
        "pattern":"fn $N() -> u8 { $$$B }"});
    let out = run(&root.0, query.clone()).expect("match");
    let m = &out["files"][0]["matches"][0];
    let body = m["metavarRanges"]["B"].as_array().expect("B ranges");
    assert_eq!(body.len(), 1, "list capture must collapse to one span: {m}");
    assert_eq!(body[0]["count"], 3, "{m}");
    assert_eq!(body[0]["line"], 2, "{m}");
    assert_eq!(body[0]["endLine"], 4, "{m}");
    assert!(
        body[0].get("text").is_none(),
        "no body dump by default: {m}"
    );
    assert_eq!(m["capturesTruncated"], true, "{m}");
    assert!(out["next"]["expandCaptures"].is_object(), "{out}");
    // Single captures keep their text.
    assert_eq!(m["metavarRanges"]["N"][0]["text"], "a", "{m}");

    let mut expanded = query;
    expanded["captureText"] = json!(true);
    let out = run(&root.0, expanded).expect("match");
    let body = out["files"][0]["matches"][0]["metavarRanges"]["B"]
        .as_array()
        .expect("B ranges");
    assert_eq!(body.len(), 3, "{out}");
    assert_eq!(body[0]["text"], "let x = 1;", "{out}");
}

#[test]
fn rust_item_pattern_without_visibility_notes_that_pub_items_are_excluded() {
    let root = Fixture::new();
    let source = root.0.join("lib.rs");
    std::fs::write(
        &source,
        "pub fn a() -> Result<u8, String> { Ok(1) }\nfn b() -> Result<u8, String> { Ok(2) }\n",
    )
    .expect("source");
    let out = run(
        &root.0,
        json!({"operation":"match","reasoning":"test","path":source,
            "pattern":"fn $N() -> Result<$T, String> { $$$B }"}),
    )
    .expect("match");
    // ast-grep semantics: the modifier is a named child, so `pub fn` is not
    // matched. The result must say so instead of implying full coverage.
    assert_eq!(out["stats"]["totalStructuralMatches"], 1, "{out}");
    let notes = out["diagnostics"].as_array().expect("diagnostics");
    assert!(
        notes
            .iter()
            .any(|d| d["code"] == "structural.pattern.visibilityExact"),
        "{out}"
    );
    // A pattern that already names the visibility gets no note.
    let out = run(
        &root.0,
        json!({"operation":"match","reasoning":"test","path":source,
            "pattern":"pub fn $N() -> Result<$T, String> { $$$B }"}),
    )
    .expect("match");
    assert!(!out.to_string().contains("visibilityExact"), "{out}");
}

#[test]
fn yaml_rule_compile_errors_get_a_rule_hint_not_a_pattern_hint() {
    let root = Fixture::new();
    let source = root.0.join("lib.rs");
    std::fs::write(&source, "fn a() {}\n").expect("source");
    let error = run(
        &root.0,
        json!({"operation":"match","reasoning":"test","path":source,
            "rule":"rule:\n  kindx: function_item\n"}),
    )
    .expect_err("invalid rule");
    assert_eq!(error.code, "structural.query.compileFailed");
    let hint = error.hints.join(" ");
    assert!(hint.contains("rule"), "{hint}");
    assert!(!hint.contains("add `;`"), "{hint}");
}
