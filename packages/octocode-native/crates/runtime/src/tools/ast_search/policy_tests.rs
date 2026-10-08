use super::{AstResult, AstSearchQuery, execute_ast};
use crate::tools::cancel::NeverCancel;
use crate::tools::result::ToolError;
use crate::tools::test_support::{AfterRoot, Fixture, sensitive_fixture, simple_policy};
use crate::{
    policy::path::PathPolicy, security::ContentSecurity, tools::cancel::CancellationCheck,
};
use serde_json::json;
use std::path::PathBuf;

/// Tests speak JSON rows, or a continuation's complete input (its one row);
/// the runtime owns the typed parse.
fn execute_row(
    query: serde_json::Value,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancellation: &dyn CancellationCheck,
) -> AstResult {
    let row = query
        .get("queries")
        .map_or(query.clone(), |rows| rows[0].clone());
    let query: AstSearchQuery = serde_json::from_value(row).expect("typed astSearch row");
    execute_ast(&query, paths, security, cancellation)
}

/// A match row `{line, column, endLine?, value}` as (line, endLine, value).
fn lean_row(row: &serde_json::Value) -> (u64, Option<u64>, &str) {
    (
        row["line"].as_u64().expect("match row line"),
        row["endLine"].as_u64(),
        row["value"].as_str().expect("match row value"),
    )
}

/// The declaration names of an outline row (an entry string, or a container
/// and its nested members), in source order.
fn outline_row_names(row: &serde_json::Value) -> Vec<&str> {
    if let Some(text) = row.as_str() {
        assert!(
            crate::tools::symbol_outline::parse_entry(text).is_some(),
            "outline entry: {text}"
        );
        return vec![&text[..text.rfind(" (").expect("entry fields")]];
    }
    let mut names = vec![row["symbolName"].as_str().expect("declaration row")];
    for member in row["members"].as_array().into_iter().flatten() {
        names.extend(outline_row_names(member));
    }
    names
}

/// Every expansion was followed: at most the read of the hits remains.
fn only_read_left(out: &serde_json::Value) -> bool {
    out["next"]
        .as_object()
        .is_none_or(|next| next.keys().all(|name| name == "read"))
}

/// The first declaration name of an outline row.
fn outline_name(row: &serde_json::Value) -> &str {
    outline_row_names(row)[0]
}

fn outline_names(rows: &serde_json::Value) -> Vec<&str> {
    rows.as_array()
        .expect("outline rows")
        .iter()
        .flat_map(outline_row_names)
        .collect()
}

#[test]
fn match_pagination_ceilings_preserve_rows_and_disclose_terminal_limits() {
    let root = Fixture::new();
    for i in 0..1001 {
        std::fs::write(root.0.join(format!("f{i:04}.ts")), "console.log(1);\n").expect("file");
    }
    let (paths, security) = simple_policy(&root.0);
    let base = json!({"operation":"match","path":root.0,"pattern":"console.log($A)","language":"typescript","pageSize":1,"maxFiles":1500,"mainGoal":"test","reasoning":"test"});
    let first = execute_row(base.clone(), &paths, &security, &NeverCancel).expect("first page");
    let mut last = first["next"]["nextPage"]["query"]["queries"][0].clone();
    last["page"] = json!(1000);
    let page = execute_row(last, &paths, &security, &NeverCancel).expect("ceiling page");
    assert_eq!(page["files"].as_array().expect("files").len(), 1, "{page}");
    assert!(page["next"].get("nextPage").is_none(), "{page}");
    assert_eq!(page["terminalLimit"], true, "{page}");

    let file = root.0.join("rows.ts");
    std::fs::write(&file, "console.log(1);\n".repeat(1001)).expect("match rows");
    let first_rows = execute_row(json!({"operation":"match","path":file,"pattern":"console.log($A)","language":"typescript","matchPageSize":1,"mainGoal":"test","reasoning":"test"}), &paths, &security, &NeverCancel).expect("first match page");
    let mut last_rows = first_rows["next"]["nextMatchPage"]["query"]["queries"][0].clone();
    last_rows["matchPage"] = json!(1000);
    let row_page = execute_row(last_rows, &paths, &security, &NeverCancel).expect("match ceiling");
    assert_eq!(
        row_page["files"][0]["matches"]
            .as_array()
            .expect("rows")
            .len(),
        1,
        "{row_page}"
    );
    assert!(
        row_page["next"].get("nextMatchPage").is_none(),
        "{row_page}"
    );
    assert_eq!(row_page["terminalLimit"], true, "{row_page}");
}

#[test]
fn descendant_policy_precedes_discovery_totals_and_line_reads() {
    let root = sensitive_fixture();
    let (paths, security) = simple_policy(&root.0);
    let symbols = execute_row(
        json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":root.0}),
        &paths,
        &security,
        &NeverCancel,
    )
    .expect("symbols");
    assert_eq!(symbols["filesScanned"], 1);
    assert_eq!(symbols["filesSkipped"], 0);
    assert_eq!(outline_name(&symbols["files"][0]["symbols"][0]), "visible");

    std::fs::write(root.0.join(".aws/hidden.ts"), "oldCall(secret);\n").expect("hidden ast");
    std::fs::write(root.0.join(".env.ts"), "oldCall(ignored);\n").expect("ignored ast");
    std::fs::write(root.0.join("visible.ts"), "oldCall(visible);\n").expect("visible ast");
    let matches = execute_row(
        json!({
            "operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":"typescript",
            "pattern":"oldCall($A)","hidden":true
        }),
        &paths,
        &security,
        &NeverCancel,
    )
    .expect("structural matches");
    assert_eq!(matches["stats"]["matchCount"], 1);
    assert_eq!(matches["files"].as_array().expect("files").len(), 1);
    let root_name = root.0.file_name().expect("root name").to_string_lossy();
    assert_eq!(
        matches["files"][0]["path"],
        format!("{root_name}/visible.ts")
    );
    assert!(!matches.to_string().contains("hidden.ts"));
    assert!(!matches.to_string().contains(".env.ts"));
}

/// A directory scan whose path policy withheld candidate entries says so,
/// as localSearch and structureSearch do: an empty or "complete" result
/// over a `secrets/` directory is not proof of absence.
#[test]
fn directory_scans_disclose_policy_withheld_entries() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("secrets")).expect("secrets dir");
    std::fs::write(root.0.join("secrets/store.ts"), "register(secret);\n").expect("withheld");
    std::fs::write(
        root.0.join("visible.ts"),
        "register(visible);\nexport function shown() {}\n",
    )
    .expect("visible");
    let disclosed = |out: &serde_json::Value| {
        out["warnings"].as_array().is_some_and(|warnings| {
            warnings.iter().any(|w| {
                w.as_str().is_some_and(|w| {
                    w.contains("withheld by path policy: security-policy dirs secrets/")
                })
            })
        })
    };
    let matches = run(
        &root.0,
        json!({"operation":"match","path":root.0,"language":"typescript","pattern":"register($A)"}),
    )
    .expect("match");
    assert_eq!(matches["stats"]["matchCount"], 1, "{matches}");
    assert!(disclosed(&matches), "{matches}");
    assert!(matches.get("complete").is_none(), "{matches}");
    let empty = run(
        &root.0,
        json!({"operation":"match","path":root.0,"language":"typescript","pattern":"register(secret)"}),
    )
    .expect("empty match");
    assert!(disclosed(&empty), "{empty}");
    let symbols = run(&root.0, json!({"operation":"symbols","path":root.0})).expect("symbols");
    assert!(disclosed(&symbols), "{symbols}");
}

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
    let guidance = missing["diagnostics"][0]["message"]
        .as_str()
        .expect("no-match guidance");
    assert!(guidance.contains("operation:\"syntaxTree\""), "{guidance}");

    // Trailing punctuation the pattern omits is not required (ast-grep
    // smart strictness, as astRewrite matches).
    for pattern in ["const $A = $B;", "const $A = $B"] {
        let found = execute_row(
            json!({"operation":"match","mainGoal":"test","reasoning":"test","path":source,"pattern":pattern}),
            &paths,
            &security,
            &NeverCancel,
        )
        .expect("structural match");
        assert_eq!(found["stats"]["matchCount"], 1, "{found}");
        assert!(found.get("status").is_none(), "{found}");
    }
}

#[test]
fn directory_prefilter_skips_are_aggregated_once() {
    let root = Fixture::new();
    for name in ["one.ts", "two.ts", "three.ts"] {
        std::fs::write(root.0.join(name), "const value = 1;\n").expect("source");
    }
    let (paths, security) = simple_policy(&root.0);
    let result = execute_row(
        json!({
            "operation":"match","mainGoal":"test","reasoning":"test",
            "path":root.0,
            "language":"typescript",
            "pattern":"missingCall($A);"
        }),
        &paths,
        &security,
        &NeverCancel,
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
    let root = Fixture::new();
    std::fs::write(root.0.join("source.rs"), "source").expect("source");
    let (paths, security) = simple_policy(&root.0);
    let error = execute_row(
        json!({
            "operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":"rust",
            "pattern":"source($A)"
        }),
        &paths,
        &security,
        &AfterRoot(std::sync::atomic::AtomicUsize::new(0)),
    )
    .expect_err("cancel structural walk");
    assert_eq!(error.code, "cancelled");
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
    let (paths, security) = simple_policy(&root.0);
    let out = execute_row(
        json!({
            "operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":"rust",
            "pattern":"pub fn source() {}","maxFiles":1
        }),
        &paths,
        &security,
        &NeverCancel,
    )
    .expect("match runs");

    assert_eq!(
        out["truncated"],
        json!(true),
        "capped scan must set truncated=true; got {out}"
    );
    assert_eq!(
        out["isPartial"],
        json!(true),
        "capped scan is partial; got {out}"
    );
    let diagnostics = out["diagnostics"].as_array().cloned().unwrap_or_default();
    assert!(
        diagnostics
            .iter()
            .any(|d| d["code"] == json!("structural.scan.truncated")),
        "expected a structural.scan.truncated diagnostic; got {out}"
    );
}

/// A `maxFiles` cut below the schema maximum is a raisable bound, not a
/// terminal limit: match and symbols pages carry `next.expandScan`, which
/// doubles the bound from page 1, and a warning naming the cut.
#[test]
fn a_raisable_max_files_cut_offers_an_expanded_scan() {
    let root = Fixture::new();
    for name in ["a.rs", "b.rs", "c.rs"] {
        std::fs::write(root.0.join(name), "pub fn source() {}\n").expect("source file");
    }
    let rows = [
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":"rust",
            "pattern":"pub fn source() {}","maxFiles":1}),
        json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":root.0,"maxFiles":1}),
    ];
    for query in rows {
        let out = run(&root.0, query.clone()).expect("scan");
        assert!(out.get("terminalLimit").is_none(), "{out}");
        let expand = &out["next"]["expandScan"]["query"]["queries"][0];
        assert_eq!(expand["maxFiles"], 2, "{out}");
        assert_eq!(expand["page"], 1, "{out}");
        assert!(
            expand
                .get("snapshot")
                .is_none_or(serde_json::Value::is_null),
            "{out}"
        );
        assert!(
            out["diagnostics"]
                .as_array()
                .is_some_and(|diagnostics| diagnostics
                    .iter()
                    .any(|d| d["code"] == "structural.scan.truncated")),
            "{out}"
        );
        let expanded = run(&root.0, expand.clone()).expect("expanded scan");
        assert_eq!(
            expanded["next"]["expandScan"]["query"]["queries"][0]["maxFiles"], 4,
            "{expanded}"
        );
        let whole = {
            let mut whole = query.clone();
            whole["maxFiles"] = json!(4);
            run(&root.0, whole).expect("whole scan")
        };
        assert!(whole["next"].get("expandScan").is_none(), "{whole}");
        assert!(whole.get("terminalLimit").is_none(), "{whole}");
    }
}

#[test]
fn match_continuation_rejects_stale_snapshot() {
    let root = Fixture::new();
    for name in ["a.rs", "b.rs", "c.rs", "d.rs"] {
        std::fs::write(root.0.join(name), "pub fn source() {}\n").expect("file");
    }
    let (paths, security) = simple_policy(&root.0);
    let page1 = execute_row(
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":"rust","pattern":"pub fn source() {}","pageSize":2}),
        &paths,
        &security,
        &NeverCancel,
    )
    .expect("page1");
    let snapshot = page1["snapshot"].as_str().expect("snapshot").to_string();
    std::fs::write(root.0.join("e.rs"), "pub fn source() {}\n").expect("mutate corpus");
    let page2 = execute_row(
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":"rust","pattern":"pub fn source() {}","pageSize":2,"page":2,"snapshot":snapshot}),
        &paths,
        &security,
        &NeverCancel,
    )
    .expect("page2");
    assert_eq!(page2["errorCode"], json!("staleSnapshot"), "got {page2}");
    // Never a dead end: the restart is page 1 of the same query, unpinned.
    let restart = &page2["next"]["restart"]["query"]["queries"][0];
    assert_eq!(restart["page"], 1, "{page2}");
    assert!(restart.get("snapshot").is_none(), "{page2}");
    let fresh = run(&root.0, restart.clone()).expect("restart runs");
    assert_eq!(fresh["pagination"]["totalItems"], 5, "{fresh}");
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
        assert_eq!(error.code, "invalidPattern");
        assert!(!error.message.starts_with('['), "{}", error.message);
    }
}

#[test]
fn a_match_offers_a_read_of_the_top_files_hits() {
    let root = Fixture::new();
    std::fs::write(
        root.0.join("a.rs"),
        "fn a() {\n    hit(1);\n}\n\n\n\n\n\n\n\n\nfn b() { hit(2); }\n",
    )
    .expect("a");
    let out = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":"rust","pattern":"hit($A)"}),
    )
    .expect("match");
    let read = &out["next"]["read"];
    assert_eq!(read["tool"], "localFetch", "{out}");
    let row = &read["query"]["queries"][0];
    assert_eq!(row["ranges"], json!(["1-5", "9-15"]), "{out}");
    assert!(
        row["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("a.rs") && std::path::Path::new(path).is_absolute()),
        "{out}"
    );
    crate::contracts::validate_query("localFetch", row.clone()).expect("a valid localFetch row");
}

fn run(root: &std::path::Path, query: serde_json::Value) -> Result<serde_json::Value, ToolError> {
    let (paths, security) = simple_policy(root);
    execute_row(query, &paths, &security, &NeverCancel)
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
    assert_eq!(error.code, "invalidPattern");
}

#[test]
fn match_pagination_limits_next_match_page_to_the_current_file_page() {
    let root = Fixture::new();
    std::fs::write(root.0.join("a.rs"), "fn a() { hit(1); }\n").expect("a");
    std::fs::write(root.0.join("b.rs"), "fn b() { hit(1); hit(2); hit(3); }\n").expect("b");
    let base = json!({
        "operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":"rust","pattern":"hit($A)",
        "pageSize":1,"matchPageSize":2
    });
    let page1 = run(&root.0, base.clone()).expect("page1");
    // A file whose matches all fit on its match page carries no row counts.
    assert_eq!(
        page1["files"][0]["matches"].as_array().map(Vec::len),
        Some(1),
        "{page1}"
    );
    assert!(
        page1["files"][0].get("totalMatchRows").is_none()
            && page1["files"][0].get("returnedMatchRows").is_none(),
        "{page1}"
    );
    assert!(
        page1["next"].get("nextMatchPage").is_none(),
        "page 1 has no truncated file, so no nextMatchPage: {page1}"
    );
    let next = page1["next"]["nextPage"]["query"]["queries"][0].clone();
    let page2 = run(&root.0, next).expect("page2");
    assert_eq!(page2["files"][0]["returnedMatchRows"], 2, "{page2}");
    assert!(
        page2["next"]["nextMatchPage"].is_object(),
        "unreturned matches remain: {page2}"
    );
    let deeper = page2["next"]["nextMatchPage"]["query"]["queries"][0].clone();
    assert_eq!(deeper["matchPage"], 2, "{page2}");
    assert_eq!(deeper["page"], 2, "{page2}");
    let page2b = run(&root.0, deeper.clone()).expect("page2 matchPage2");
    assert_eq!(page2b["files"][0]["returnedMatchRows"], 1, "{page2b}");

    // A nextPage continuation always restarts per-file match pagination.
    let mut mid = base;
    mid["matchPage"] = json!(2);
    mid["snapshot"] = page1["snapshot"].clone();
    let mid = run(&root.0, mid).expect("page1 matchPage2");
    assert_eq!(
        mid["next"]["nextPage"]["query"]["queries"][0]["matchPage"], 1,
        "{mid}"
    );
}

#[test]
fn single_file_with_unreturned_matches_is_not_complete() {
    let root = Fixture::new();
    let source = root.0.join("many.rs");
    std::fs::write(&source, "fn m() { hit(1); hit(2); hit(3); }\n").expect("source");
    let out = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":source,"pattern":"hit($A)","matchPageSize":2}),
    )
    .expect("match");
    assert!(out["next"]["nextMatchPage"].is_object(), "{out}");
}

#[test]
fn lang_type_is_validated_and_intersected_with_include() {
    let root = Fixture::new();
    std::fs::write(root.0.join("a.ts"), "oldCall(x);\n").expect("ts");
    std::fs::write(root.0.join("b.py"), "oldCall(x)\n").expect("py");
    let unknown = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":"klingon","pattern":"oldCall($A)"}),
    )
    .expect_err("unknown language");
    assert_eq!(unknown.code, "languageUnsupported");

    let out = run(
        &root.0,
        json!({
            "operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":"typescript",
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
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0.join("b.py"),"language":"typescript","pattern":"oldCall($A)"}),
    )
    .expect_err("single file outside language");
    assert_eq!(mismatch.code, "languageMismatch");

    std::fs::write(root.0.join("notes.txt"), "oldCall(x)\n").expect("txt");
    let unsupported = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0.join("notes.txt"),"pattern":"oldCall($A)"}),
    )
    .expect("unsupported single file");
    assert_eq!(unsupported["isPartial"], true, "{unsupported}");
    assert_ne!(unsupported["status"], "empty", "{unsupported}");
}

#[test]
fn dot_prefixed_lang_type_selects_only_that_extension() {
    let root = Fixture::new();
    std::fs::write(root.0.join("a.ts"), "oldCall(x);\n").expect("ts");
    std::fs::write(root.0.join("b.mts"), "oldCall(y);\n").expect("mts");

    let exact = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":".ts","pattern":"oldCall($A)"}),
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
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":"typescript","pattern":"oldCall($A)"}),
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
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":header,"language":"cpp","rule":"kind: class_specifier"}),
    )
    .expect("explicit C++ match");
    assert_eq!(selected["stats"]["matchCount"], 1, "{selected}");

    let selected_directory = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":"cpp","include":["*.h"],"rule":"kind: class_specifier"}),
    )
    .expect("explicit C++ directory match");
    assert_eq!(
        selected_directory["stats"]["matchCount"], 1,
        "{selected_directory}"
    );

    let selected_directory_default = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":"cpp","rule":"kind: class_specifier"}),
    )
    .expect("C++ directory includes ambiguous headers");
    assert_eq!(
        selected_directory_default["stats"]["matchCount"], 1,
        "{selected_directory_default}"
    );

    let tree = run(
        &root.0,
        json!({"operation":"syntaxTree","mainGoal":"test","reasoning":"test","path":header,"language":"cpp"}),
    )
    .expect("explicit C++ tree");
    assert_eq!(tree["isPartial"], false, "{tree}");
    let default_tree = run(
        &root.0,
        json!({"operation":"syntaxTree","mainGoal":"test","reasoning":"test","path":header}),
    )
    .expect(".h defaults to C");
    assert_eq!(default_tree["isPartial"], true, "{default_tree}");
    assert!(
        tree["nodes"].as_array().is_some_and(|nodes| nodes
            .iter()
            .any(|node| node.as_str().and_then(|row| row.split(' ').nth(1))
                == Some("class_specifier"))),
        "{tree}"
    );

    let symbols = run(
        &root.0,
        json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":header,"language":"cpp"}),
    )
    .expect("explicit C++ symbols");
    assert!(
        outline_names(&symbols["symbols"]).contains(&"Widget"),
        "{symbols}"
    );

    let wrong_file = root.0.join("wrong.c");
    std::fs::write(&wrong_file, "int value;\n").expect("C file");
    let mismatch = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":wrong_file,"language":"cpp","rule":"kind: declaration"}),
    )
    .expect_err("C source is not an ambiguous header");
    assert_eq!(mismatch.code, "languageMismatch");
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
    let result = run(&root.0, json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":root.0,"languageGlobs":{"cpp":["include/**/*.h"]},"pageSize":1})).expect("directory symbols");
    assert_eq!(result["filesScanned"], 2, "{result}");
    assert_eq!(
        result["next"]["nextPage"]["query"]["queries"][0]["languageGlobs"]["cpp"][0],
        "include/**/*.h"
    );
    let next_query = result["next"]["nextPage"]["query"]["queries"][0].clone();
    let next = run(&root.0, next_query).expect("next page");
    let names = [
        outline_name(&result["files"][0]["symbols"][0]),
        outline_name(&next["files"][0]["symbols"][0]),
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
        let mut query = json!({"operation":"match","mainGoal":"test","reasoning":"test","path":source,"pattern":"call($$$A)"});
        if let Some(length) = length {
            query["matchContentLength"] = json!(length);
        }
        let out = run(&root.0, query).expect("match");
        lean_row(&out["files"][0]["matches"][0]).2.chars().count()
    };
    assert_eq!(value_len(Some(40)), 40);
    assert!(value_len(Some(5_000)) > 300);
    assert_eq!(value_len(None), 500);
}

/// The localFetch read rows of every `expandValues*` lead, in order.
fn value_reads(out: &serde_json::Value) -> Vec<serde_json::Value> {
    let mut reads = out["next"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(name, _)| name.starts_with("expandValues"))
        .map(|(name, read)| {
            assert_eq!(read["tool"], "localFetch", "{name}: {read}");
            let row = read["query"]["queries"][0].clone();
            crate::contracts::validate_query("localFetch", row.clone())
                .expect("a valid localFetch row");
            row
        })
        .collect::<Vec<_>>();
    reads.sort_by_key(|row| row["path"].as_str().map(str::to_owned));
    reads
}

/// A value cut at `matchContentLength` is display clipping, not a coverage
/// gap: every match is listed, so the row is not partial. The cut is never
/// silent: the shown text ends in `…`, an object row says `truncated`, and
/// `next.expandValues` reads exactly the clipped rows' lines whole.
#[test]
fn clipped_values_do_not_mark_the_row_partial() {
    let root = Fixture::new();
    let source = root.0.join("long.rs");
    let args = (0..200)
        .map(|i| format!("arg{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    std::fs::write(
        &source,
        format!("fn n() {{ call(x); }}\nfn m() {{\n    call({args});\n}}\n"),
    )
    .expect("source");
    let out = run(
        &root.0,
        json!({"operation":"match","path":source,"pattern":"call($$$A)","captureText":true}),
    )
    .expect("match");
    assert!(out.get("isPartial").is_none(), "{out}");
    assert!(out.get("partialReasons").is_none(), "{out}");
    let rows = out["files"][0]["matches"].as_array().expect("rows");
    assert_eq!(rows.len(), 2, "{out}");
    assert!(rows[0].get("truncated").is_none(), "{out}");
    assert_eq!(rows[1]["truncated"], true, "{out}");
    assert!(
        rows[1]["value"].as_str().is_some_and(|v| v.ends_with('…')),
        "{out}"
    );
    let reads = value_reads(&out);
    assert_eq!(reads.len(), 1, "{out}");
    assert_eq!(reads[0]["ranges"], json!(["3-3"]), "{out}");
    assert!(
        reads[0]["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("long.rs")),
        "{out}"
    );
    // The lean view hides the `$$$A` capture as well: one expandCaptures
    // re-run returns captures and whole values; it is still not partial.
    let lean = run(
        &root.0,
        json!({"operation":"match","path":source,"pattern":"call($$$A)"}),
    )
    .expect("lean match");
    assert!(lean.get("isPartial").is_none(), "{lean}");
    let expand = &lean["next"]["expandCaptures"]["query"]["queries"][0];
    assert_eq!(expand["captureText"], true, "{lean}");
    let full = format!("call({args})");
    assert_eq!(expand["matchContentLength"], full.chars().count(), "{lean}");
    let expanded = run(&root.0, expand.clone()).expect("expanded");
    assert_eq!(expanded["files"][0]["matches"][1]["value"], full.as_str());
    assert!(only_read_left(&expanded), "{expanded}");
}

/// A match row with every file and match page shown and no scope gap is
/// `complete`: its expansions (`expandCaptures`, `expandValues*`) and the
/// hits read are drill-downs, so the response stage neither marks it partial
/// nor lets the CLI exit as "more to read". A file page left, or a scope gap,
/// keeps it open.
#[test]
fn match_rows_without_unread_pages_are_complete() {
    let root = Fixture::new();
    let long = format!("call({});\n", "y".repeat(3000));
    std::fs::write(root.0.join("a.ts"), &long).expect("a");
    std::fs::write(root.0.join("b.ts"), "call(1);\n").expect("b");
    let base =
        json!({"operation":"match","path":root.0,"pattern":"call($$$A)","language":"typescript"});
    let out = run(&root.0, base.clone()).expect("match");
    assert_eq!(out["complete"], true, "{out}");
    assert!(out["next"].get("expandCaptures").is_some(), "{out}");
    let row = crate::response::rows::result_row(
        crate::tools::id::ToolId::AstSearch,
        0,
        &base,
        out.clone(),
        None,
    );
    assert!(row.pointer("/data/isPartial").is_none(), "{row}");
    let mut paged = base.clone();
    paged["pageSize"] = json!(1);
    let open = run(&root.0, paged).expect("first file page");
    assert!(open.get("complete").is_none(), "{open}");
    let mut capped = base;
    capped["maxFiles"] = json!(1);
    let gap = run(&root.0, capped).expect("capped scan");
    assert!(gap.get("complete").is_none(), "{gap}");
}

/// Clipped rows across files and past the read-range limit each stay
/// reachable: one `expandValues*` read per file and per range batch.
#[test]
fn clipped_values_read_per_file_in_range_batches() {
    let root = Fixture::new();
    let long = format!("call({});", "y".repeat(80));
    let many = (0..30)
        .map(|i| {
            if i % 2 == 0 {
                long.clone()
            } else {
                format!("// {i}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(root.0.join("a.rs"), format!("fn a() {{\n{many}\n}}\n")).expect("a");
    std::fs::write(root.0.join("b.rs"), format!("fn b() {{\n{long}\n}}\n")).expect("b");
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"language":"rust","pattern":"call($A)","captureText":true,"matchContentLength":20}),
    )
    .expect("match");
    assert!(out.get("isPartial").is_none(), "{out}");
    let reads = value_reads(&out);
    let ranges = reads
        .iter()
        .flat_map(|row| row["ranges"].as_array().cloned().unwrap_or_default())
        .collect::<Vec<_>>();
    // 15 clipped calls in a.rs (lines 2, 4, … 30) and one in b.rs (line 2).
    assert_eq!(ranges.len(), 16, "{out}");
    let max = crate::tools::local_fetch::MAX_READ_RANGES;
    assert!(
        reads
            .iter()
            .all(|row| row["ranges"].as_array().map_or(0, Vec::len) <= max),
        "{out}"
    );
    assert!(reads.len() >= 3, "{out}");
}

/// A value longer than the `matchContentLength` maximum still reads whole:
/// the lead is a line read, not a longer re-run that would cut again.
#[test]
fn values_past_the_content_maximum_read_their_lines() {
    let root = Fixture::new();
    let source = root.0.join("huge.rs");
    let args = (0..2_000)
        .map(|i| format!("arg{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    std::fs::write(&source, format!("fn m() {{\n    call({args});\n}}\n")).expect("source");
    let out = run(
        &root.0,
        json!({"operation":"match","path":source,"pattern":"call($$$A)","captureText":true}),
    )
    .expect("match");
    assert!(out.get("isPartial").is_none(), "{out}");
    assert_eq!(value_reads(&out)[0]["ranges"], json!(["2-2"]), "{out}");
    let codes = out["diagnostics"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|d| d["code"].as_str())
        .collect::<Vec<_>>();
    assert!(!codes.contains(&"structural.match.valueClipped"), "{out}");
}

/// A cut header row (a whole block) expands its captures and whole text in
/// one re-run; a lean cut without hidden captures reads its lines.
#[test]
fn block_and_lean_cuts_expand_to_the_whole_text() {
    let root = Fixture::new();
    let source = root.0.join("long.rs");
    std::fs::write(&source, "fn m() { call(1); }\nfn n() { call(x); }\n").expect("source");
    let lean = run(
        &root.0,
        json!({"operation":"match","path":source,"pattern":"fn n() { call(x); }","matchContentLength":8}),
    )
    .expect("lean cut");
    assert!(lean.get("isPartial").is_none(), "{lean}");
    assert!(lean["next"].get("expandCaptures").is_none(), "{lean}");
    assert_eq!(value_reads(&lean)[0]["ranges"], json!(["2-2"]), "{lean}");
    let block = root.0.join("block.rs");
    let body = (0..120)
        .map(|i| format!("    let v{i} = {i};"))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&block, format!("fn big() {{\n{body}\n}}\n")).expect("block");
    let out = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":block,"pattern":"fn $N() { $$$B }"}),
    )
    .expect("block match");
    assert!(value_reads(&out).is_empty(), "{out}");
    let expand = out["next"]["expandCaptures"]["query"]["queries"][0].clone();
    assert_eq!(expand["captureText"], true, "{out}");
    let expanded = run(&root.0, expand).expect("expanded block");
    let value = expanded["files"][0]["matches"][0]["value"]
        .as_str()
        .expect("value");
    assert!(value.ends_with("let v119 = 119; }"), "{value}");
    assert!(only_read_left(&expanded), "{expanded}");
}

#[test]
fn unknown_symbol_kinds_are_rejected_and_source_limits_are_errors() {
    let root = Fixture::new();
    let source = root.0.join("lib.rs");
    std::fs::write(&source, "pub fn visible() {}\n").expect("source");
    let error = run(
        &root.0,
        json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":source,"kinds":["functoin"]}),
    )
    .expect_err("unknown kind");
    // An `.input.invalid` code is an input rejection: CLI exit 2 and a
    // "correct the field" hint, never "broaden the query".
    assert_eq!(error.code, "invalidInput");
    assert!(crate::response::rows::is_invalid_input_code(&error.code));
    assert!(
        error.message.contains("function, impl"),
        "{}",
        error.message
    );
    let ok = run(
        &root.0,
        json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":source,"kinds":["function"]}),
    )
    .expect("known kind");
    assert_eq!(outline_name(&ok["symbols"][0]), "visible");

    let large = root.0.join("large.rs");
    std::fs::write(
        &large,
        "// x\n".repeat(super::MAX_PARSE_SOURCE_BYTES / 5 + 1),
    )
    .expect("large");
    for query in [
        json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":large}),
        json!({"operation":"syntaxTree","mainGoal":"test","reasoning":"test","path":large}),
    ] {
        let out = run(&root.0, query).expect("limit row");
        assert_eq!(out["errorCode"], "fileTooLarge", "{out}");
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
        json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":source}),
    )
    .expect("symbols");
    // Rows are path-free; a continuation names its file absolutely until the
    // envelope anchors it on the workspace.
    let mut rows = out.clone();
    rows.as_object_mut().map(|row| row.remove("next"));
    let text = rows.to_string();
    let root_text = root.0.to_string_lossy();
    assert!(
        !text.contains(root_text.as_ref()),
        "absolute path leaked: {out}"
    );
    assert_eq!(out["isPartial"], false, "{out}");
    // One declaration row each (rendered here as the text outline): no
    // path, ranges or engine ids; an endLine only when the declaration
    // spans lines; members indented under the `impl` that holds them.
    assert_eq!(
        json!(crate::tools::symbol_outline::outline_rows(
            &crate::tools::symbol_outline::flatten_members(
                out["symbols"].as_array().expect("rows")
            )
        )),
        json!([
            "1 struct A",
            "2-4 impl A",
            "  3 method run",
            "5 struct B",
            "5 impl B",
            "  5 method run"
        ]),
        "{out}"
    );
    // One page has nothing to pin, and the static syntax-only caveat is in
    // the tool description, not every response.
    assert!(out.get("snapshot").is_none(), "{out}");
    assert!(
        !out.to_string().contains("syntax-only"),
        "static caveat repeated: {out}"
    );
}

#[test]
fn cached_symbol_pages_still_check_content_and_path_policy() {
    let root = Fixture::new();
    let source = root.0.join("pages.ts");
    std::fs::write(
        &source,
        "export function one() {}\nexport function two() {}\n",
    )
    .expect("source");
    let first = run(
        &root.0,
        json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":source,"pageSize":1}),
    )
    .expect("first");
    let next = first["next"]["nextPage"]["query"]["queries"][0].clone();
    assert_eq!(
        outline_name(&run(&root.0, next.clone()).expect("second")["symbols"][0]),
        "two"
    );
    // Same byte length: cache invalidation cannot rely only on file size.
    std::fs::write(
        &source,
        "export function one() {}\nexport function six() {}\n",
    )
    .expect("changed");
    let stale = run(&root.0, next.clone()).expect("stale");
    assert_eq!(stale["errorCode"], "staleSnapshot");
    let restart = &stale["next"]["restart"];
    assert_eq!(restart["tool"], "astSearch");
    assert_eq!(restart["query"]["queries"][0]["operation"], "symbols");
    assert_eq!(restart["query"]["queries"][0]["pageSize"], 1);
    assert_eq!(restart["query"]["queries"][0]["page"], 1);
    assert!(restart["query"].get("snapshot").is_none());
    let restarted =
        run(&root.0, restart["query"].clone()).expect("execute returned restart unchanged");
    assert_eq!(outline_name(&restarted["symbols"][0]), "one");
    assert_ne!(restarted["snapshot"], first["snapshot"]);
    let second = run(
        &root.0,
        restarted["next"]["nextPage"]["query"]["queries"][0].clone(),
    )
    .expect("new snapshot continuation");
    assert_eq!(outline_name(&second["symbols"][0]), "six");
    let fresh = run(
        &root.0,
        json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":source,"pageSize":2}),
    )
    .expect("fresh");
    assert_eq!(outline_names(&fresh["symbols"])[1], "six");
    let forbidden = Fixture::new();
    assert!(
        run(&forbidden.0, next).is_err(),
        "warm extraction must not bypass root authorization"
    );
}

#[test]
fn symbols_outline_places_twin_members_and_columns_only_when_ambiguous() {
    let root = Fixture::new();
    let source = root.0.join("twins.rs");
    std::fs::write(
        &source,
        "struct A;\nimpl A {\n    fn one() {}\n}\nimpl A {\n    fn two() {}\n}\nfn x() {} fn x() {}\n",
    )
    .expect("source");
    let out = run(
        &root.0,
        json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":source}),
    )
    .expect("symbols");
    // The two `impl A` blocks share one row listing both ranges, so `two`
    // (line 6) sits in the second; the two `x` on one line differ only by
    // column, so only they carry it.
    assert_eq!(
        json!(crate::tools::symbol_outline::outline_rows(
            &crate::tools::symbol_outline::flatten_members(
                out["symbols"].as_array().expect("rows")
            )
        )),
        json!([
            "1 struct A",
            "2-4,5-7 impl A",
            "  3 method one; 6 two",
            "8 function x col 4; 8 x col 14"
        ]),
        "{out}"
    );
    // A page that starts below the parent names it with its line.
    let second = run(
        &root.0,
        json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":source,"pageSize":4,"page":2,
            "snapshot":out_snapshot(&root.0, &source)}),
    )
    .expect("second page");
    assert_eq!(
        crate::tools::symbol_outline::outline_rows(&crate::tools::symbol_outline::flatten_members(
            second["symbols"].as_array().expect("rows")
        ))[0],
        "6 method two (in A@5)",
        "{second}"
    );
}

#[test]
fn symbols_trait_impls_name_their_trait_and_macro_members_nest() {
    // tokio named_pipe.rs: five `impl … NamedPipeServer` blocks must say
    // which trait each implements, and `cfg_io_util! { pub fn … }` inside an
    // impl belongs to that impl, in source order.
    let root = Fixture::new();
    let source = root.0.join("pipe.rs");
    std::fs::write(
        &source,
        "pub struct Pipe;\nimpl Pipe {\n    pub fn read() {}\n    cfg_io_util! {\n        pub fn read_buf() {}\n    }\n}\nimpl AsyncWrite for Pipe {\n    fn poll_flush() {}\n}\nimpl<T: Clone> std::fmt::Debug for Wrap<T> {\n    fn fmt() {}\n}\n",
    )
    .expect("source");
    let out = run(&root.0, json!({"operation":"symbols","path":source})).expect("symbols");
    assert_eq!(
        json!(crate::tools::symbol_outline::outline_rows(
            &crate::tools::symbol_outline::flatten_members(
                out["symbols"].as_array().expect("rows")
            )
        )),
        json!([
            "1 struct Pipe +",
            "2-7 impl Pipe",
            "  3 method read +; 5 read_buf +",
            "8-10 impl AsyncWrite for Pipe",
            "  9 method poll_flush",
            "11-13 impl std::fmt::Debug for Wrap",
            "  12 method fmt"
        ]),
        "{out}"
    );
    // The type name still selects every impl of the type; the lead anchors
    // lspSearch on the type, as for an inherent impl.
    let named = run(
        &root.0,
        json!({"operation":"symbols","path":source,"symbolName":"Pipe","kinds":["impl"]}),
    )
    .expect("named");
    let names: Vec<&str> = named["symbols"]
        .as_array()
        .expect("rows")
        .iter()
        .map(outline_name)
        .collect();
    assert_eq!(names, ["Pipe", "AsyncWrite for Pipe"], "{named}");
    let trait_only = run(
        &root.0,
        json!({"operation":"symbols","path":source,"symbolName":"AsyncWrite for Pipe"}),
    )
    .expect("trait impl");
    assert_eq!(
        outline_name(&trait_only["symbols"][0]),
        "AsyncWrite for Pipe"
    );
    // The lead exists only where rust-analyzer runs.
    if let Some(lead) = trait_only["hints"].get("references") {
        assert_eq!(
            lead["query"]["queries"][0]["symbolName"], "Pipe",
            "{trait_only}"
        );
    }
}

/// The snapshot of a 4-row symbols page of `source`.
fn out_snapshot(root: &std::path::Path, source: &std::path::Path) -> serde_json::Value {
    let first = run(
        root,
        json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":source,"pageSize":4}),
    )
    .expect("first page");
    first["snapshot"].clone()
}

#[test]
fn directory_symbols_keep_row_paths_without_static_notes() {
    let root = Fixture::new();
    std::fs::write(root.0.join("a.rs"), "fn a() {}\n").expect("a");
    std::fs::write(root.0.join("b.rs"), "fn b() {}\n").expect("b");
    let out = run(
        &root.0,
        json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":root.0}),
    )
    .expect("symbols");
    assert!(out.get("symbols").is_none(), "{out}");
    let files = out["files"].as_array().expect("files");
    assert_eq!(files.len(), 2, "{out}");
    for file in files {
        assert!(file["path"].is_string(), "{out}");
        let rows = file["symbols"].as_array().expect("rows");
        assert_eq!(rows.len(), 1, "{out}");
        assert!(
            rows[0].get("path").is_none(),
            "path written once per file: {out}"
        );
    }
    let diagnostics = out["diagnostics"].as_array().expect("diagnostics");
    assert!(diagnostics.is_empty(), "{out}");
}

#[test]
fn match_rows_withhold_captures_by_default_and_omit_single_line_ends() {
    let root = Fixture::new();
    let source = root.0.join("calls.ts");
    std::fs::write(&source, "oldCall(one);\noldCall(\n  two\n);\n").expect("source");
    let query = json!({"operation":"match","mainGoal":"test","reasoning":"test","path":source,"pattern":"oldCall($A)"});
    let out = run(&root.0, query.clone()).expect("match");
    let matches = out["files"][0]["matches"].as_array().expect("matches");
    // Lean rows (X1 objects): the line and 1-based column (a span adds its
    // end line) and the normalized text; no captures.
    assert_eq!(
        matches,
        &vec![
            json!({"line":1,"column":1,"value":"oldCall(one)"}),
            json!({"line":2,"column":1,"endLine":4,"value":"oldCall( two )"})
        ],
        "{out}"
    );
    // Equal per-file counts repeat `matches.len()`.
    assert!(out["files"][0].get("totalMatchRows").is_none(), "{out}");
    // Both $A captures show in the values: nothing to expand.
    assert!(
        out.get("next")
            .is_none_or(|next| next.get("expandCaptures").is_none()),
        "{out}"
    );
    // A value cut short hides its capture, which stays one exact
    // continuation away.
    let mut cut = query.clone();
    cut["matchContentLength"] = json!(6);
    let out = run(&root.0, cut).expect("match");
    let expand = &out["next"]["expandCaptures"]["query"]["queries"][0];
    assert_eq!(expand["captureText"], true, "{out}");

    let mut expanded = query;
    expanded["captureText"] = json!(true);
    let out = run(&root.0, expanded).expect("match");
    let single = &out["files"][0]["matches"][0];
    assert_eq!(single["line"], 1, "{single}");
    assert_eq!(single["column"], 1, "{single}");
    assert!(single.get("endLine").is_none(), "{single}");
    assert!(
        single.get("endColumn").is_none(),
        "single-line end: {single}"
    );
    let span = &out["files"][0]["matches"][1];
    assert_eq!(span["endLine"], 4, "{span}");
    assert!(span["endColumn"].is_u64(), "{span}");
    assert!(
        single.get("metavars").is_none(),
        "duplicate capture map: {single}"
    );
    assert_eq!(single["metavarRanges"]["A"][0]["text"], "one", "{single}");
    assert!(
        single["metavarRanges"]["A"][0].get("endLine").is_none(),
        "{single}"
    );
    assert!(out["next"].get("expandCaptures").is_none(), "{out}");
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

#[test]
fn list_captures_are_withheld_by_default_and_expand_on_request() {
    let root = Fixture::new();
    let source = root.0.join("body.rs");
    std::fs::write(
        &source,
        "fn a() -> u8 {\n    let x = 1;\n    let y = 2;\n    x + y\n}\n",
    )
    .expect("source");
    let query = json!({"operation":"match","mainGoal":"test","reasoning":"test","path":source,
        "pattern":"fn $N() -> u8 { $$$B }"});
    let out = run(&root.0, query.clone()).expect("match");
    // A lean row: no capture dump, the header stands in for the body.
    let (line, end, value) = lean_row(&out["files"][0]["matches"][0]);
    assert_eq!((line, end, value), (1, Some(5), "fn a() -> u8 …"), "{out}");
    assert!(out["next"]["expandCaptures"].is_object(), "{out}");

    let mut expanded = query;
    expanded["captureText"] = json!(true);
    let out = run(&root.0, expanded).expect("match");
    let m = &out["files"][0]["matches"][0];
    let body = m["metavarRanges"]["B"].as_array().expect("B ranges");
    assert_eq!(body.len(), 3, "{out}");
    assert_eq!(body[0]["text"], "let x = 1;", "{out}");
    assert_eq!(body[0]["line"], 2, "{out}");
    assert_eq!(m["metavarRanges"]["N"][0]["text"], "a", "{m}");
}

#[test]
fn rust_item_pattern_without_visibility_also_matches_pub_items() {
    let root = Fixture::new();
    let source = root.0.join("lib.rs");
    std::fs::write(
        &source,
        "pub fn a() -> Result<u8, String> { Ok(1) }\nfn b() -> Result<u8, String> { Ok(2) }\n",
    )
    .expect("source");
    let relaxed = |out: &serde_json::Value| {
        out["diagnostics"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|d| d["code"] == "structural.pattern.relaxed" && d["severity"] == "warning")
    };
    // ast-grep reads the visibility modifier as a named child, so `fn` alone
    // skips `pub fn`; the result also runs the visible spelling and says so.
    let out = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":source,
            "pattern":"fn $N() -> Result<$T, String> { $$$B }"}),
    )
    .expect("match");
    assert_eq!(out["stats"]["matchCount"], 2, "{out}");
    assert!(relaxed(&out), "{out}");
    assert!(!out.to_string().contains("visibilityExact"), "{out}");
    // A pattern that already names the visibility is run as written.
    let out = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":source,
            "pattern":"pub fn $N() -> Result<$T, String> { $$$B }"}),
    )
    .expect("match");
    assert_eq!(out["stats"]["matchCount"], 1, "{out}");
    assert!(!relaxed(&out), "{out}");
}

#[test]
fn yaml_rule_compile_errors_get_a_rule_hint_not_a_pattern_hint() {
    let root = Fixture::new();
    let source = root.0.join("lib.rs");
    std::fs::write(&source, "fn a() {}\n").expect("source");
    let error = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":source,
            "rule":"rule:\n  kindx: function_item\n"}),
    )
    .expect_err("invalid rule");
    assert_eq!(error.code, "invalidPattern");
    let hint = error.hints.join(" ");
    assert!(hint.contains("rule"), "{hint}");
    assert!(!hint.contains("add `;`"), "{hint}");
}

#[test]
fn flow_js_symbols_list_hooks_and_mark_a_recovered_parse_partial() {
    let root = Fixture::new();
    let source = root.0.join("Hooks.js");
    std::fs::write(
        &source,
        "// @flow\nimport type {D} from 'd';\ntype Dispatch<A> = A => void;\n\
         export function useState<S>(init: (() => S) | S): [S, Dispatch<S>] {\n  if (x) {\n    y();\n  }\n  return z;\n}\n\
         export function useRef<T>(v: T): {current: T} {\n  if (x) {\n    y();\n  }\n  return z;\n}\n",
    )
    .expect("source");
    let out = run(
        &root.0,
        json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":source}),
    )
    .expect("symbols");
    let names = outline_names(&out["symbols"]);
    assert!(
        names.contains(&"useState") && names.contains(&"useRef"),
        "{out}"
    );
    assert!(!names.contains(&"if"), "{out}");
    let recovered = out["diagnostics"].as_array().is_some_and(|ds| {
        ds.iter().any(|d| {
            d["message"]
                .as_str()
                .is_some_and(|m| m.starts_with("tree-sitter recovered"))
        })
    });
    // A recovered parse is a partial inventory, never a complete one.
    assert!(
        recovered,
        "the Flow type alias forces a recovered parse: {out}"
    );
    assert_eq!(out["isPartial"], json!(true), "{out}");
}

/// A single-file match parses the file as a directory scan does: raw.
/// Redacting first shifted columns and could erase a matched row; values
/// are still redacted in the output.
#[test]
fn single_file_match_keeps_redacted_rows_and_directory_positions() {
    let root = Fixture::new();
    let source = root.0.join("k.ts");
    std::fs::write(
        &source,
        "export const A = \"plain\"; hit(1);\nexport const GITHUB_TOKEN = \"ghp_aBcDeFgHiJkLmNoPqRsTuVwXyZ0123456789\"; hit(2);\n",
    )
    .expect("source");
    let rows = |path: &std::path::Path| {
        let out = run(
            &root.0,
            json!({"operation":"match","mainGoal":"test","reasoning":"test","path":path,"language":"typescript","pattern":"export const $N = $V"}),
        )
        .expect("match");
        let file = out["files"][0].clone();
        assert!(file.is_object(), "{out}");
        file["matches"].as_array().expect("matches").clone()
    };
    let single = rows(&source);
    let directory = rows(&root.0);
    assert_eq!(single.len(), 2, "{single:?}");
    assert_eq!(single, directory);
}

/// A pattern the grammar cannot parse fails before any prefilter: a
/// directory where no file holds the anchor must not report `complete` and
/// empty for `foo(`.
#[test]
fn invalid_pattern_is_an_error_even_when_every_file_is_prefiltered() {
    let root = Fixture::new();
    std::fs::write(root.0.join("a.ts"), "const value = 1;\n").expect("source");
    let error = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":"typescript","pattern":"foo("}),
    )
    .expect_err("an unparseable pattern must fail loudly");
    // The response stage attaches the repair hint for this code.
    assert_eq!(error.code, "invalidPattern", "{error:?}");
}

/// A multi-line match with a body block shows its header (text before the
/// body) with line/endLine anchors, not the flattened body; captureText:true
/// (next.expandCaptures) returns the whole match text.
#[test]
fn block_matches_default_to_their_header_and_expand_on_request() {
    let root = Fixture::new();
    let source = root.0.join("lib.rs");
    std::fs::write(
        &source,
        "pub fn load(\n    path: &str,\n) -> Result<u8, String> {\n    let raw = read(path)?;\n    parse(raw)\n}\n",
    )
    .expect("source");
    let query = json!({"operation":"match","mainGoal":"test","reasoning":"test","path":source,
        "rule":"rule:\n  kind: function_item\n"});
    let out = run(&root.0, query.clone()).expect("match");
    assert_eq!(
        out["files"][0]["matches"][0],
        json!({"line":1,"column":1,"endLine":6,"value":"pub fn load( path: &str, ) -> Result<u8, String> …"}),
        "{out}"
    );
    assert!(out["next"]["expandCaptures"].is_object(), "{out}");

    let mut expanded = query;
    expanded["captureText"] = json!(true);
    let out = run(&root.0, expanded).expect("match");
    let value = out["files"][0]["matches"][0]["value"]
        .as_str()
        .expect("value");
    assert!(value.ends_with("parse(raw) }"), "{out}");
    assert!(out["next"].get("expandCaptures").is_none(), "{out}");
}

/// A directory match without language uses the one grammar whose files the
/// scope holds; continuations pin the inferred grammar.
#[test]
fn directory_match_infers_the_only_grammar_present() {
    let root = Fixture::new();
    std::fs::write(root.0.join("a.rs"), "fn a() { hit(1); }\n").expect("a");
    std::fs::create_dir(root.0.join("nested")).expect("nested");
    std::fs::write(root.0.join("nested/b.rs"), "fn b() { hit(2); }\n").expect("b");
    std::fs::write(root.0.join("README.md"), "hit(3)\n").expect("doc");
    let out = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"pattern":"hit($A)","pageSize":1}),
    )
    .expect("language inferred from the .rs files");
    assert_eq!(out["stats"]["matchCount"], 2, "{out}");
    assert_eq!(out["inferredLanguage"], "rust", "{out}");
    assert_eq!(
        out["next"]["nextPage"]["query"]["queries"][0]["language"], "rust",
        "{out}"
    );
    let explicit = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"pattern":"hit($A)","pageSize":1,"language":"rust"}),
    )
    .expect("explicit language");
    assert_eq!(
        explicit["snapshot"], out["snapshot"],
        "same scope, same snapshot"
    );
    assert!(explicit.get("inferredLanguage").is_none(), "{explicit}");
}

#[test]
fn directory_match_with_several_parsing_grammars_names_the_candidates() {
    let root = Fixture::new();
    std::fs::write(root.0.join("a.rs"), "fn a() { hit(1); }\n").expect("a");
    std::fs::write(root.0.join("b.ts"), "hit(2);\n").expect("b");
    let error = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"pattern":"hit($A)"}),
    )
    .expect_err("two grammars parse the pattern");
    assert_eq!(error.code, "languageRequired", "{error:?}");
    for language in ["rust", "typescript"] {
        assert!(error.message.contains(language), "{error:?}");
    }
    // Every candidate grammar is its own runnable lead.
    let next = error.next.as_ref().expect("per-grammar leads");
    for (lead, language) in [("withRust", "rust"), ("withTypescript", "typescript")] {
        let row = &next[lead]["query"]["queries"][0];
        assert_eq!(row["language"], language, "{next}");
        assert_eq!(row["pattern"], "hit($A)", "{next}");
    }
    // The named language is the repair: it scans only that grammar's files.
    let out = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"pattern":"hit($A)","language":"rust"}),
    )
    .expect("explicit language");
    assert_eq!(out["stats"]["matchCount"], 1, "{out}");
    assert!(out.get("inferredLanguage").is_none(), "{out}");
}

/// A rule object and its YAML string select the same nodes.
#[test]
fn object_and_yaml_rules_match_identically() {
    let root = Fixture::new();
    std::fs::write(
        root.0.join("lib.rs"),
        "fn a() { unsafe { Pin::new_unchecked(&mut x); } }\nfn b() { Pin::new_unchecked(&mut y); }\n",
    )
    .expect("source");
    // A short value cut hides the capture, so the page offers expandCaptures.
    let base = json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":"rust","matchContentLength":12});
    let mut yaml = base.clone();
    yaml["rule"] = json!(
        "pattern: Pin::new_unchecked($A)\nnot:\n  inside:\n    kind: unsafe_block\n    stopBy: end"
    );
    let mut object = base;
    object["rule"] = json!({"pattern":"Pin::new_unchecked($A)","not":{"inside":{"kind":"unsafe_block","stopBy":"end"}}});
    let yaml = run(&root.0, yaml).expect("yaml rule");
    let object = run(&root.0, object).expect("object rule");
    assert_eq!(yaml["files"], object["files"], "{yaml} vs {object}");
    assert_eq!(
        yaml["files"][0]["matches"].as_array().map(Vec::len),
        Some(1),
        "{yaml}"
    );
    assert_eq!(lean_row(&yaml["files"][0]["matches"][0]).0, 2, "{yaml}");
    // The continuation copies the object rule exactly as written.
    assert_eq!(
        object["next"]["expandCaptures"]["query"]["queries"][0]["rule"],
        json!({"pattern":"Pin::new_unchecked($A)","not":{"inside":{"kind":"unsafe_block","stopBy":"end"}}}),
        "{object}"
    );
    let continuation = object["next"]["expandCaptures"]["query"]["queries"][0].clone();
    assert!(
        serde_json::from_value::<super::AstSearchQuery>(continuation).is_ok(),
        "the continuation parses as a typed row"
    );
}

/// X14: matches inside a `macro_rules!` template body are listed (no
/// silent omission behind `complete:true`).
#[test]
fn macro_rules_template_bodies_are_listed() {
    let root = Fixture::new();
    std::fs::write(
        root.0.join("m.rs"),
        "macro_rules! get {\n    ($e:expr) => {\n        config.value.unwrap()\n    };\n}\nfn main() {\n    other.unwrap();\n}\n",
    )
    .expect("source");
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

/// AS5: the rule-file object `{rule: …}` matches like the YAML rule file
/// (`rule:` wrapper) and like the bare rule.
#[test]
fn rule_config_object_matches_like_yaml_rule_file() {
    let root = Fixture::new();
    std::fs::write(
        root.0.join("lib.rs"),
        "fn a() { unsafe { Pin::new_unchecked(&mut x); } }\nfn b() { Pin::new_unchecked(&mut y); }\n",
    )
    .expect("source");
    let base = json!({"operation":"match","path":root.0,"language":"rust"});
    let bare = json!({"pattern":"Pin::new_unchecked($A)","not":{"inside":{"kind":"unsafe_block","stopBy":"end"}}});
    let mut yaml = base.clone();
    yaml["rule"] = json!(
        "id: pin\nlanguage: rust\nrule:\n  pattern: Pin::new_unchecked($A)\n  not:\n    inside:\n      kind: unsafe_block\n      stopBy: end"
    );
    let mut object = base.clone();
    object["rule"] = json!({ "rule": bare });
    let mut plain = base;
    plain["rule"] = bare;
    let yaml = run(&root.0, yaml).expect("yaml rule file");
    let object = run(&root.0, object).expect("rule file object");
    let plain = run(&root.0, plain).expect("bare rule");
    assert_eq!(yaml["files"], object["files"], "{yaml} vs {object}");
    assert_eq!(plain["files"], object["files"], "{plain} vs {object}");
    assert_eq!(
        object["files"][0]["matches"].as_array().map(Vec::len),
        Some(1),
        "{object}"
    );
}

#[test]
fn symbols_name_list_returns_the_union_and_a_string_is_one_entry() {
    let root = Fixture::new();
    let source = root.0.join("task.rs");
    std::fs::write(
        &source,
        "fn complete() {}\nfn try_read_output() {}\nfn other() {}\n",
    )
    .expect("source");
    let names = |out: &serde_json::Value| {
        outline_names(&out["symbols"])
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let list = run(
        &root.0,
        json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":source,"symbolName":["complete","try_read_output"]}),
    )
    .expect("list");
    assert_eq!(names(&list), ["complete", "try_read_output"], "{list}");
    let single = run(
        &root.0,
        json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":source,"symbolName":"complete"}),
    )
    .expect("single");
    assert_eq!(names(&single), ["complete"], "{single}");
}

#[test]
fn symbols_list_entries_prefer_exact_names_and_lowercase_ignores_case() {
    let root = Fixture::new();
    let source = root.0.join("query.rs");
    std::fs::write(
        &source,
        "fn get() {}\nfn aget() {}\nfn get_or_create() {}\nfn Native() {}\nfn parse_x() {}\n",
    )
    .expect("source");
    let names = |query: serde_json::Value| {
        let mut row =
            json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":source});
        row["symbolName"] = query;
        let out = run(&root.0, row).expect("symbols");
        outline_names(&out["symbols"])
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    // A list names identifiers: an entry with an exact declaration keeps
    // only it; an entry without one still matches as a substring.
    assert_eq!(names(json!(["get", "parse"])), ["get", "parse_x"]);
    // One string is a one-entry list: exact first, else a substring family.
    assert_eq!(names(json!("get")), ["get"]);
    assert_eq!(names(json!("ge")), ["get", "aget", "get_or_create"]);
    // All-lowercase text ignores case; any capital makes it exact-case.
    assert_eq!(names(json!("native")), ["Native"]);
    assert!(names(json!("NATIVE")).is_empty());
}

#[test]
fn a_single_named_declaration_leads_to_its_callers() {
    let root = Fixture::new();
    let source = root.0.join("lead.rs");
    std::fs::write(&source, "struct S;\n\nfn helper() {}\nfn helper_two() {}\n").expect("source");
    let symbols = |name: serde_json::Value| {
        run(
            &root.0,
            json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":source,"symbolName":name}),
        )
        .expect("symbols")
    };
    // A string names an identifier exactly like a one-entry list.
    assert_eq!(symbols(json!("helper")), symbols(json!(["helper"])));
    let one = symbols(json!(["helper"]));
    // Offered only where a language server runs for the file.
    if crate::tools::lsp_search::verify_query(
        &source.to_string_lossy(),
        "helper",
        3,
        crate::tools::lsp_search::Verify::Callers,
    )
    .is_none()
    {
        assert!(one.get("next").is_none(), "{one}");
        return;
    }
    let lead = &one["next"]["callers"];
    assert_eq!(lead["tool"], "lspSearch", "{one}");
    // A function's usual next question is its call sites.
    assert_eq!(lead["query"]["queries"][0]["operation"], "callers", "{one}");
    assert_eq!(lead["query"]["queries"][0]["symbolName"], "helper", "{one}");
    assert_eq!(lead["query"]["queries"][0]["lineHint"], 3, "{one}");
    let uri = lead["query"]["queries"][0]["path"].as_str().expect("uri");
    assert!(
        std::path::Path::new(uri).is_absolute() && uri.ends_with("lead.rs"),
        "{uri}"
    );
    // Several declarations, or no name filter, name no single symbol.
    for many in [symbols(json!("help")), symbols(json!(null))] {
        assert!(many["next"].get("callers").is_none(), "{many}");
    }
    // A directory scope resolves the row's file for the lead.
    let dir = run(
        &root.0,
        json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":root.0,"symbolName":["helper"]}),
    )
    .expect("directory symbols");
    let uri = dir["next"]["callers"]["query"]["queries"][0]["path"]
        .as_str()
        .expect("directory path");
    assert_eq!(
        std::path::Path::new(uri),
        source.canonicalize().expect("canonical"),
        "{dir}"
    );
}

#[test]
fn a_bare_include_word_matches_file_names_containing_it() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("nested")).expect("dir");
    std::fs::write(
        root.0.join("nested/handler_test.rs"),
        "fn a() { hit(1); }\n",
    )
    .expect("a");
    std::fs::write(root.0.join("other.rs"), "fn b() { hit(2); }\n").expect("b");
    let out = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,
            "language":"rust","pattern":"hit($A)","include":["test"]}),
    )
    .expect("match");
    assert_eq!(out["stats"]["matchCount"], 1, "{out}");
    assert!(out.to_string().contains("handler_test.rs"), "{out}");
}

#[test]
fn directory_symbols_by_name_parse_only_files_that_spell_the_name() {
    let root = Fixture::new();
    std::fs::write(root.0.join("a.rs"), "pub fn helper() {}\n").expect("a");
    // No file without the name is parsed: its recovered-parse diagnostic
    // cannot mark the named outline partial.
    std::fs::write(root.0.join("b.rs"), "fn broken( {\n").expect("b");
    std::fs::write(root.0.join("c.rs"), "fn Helper_case() {}\n").expect("c");
    let out = run(
        &root.0,
        json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":root.0,"symbolName":"helper"}),
    )
    .expect("symbols");
    assert_eq!(out["isPartial"], false, "{out}");
    assert_eq!(
        out["diagnostics"].as_array().map(Vec::len),
        Some(0),
        "{out}"
    );
    assert_eq!(out["filesScanned"], 3, "{out}");
    let files = out["files"].as_array().expect("files");
    assert_eq!(files.len(), 1, "{out}");
    assert_eq!(outline_names(&files[0]["symbols"]), ["helper"], "{out}");
    // All-lowercase text ignores case in the byte prefilter too.
    let folded = run(
        &root.0,
        json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":root.0,"symbolName":"helper_case"}),
    )
    .expect("folded");
    assert_eq!(
        outline_names(&folded["files"][0]["symbols"]),
        ["Helper_case"],
        "{folded}"
    );
}

#[test]
fn an_empty_declaration_pattern_retries_with_the_parts_it_omitted() {
    let root = Fixture::new();
    let typed = root.0.join("typed.ts");
    std::fs::write(
        &typed,
        "export function typed(a: number): number { return a; }\nfunction local(): void {}\n",
    )
    .expect("typed");
    let mixed = root.0.join("mixed.ts");
    std::fs::write(
        &mixed,
        "export function typed(a: number): number { return a; }\nexport function untyped(b) { return b; }\n",
    )
    .expect("mixed");
    let rust = root.0.join("items.rs");
    std::fs::write(
        &rust,
        "pub fn a() -> u8 { 1 }\npub(crate) fn b() {}\nfn c() {}\n",
    )
    .expect("rust");
    let python = root.0.join("defs.py");
    std::fs::write(&python, "def f(x) -> int:\n    return x\n").expect("python");
    let matched = |path: &PathBuf, pattern: &str| {
        run(
            &root.0,
            json!({"operation":"match","mainGoal":"test","reasoning":"test","path":path,"pattern":pattern}),
        )
        .expect("match")
    };
    let relaxed = |out: &serde_json::Value| {
        out["diagnostics"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|d| d["code"] == "structural.pattern.relaxed")
    };

    // Every function has a return type: the pattern as written matches
    // nothing, so the retry spells the return type it omitted.
    let out = matched(&typed, "export function $N($$$A) { $$$B }");
    assert_eq!(out["stats"]["matchCount"], 1, "{out}");
    assert!(out.get("status").is_none(), "{out}");
    assert!(relaxed(&out), "{out}");
    let out = matched(&typed, "function $N($$$A) { $$$B }");
    assert_eq!(out["stats"]["matchCount"], 2, "{out}");

    // A pattern that matches as written still gains the spellings it
    // omitted: the untyped function and the typed one.
    let out = matched(&mixed, "export function $N($$$A) { $$$B }");
    assert_eq!(out["stats"]["matchCount"], 2, "{out}");
    assert!(relaxed(&out), "{out}");

    // Rust: visibility and the return type; Python: the return annotation.
    let out = matched(&rust, "fn $N() { $$$B }");
    assert_eq!(out["stats"]["matchCount"], 3, "{out}");
    assert!(relaxed(&out), "c as written, a and b relaxed: {out}");
    let only_pub = root.0.join("pub.rs");
    std::fs::write(&only_pub, "pub fn a() -> u8 { 1 }\npub(crate) fn b() {}\n").expect("pub");
    let out = matched(&only_pub, "fn $N() { $$$B }");
    assert_eq!(out["stats"]["matchCount"], 2, "{out}");
    assert!(relaxed(&out), "{out}");
    let out = matched(&python, "def $F($$$A): $$$B");
    assert_eq!(out["stats"]["matchCount"], 1, "{out}");
    assert!(relaxed(&out), "{out}");

    // Nothing to add, or nothing found either way: the plain empty result.
    let out = matched(&typed, "class $C { $$$B }");
    assert_eq!(out["status"], "empty", "{out}");
    assert!(!relaxed(&out), "{out}");
}

/// A single-file outline leads to its natural next read: the top
/// declaration's lines (C8); a directory outline names no single read.
#[test]
fn a_single_file_outline_leads_to_its_top_declaration() {
    let root = Fixture::new();
    let file = root.0.join("lib.rs");
    std::fs::write(
        &file,
        "pub fn first() {\n    one();\n}\n\npub fn second() {}\n",
    )
    .expect("source");
    let (paths, security) = simple_policy(&root.0);
    let outline = execute_row(
        json!({"operation":"symbols","path":file}),
        &paths,
        &security,
        &NeverCancel,
    )
    .expect("symbols");
    let read = &outline["next"]["read"];
    assert_eq!(read["tool"], "localFetch", "{outline}");
    assert_eq!(
        read["query"]["queries"][0]["ranges"],
        json!(["1-3"]),
        "{outline}"
    );
    let listing = execute_row(
        json!({"operation":"symbols","path":root.0}),
        &paths,
        &security,
        &NeverCancel,
    )
    .expect("symbols");
    assert!(listing["next"].get("read").is_none(), "{listing}");
}

/// `.ts` and `.tsx` are one family: `typescript` scans `.tsx` with the TSX
/// parser, so a mixed directory needs no language.
#[test]
fn directory_match_over_a_grammar_family_infers_the_family() {
    let root = Fixture::new();
    std::fs::write(root.0.join("a.ts"), "hit(1);\n").expect("a");
    std::fs::write(root.0.join("b.tsx"), "const v = <div>{hit(2)}</div>;\n").expect("b");
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"pattern":"hit($A)"}),
    )
    .expect("one family");
    assert_eq!(out["inferredLanguage"], "typescript", "{out}");
    assert_eq!(out["stats"]["matchCount"], 2, "{out}");
}

/// Parse-recovery notes stay on files that list a declaration; the other
/// files share one entry per message with every path, and a text search
/// completes the name lookup the recovered parses may have missed.
#[test]
fn symbols_group_recovery_notes_of_files_without_a_listed_declaration() {
    let root = Fixture::new();
    std::fs::write(root.0.join("a.rs"), "fn sdsnewlen() {}\n").expect("a");
    for file in ["b.rs", "c.rs"] {
        std::fs::write(root.0.join(file), "fn caller() { sdsnewlen(; }\n").expect("broken");
    }
    let out = run(
        &root.0,
        json!({"operation":"symbols","path":root.0,"symbolName":"sdsnewlen"}),
    )
    .expect("symbols");
    let diagnostics = out["diagnostics"].as_array().expect("diagnostics");
    assert_eq!(diagnostics.len(), 1, "{out}");
    assert!(diagnostics[0].get("path").is_none(), "{out}");
    let files = diagnostics[0]["files"].as_array().expect("files");
    assert_eq!(files.len(), 2, "{out}");
    assert!(files.iter().all(|file| file["path"].is_string()), "{out}");
    assert!(
        diagnostics[0]["message"]
            .as_str()
            .is_some_and(|m| m.starts_with("tree-sitter recovered")),
        "{out}"
    );
    assert_eq!(out["isPartial"], true, "{out}");
    let lead = &out["next"]["textSearch"];
    assert_eq!(lead["tool"], "localSearch", "{out}");
    assert_eq!(
        lead["query"]["queries"][0]["matchString"], "sdsnewlen",
        "{out}"
    );
}

/// N11: a recovered parse can hide a declaration of the queried name only
/// where its syntax errors spell that name. A file that only calls the name
/// in code that parsed keeps no note and leaves the outline complete.
#[test]
fn recovered_notes_only_for_files_whose_errors_spell_the_name() {
    let root = Fixture::new();
    std::fs::write(root.0.join("a.c"), "int foo(int x) { return x; }\n").expect("a");
    // Calls `foo` on a clean line; the syntax error is elsewhere.
    std::fs::write(
        root.0.join("b.c"),
        "int caller(void) { return foo(1); }\n\nint broken( {\n  return 2;\n}\n",
    )
    .expect("b");
    // The syntax error spells `foo`: a declaration could hide there.
    std::fs::write(root.0.join("c.c"), "int user(void) {\n  int foo( = 3;\n}\n").expect("c");
    let out = run(
        &root.0,
        json!({"operation":"symbols","path":root.0,"symbolName":"foo"}),
    )
    .expect("symbols");
    let diagnostics = out["diagnostics"].as_array().cloned().unwrap_or_default();
    let noted = diagnostics
        .iter()
        .flat_map(|d| {
            d["files"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|file| file["path"].clone())
                .chain(d.get("path").cloned())
        })
        .filter_map(|path| path.as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    assert!(noted.iter().any(|path| path.ends_with("c.c")), "{out}");
    assert!(!noted.iter().any(|path| path.ends_with("b.c")), "{out}");
    assert_eq!(out["isPartial"], true, "{out}");
    // Only b.c's irrelevant note: the outline is complete.
    std::fs::remove_file(root.0.join("c.c")).expect("remove c");
    let out = run(
        &root.0,
        json!({"operation":"symbols","path":root.0,"symbolName":"foo"}),
    )
    .expect("symbols");
    assert!(
        out["diagnostics"].as_array().is_none_or(Vec::is_empty),
        "{out}"
    );
    assert_eq!(out["isPartial"], false, "{out}");
}

/// AS4: an empty symbols search says what it searched (the name and the
/// file count) and leads to the name's text.
#[test]
fn empty_symbols_hint_names_file_count_and_text_lead() {
    let root = Fixture::new();
    std::fs::write(root.0.join("a.rs"), "fn alpha() {}\n").expect("a");
    std::fs::write(root.0.join("b.rs"), "fn beta() { missing_name(); }\n").expect("b");
    let out = run(
        &root.0,
        json!({"operation":"symbols","path":root.0,"symbolName":"missing_name"}),
    )
    .expect("symbols");
    assert_eq!(out["status"], "empty", "{out}");
    let hints = out["hints"].as_array().expect("hints");
    assert!(
        hints.iter().any(|hint| hint
            .as_str()
            .is_some_and(|text| text.contains("0 declarations named missing_name in 2 files"))),
        "{out}"
    );
    let lead = &out["next"]["textSearch"];
    assert_eq!(lead["tool"], "localSearch", "{out}");
    assert_eq!(
        lead["query"]["queries"][0]["matchString"], "missing_name",
        "{out}"
    );
    // Without a name, the hint counts the files and names no text lead.
    let outline = run(
        &root.0,
        json!({"operation":"symbols","path":root.0,"kinds":["class"]}),
    )
    .expect("outline");
    assert_eq!(outline["status"], "empty", "{outline}");
    assert!(
        outline["hints"][0]
            .as_str()
            .is_some_and(|text| text.contains("0 declarations in 2 files")),
        "{outline}"
    );
    assert!(outline["next"].get("textSearch").is_none(), "{outline}");
}

/// AS4: an empty match counts the files it parsed and leads to the syntax
/// tree of the first, where the pattern's node shape can be compared.
#[test]
fn empty_match_hint_leads_to_syntax_tree() {
    let root = Fixture::new();
    std::fs::write(root.0.join("a.rs"), "fn alpha() { beta(1); }\n").expect("a");
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"language":"rust","pattern":"beta($A, $B)"}),
    )
    .expect("match");
    assert_eq!(out["status"], "empty", "{out}");
    assert!(
        out["diagnostics"][0]["message"]
            .as_str()
            .is_some_and(|text| text.contains("in 1 parsed file")),
        "{out}"
    );
    let lead = &out["next"]["viewTree"];
    assert_eq!(lead["tool"], "astSearch", "{out}");
    let row = &lead["query"]["queries"][0];
    assert_eq!(row["operation"], "syntaxTree", "{out}");
    assert!(
        row["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("a.rs")),
        "{out}"
    );
    crate::contracts::validate_query("astSearch", row.clone()).expect("a valid syntaxTree row");
}

/// AS5 (native half): a wrapped rule document `rule:\n  …` matches like the
/// bare rule. The object form `{rule:{…}}` serializes to the same document,
/// so it runs unchanged once core declares it (core request AS5).
#[test]
fn wrapped_rule_document_matches_like_the_bare_rule() {
    let root = Fixture::new();
    std::fs::write(root.0.join("a.rs"), "fn a() { f(x); g(1); }\n").expect("source");
    let base = json!({"operation":"match","path":root.0,"language":"rust"});
    let mut bare = base.clone();
    bare["rule"] = json!({"pattern":"f($A)"});
    let mut wrapped = base;
    // The JSON text of `{rule:{pattern:"f($A)"}}` is valid YAML for it.
    wrapped["rule"] = json!(json!({"rule":{"pattern":"f($A)"}}).to_string());
    let bare = run(&root.0, bare).expect("bare rule");
    let wrapped = run(&root.0, wrapped).expect("wrapped rule");
    assert_eq!(bare["files"], wrapped["files"], "{bare} vs {wrapped}");
    assert_eq!(
        wrapped["files"][0]["matches"].as_array().map(Vec::len),
        Some(1),
        "{wrapped}"
    );
}

/// repo-sweep (redis module.c): a one-line `typedef struct X X;` declares
/// `type X` and `struct X` on one name line. The struct is the typedef's
/// sibling, never its own parent's member, and no row is dropped. A
/// multi-line `typedef struct Y {…} Y;` keeps its struct nested (different
/// name lines), and a struct field keeps its struct parent.
#[test]
fn c_one_line_typedef_struct_is_not_its_own_parent() {
    let root = Fixture::new();
    let source = root.0.join("module.c");
    std::fs::write(
        &source,
        "struct Api {\n    void *func;\n};\ntypedef struct Api Api;\ntypedef struct Fwd Fwd;\n\
         typedef struct Ctx {\n    int flags;\n} Ctx;\n",
    )
    .expect("source");
    let out = run(
        &root.0,
        json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":source}),
    )
    .expect("symbols");
    let rows = &out["symbols"];
    let flat = crate::tools::symbol_outline::flatten_members(rows.as_array().expect("rows"));
    // Every declaration survives, in source order.
    let named: Vec<(&str, &str, u64)> = flat
        .iter()
        .map(|row| {
            (
                row["symbolName"].as_str().expect("name"),
                row["kind"].as_str().expect("kind"),
                row["line"].as_u64().expect("line"),
            )
        })
        .collect();
    for want in [
        ("Api", "struct", 1),
        ("Api", "type", 4),
        ("Api", "struct", 4),
        ("Fwd", "type", 5),
        ("Fwd", "struct", 5),
        ("Ctx", "type", 8),
    ] {
        assert!(named.contains(&want), "{want:?} missing: {out}");
    }
    for row in &flat {
        assert!(
            !(row.get("parent") == row.get("symbolName")
                && row.get("parentLine") == row.get("line")),
            "self-parented row {row}: {out}"
        );
    }
    let parent_of = |name: &str, kind: &str, line: u64| {
        flat.iter()
            .find(|row| row["symbolName"] == name && row["kind"] == kind && row["line"] == line)
            .and_then(|row| row.get("parent").cloned())
    };
    assert_eq!(parent_of("Api", "struct", 4), None, "{out}");
    assert_eq!(parent_of("Fwd", "struct", 5), None, "{out}");
}

/// Follow `next.nextPage`, then `next.expandScan`, to the end of the chain;
/// returns every page.
fn ast_chain(root: &std::path::Path, query: serde_json::Value) -> Vec<serde_json::Value> {
    let mut pages = Vec::new();
    let mut next = Some(query);
    while let Some(query) = next.take() {
        assert!(pages.len() < 200, "the chain does not end");
        let out = run(root, query).expect("page");
        let page = out["next"].get("nextPage");
        let expand = out["next"].get("expandScan");
        assert!(
            page.is_none() || expand.is_none(),
            "one continuation per page: {out}"
        );
        next = page
            .or(expand)
            .map(|call| call["query"]["queries"][0].clone());
        if next.is_none() {
            assert!(out.get("terminalLimit").is_none(), "{out}");
        }
        pages.push(out);
    }
    pages
}

/// Every `(file, row)` a page lists: match rows under `files[].matches`,
/// symbol rows under `files[].symbols`.
fn ast_rows(pages: &[serde_json::Value]) -> Vec<String> {
    pages
        .iter()
        .flat_map(|out| out["files"].as_array().cloned().unwrap_or_default())
        .flat_map(|file| {
            let path = file["path"].as_str().expect("path").to_owned();
            let rows = file
                .get("matches")
                .or_else(|| file.get("symbols"))
                .and_then(serde_json::Value::as_array)
                .cloned()
                .unwrap_or_default();
            rows.into_iter().map(move |row| format!("{path} {row}"))
        })
        .collect()
}

/// D-2: `next.expandScan` resumes after the files its window evaluated, so
/// the chain of pages and widened scans lists every match and declaration
/// exactly once, under each sort.
#[test]
fn expanded_scans_reach_every_file_exactly_once() {
    let root = Fixture::new();
    for dir in ["a", "a/deep", "b"] {
        std::fs::create_dir_all(root.0.join(dir)).expect("dir");
        for n in 0..5 {
            let body = "pub fn source() {}\n".repeat(n + 1);
            std::fs::write(root.0.join(format!("{dir}/f{n}.rs")), body).expect("source file");
        }
    }
    let queries = [
        serde_json::json!({"operation":"match","path":root.0,"language":"rust","pattern":"pub fn source() {}","pageSize":2}),
        serde_json::json!({"operation":"match","path":root.0,"language":"rust","pattern":"pub fn source() {}","pageSize":2,"sort":"matchCount"}),
        serde_json::json!({"operation":"symbols","path":root.0,"pageSize":4}),
    ];
    for query in queries {
        let whole = ast_rows(&ast_chain(&root.0, query.clone()));
        assert_eq!(whole.len(), 45, "{query}");
        let mut cut = query.clone();
        cut["maxFiles"] = serde_json::json!(2);
        let pages = ast_chain(&root.0, cut);
        assert!(pages.len() > 4, "{query}");
        assert!(
            pages
                .iter()
                .any(|page| page["next"]["expandScan"]["query"]["queries"][0]["scanOffset"] == 8),
            "{query}"
        );
        let mut walked = ast_rows(&pages);
        let mut expected = whole.clone();
        walked.sort();
        expected.sort();
        assert_eq!(walked, expected, "{query}: no duplicate, no gap");
    }
}
