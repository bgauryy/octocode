//! astSearch `symbols`: outline rows, name filters, recovered parses,
//! paging and the leads an outline offers.
use super::test_rows::*;
use crate::execution::cancel::NeverCancel;
use crate::tools::test_support::{Fixture, checks, simple_policy};
use serde_json::json;

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
        // No page reaches the rest: a terminal limit naming its cut.
        assert_eq!(out["terminalLimit"], true, "{out}");
        assert_eq!(out["partialReasons"], json!(["fileTooLarge"]), "{out}");
        // No literal searched: the typed error is the whole answer.
        assert!(
            out.get("next").is_none() && out.get("hints").is_none(),
            "{out}"
        );
    }
    // A searched name leads to a text search of it in the same file.
    let named = run(
        &root.0,
        json!({"operation":"symbols","path":large,"symbolName":"visible"}),
    )
    .expect("limit row");
    let lead = &named["next"]["findText"];
    assert_eq!(lead["tool"], "localSearch", "{named}");
    assert_eq!(
        lead["query"]["queries"][0]["matchString"], "visible",
        "{named}"
    );
    assert!(
        lead["query"]["queries"][0]["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("large.rs")),
        "{named}"
    );
    let pattern = run(
        &root.0,
        json!({"operation":"match","path":large,"pattern":"console.log"}),
    );
    let error = pattern.expect_err("pattern over the parse limit");
    assert_eq!(error.code, "fileTooLarge");
    let next = error.next.expect("findText lead");
    assert_eq!(next["findText"]["query"]["queries"][0]["regex"], "literal");
    use crate::response::output::ToolOutput;
    assert_eq!(
        super::output::Output.error_hint("fileTooLarge"),
        Some(crate::response::output::NO_HINT)
    );
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
        json!(crate::response::symbol_outline::outline_rows(
            &crate::response::symbol_outline::flatten_members(
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
    checks::assert_absent(&out, "syntax-only");
}

/// Every symbols row is an object (a class with members, and a file
/// of member-less declarations alike); `pagination.totalItems` counts the
/// top-level rows, and pages by `pageSize` reach each top-level row once
/// with its members.
#[test]
fn symbols_rows_are_objects_and_pages_count_top_level_rows() {
    fn all_objects(rows: &serde_json::Value) -> bool {
        rows.as_array().expect("rows").iter().all(|row| {
            row.is_object()
                && row["symbolName"].is_string()
                && row["line"].is_u64()
                && row.get("members").is_none_or(all_objects)
        })
    }
    let root = Fixture::new();
    let class = root.0.join("service.ts");
    std::fs::write(
        &class,
        "export class Service {\n  setParent() {}\n  dispose() {}\n}\nexport function boot() {}\nexport class Empty {}\n",
    )
    .expect("class");
    let plain = root.0.join("plain.ts");
    std::fs::write(
        &plain,
        "export function one() {}\nexport function two() {}\nexport const three = 3;\n",
    )
    .expect("plain");
    for (source, top_level, all) in [(&class, 3, 5), (&plain, 3, 3)] {
        let out = run(
            &root.0,
            json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":source}),
        )
        .expect("symbols");
        assert!(all_objects(&out["symbols"]), "{out}");
        let rows = out["symbols"].as_array().expect("rows");
        assert_eq!(rows.len(), top_level, "{out}");
        assert_eq!(out["pagination"]["totalItems"], top_level, "{out}");
        assert_eq!(outline_names(&out["symbols"]).len(), all, "{out}");
        // Pages of one top-level row each reach every row once, members
        // with their parent.
        let start = json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":source,"pageSize":1});
        let mut paged = Vec::new();
        checks::walk("astSearch", json!({ "queries": [start] }), |call| {
            let row = &call["queries"][0];
            let page = run(&root.0, row.clone()).expect("page");
            assert_eq!(page["pagination"]["totalItems"], top_level, "{page}");
            assert!(all_objects(&page["symbols"]), "{page}");
            assert_eq!(page["symbols"].as_array().map(Vec::len), Some(1), "{page}");
            let rows = page["symbols"].as_array().expect("rows").clone();
            paged.extend(rows.iter().cloned());
            checks::Page {
                output: crate::tools::test_support::published(
                    crate::id::ToolId::AstSearch,
                    row,
                    &page,
                ),
                rows: rows.iter().map(serde_json::Value::to_string).collect(),
            }
        });
        assert_eq!(json!(paged), out["symbols"], "{source:?}");
    }
    // A directory outline groups the same object rows under each file and
    // counts each file's top-level rows.
    let out = run(
        &root.0,
        json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":root.0}),
    )
    .expect("directory symbols");
    let files = out["files"].as_array().expect("files");
    assert!(
        files.iter().all(|file| all_objects(&file["symbols"])),
        "{out}"
    );
    let rows: usize = files
        .iter()
        .map(|file| file["symbols"].as_array().map_or(0, Vec::len))
        .sum();
    assert_eq!(out["pagination"]["totalItems"], rows, "{out}");
    crate::contracts::validate_output("astSearch", &json!({"results":[{"index":0,"data":out}]}))
        .expect("object rows satisfy the output contract");
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
    // The first page (the default), without any cursor.
    for cursor in ["page", "snapshot", "scanOffset"] {
        assert!(
            restart["query"]["queries"][0].get(cursor).is_none(),
            "{cursor}: {stale}"
        );
    }
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
        json!(crate::response::symbol_outline::outline_rows(
            &crate::response::symbol_outline::flatten_members(
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
    // Pages count top-level rows (struct A, two impl A, two x): page 2 of
    // 4 holds the last `x`, and no member is cut from its parent.
    assert_eq!(out["pagination"]["totalItems"], 5, "{out}");
    let second = run(
        &root.0,
        json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":source,"pageSize":4,"page":2,
            "snapshot":out_snapshot(&root.0, &source)}),
    )
    .expect("second page");
    assert_eq!(
        crate::response::symbol_outline::outline_rows(
            &crate::response::symbol_outline::flatten_members(
                second["symbols"].as_array().expect("rows")
            )
        ),
        [json!("8 function x col 14")],
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
        json!(crate::response::symbol_outline::outline_rows(
            &crate::response::symbol_outline::flatten_members(
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

#[test]
fn directory_symbols_keep_row_paths_without_static_notes() {
    let root = Fixture::new();
    root.write("a.rs", "fn a() {}\n");
    root.write("b.rs", "fn b() {}\n");
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
    // A partial page without `next` still names why it is partial.
    assert_eq!(out["partialReasons"], json!(["parseRecovered"]), "{out}");
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
    if crate::tools::leads::lsp::verify_query(
        &source.to_string_lossy(),
        "helper",
        3,
        crate::tools::leads::lsp::Verify::Callers,
    )
    .is_none()
    {
        assert!(one["next"].get("findCallers").is_none(), "{one}");
        return;
    }
    let lead = &one["next"]["findCallers"];
    assert_eq!(lead["tool"], "lspSearch", "{one}");
    // A function's usual next question is its call sites.
    assert_eq!(lead["query"]["queries"][0]["operation"], "callers", "{one}");
    assert_eq!(lead["query"]["queries"][0]["symbolName"], "helper", "{one}");
    assert_eq!(lead["query"]["queries"][0]["line"], 3, "{one}");
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
    let uri = dir["next"]["findCallers"]["query"]["queries"][0]["path"]
        .as_str()
        .expect("directory path");
    assert_eq!(
        std::path::Path::new(uri),
        source.canonicalize().expect("canonical"),
        "{dir}"
    );
}

#[test]
fn directory_symbols_by_name_parse_only_files_that_spell_the_name() {
    let root = Fixture::new();
    root.write("a.rs", "pub fn helper() {}\n");
    // No file without the name is parsed: its recovered-parse diagnostic
    // cannot mark the named outline partial.
    root.write("b.rs", "fn broken( {\n");
    root.write("c.rs", "fn Helper_case() {}\n");
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

/// A single-file outline leads to its natural next read: the top
/// declaration's lines; a directory outline names no single read.
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

/// A parse-recovery note repeated by several files that list declarations
/// is one counted entry naming every file, not one entry per file.
#[test]
fn symbols_count_one_recovery_note_across_files_that_list_declarations() {
    let root = Fixture::new();
    for index in 1..=3 {
        root.write(
            &format!("f{index}.rs"),
            format!("fn foo_{index}() {{}}\nfn caller() {{ foo_{index}(; }}\n"),
        );
    }
    let out = run(
        &root.0,
        json!({"operation":"symbols","path":root.0,"symbolName":"foo_","pageSize":1}),
    )
    .expect("symbols");
    let diagnostics = out["diagnostics"].as_array().expect("diagnostics");
    assert_eq!(diagnostics.len(), 1, "{out}");
    let message = diagnostics[0]["message"].as_str().expect("message");
    assert!(message.starts_with("tree-sitter recovered"), "{out}");
    assert!(message.ends_with("(3 files)"), "{out}");
    let mut files = diagnostics[0]["files"]
        .as_array()
        .expect("files")
        .iter()
        .filter_map(|file| file["path"].as_str())
        .map(|path| path.rsplit('/').next().unwrap_or(path).to_owned())
        .collect::<Vec<_>>();
    files.sort();
    assert_eq!(files, ["f1.rs", "f2.rs", "f3.rs"], "{out}");
    assert_eq!(out["partialReasons"], json!(["parseRecovered"]), "{out}");
}

/// Files whose parse-recovery notes share a message share one counted
/// entry with every path, and a text search completes the name lookup the
/// recovered parses may have missed.
#[test]
fn symbols_group_recovery_notes_of_files_without_a_listed_declaration() {
    let root = Fixture::new();
    root.write("a.rs", "fn sdsnewlen() {}\n");
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
    let lead = &out["next"]["findText"];
    assert_eq!(lead["tool"], "localSearch", "{out}");
    assert_eq!(
        lead["query"]["queries"][0]["matchString"], "sdsnewlen",
        "{out}"
    );
}

/// A recovered parse can hide a declaration of the queried name only
/// where its syntax errors spell that name. A file that only calls the name
/// in code that parsed keeps no note and leaves the outline complete.
#[test]
fn recovered_notes_only_for_files_whose_errors_spell_the_name() {
    let root = Fixture::new();
    root.write("a.c", "int foo(int x) { return x; }\n");
    // Calls `foo` on a clean line; the syntax error is elsewhere.
    root.write(
        "b.c",
        "int caller(void) { return foo(1); }\n\nint broken( {\n  return 2;\n}\n",
    );
    // The syntax error spells `foo`: a declaration could hide there.
    root.write("c.c", "int user(void) {\n  int foo( = 3;\n}\n");
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

/// A name-filtered outline that lists a declaration from a recovered parse
/// cites a partial inventory: the file keeps its note and the page is
/// partial, wherever the syntax error sits. Recovered files that list
/// nothing and whose errors do not spell the name stay out.
#[test]
fn a_recovered_file_that_lists_the_named_declaration_keeps_its_note() {
    let root = Fixture::new();
    for index in 1..=3 {
        root.write(
            &format!("f{index}.rs"),
            format!("pub fn helper{index}() {{}}\nfn broken( {{\n"),
        );
    }
    let out = run(
        &root.0,
        json!({"operation":"symbols","path":root.0,"symbolName":"helper2"}),
    )
    .expect("symbols");
    assert_eq!(
        outline_names(&out["files"][0]["symbols"]),
        ["helper2"],
        "{out}"
    );
    let diagnostics = out["diagnostics"].as_array().expect("diagnostics");
    assert_eq!(diagnostics.len(), 1, "{out}");
    let noted = diagnostics[0].to_string();
    assert!(noted.contains("f2.rs"), "{out}");
    assert!(
        !noted.contains("f1.rs") && !noted.contains("f3.rs"),
        "{out}"
    );
    assert_eq!(out["isPartial"], true, "{out}");
    assert_eq!(out["partialReasons"], json!(["parseRecovered"]), "{out}");
}

/// An empty symbols search says what it searched (the name and the
/// file count) and leads to the name's text.
#[test]
fn empty_symbols_hint_names_file_count_and_text_lead() {
    let root = Fixture::new();
    root.write("a.rs", "fn alpha() {}\n");
    root.write("b.rs", "fn beta() { missing_name(); }\n");
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
    let lead = &out["next"]["findText"];
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
    assert!(outline["next"].get("findText").is_none(), "{outline}");
}

/// A one-line `typedef struct X X;` declares
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
    let flat = crate::response::symbol_outline::flatten_members(rows.as_array().expect("rows"));
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

/// A directory outline discloses the default-excluded directories it
/// pruned, and an empty one leads to the same query over them.
#[test]
fn symbols_disclose_default_excluded_directories() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("node_modules/pkg")).expect("dirs");
    root.write(
        "node_modules/pkg/index.ts",
        "export function hiddenHelper() {}\n",
    );
    root.write("main.ts", "export const x = 1;\n");
    let out = run(
        &root.0,
        json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":root.0,"symbolName":"hiddenHelper"}),
    )
    .expect("symbols");
    let warnings = out["warnings"].to_string();
    assert!(warnings.contains("node_modules/"), "{out}");
    let lead = &out["next"]["includeSkipped"];
    assert_eq!(lead["tool"], "astSearch", "{out}");
    let query = &lead["query"]["queries"][0];
    assert_eq!(query["defaultExcludes"], false, "{out}");
    assert_eq!(query["symbolName"], "hiddenHelper", "{out}");
    assert!(query.get("snapshot").is_none(), "{out}");
    let all = run(&root.0, query.clone()).expect("lead runs");
    assert!(
        all["files"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|file| file["symbols"].as_array().into_iter().flatten())
            .any(|symbol| symbol["symbolName"] == "hiddenHelper"),
        "{all}"
    );
}

/// An empty one-file outline counts "1 file", not "1 files".
#[test]
fn an_empty_single_file_outline_counts_one_file() {
    let root = Fixture::new();
    let source = root.0.join("a.ts");
    std::fs::write(&source, "x();\n").expect("source");
    let out = run(&root.0, json!({"operation":"symbols","path":source})).expect("symbols");
    assert_eq!(out["status"], "empty", "{out}");
    assert_eq!(out["hints"], json!(["0 declarations in 1 file."]), "{out}");
}
