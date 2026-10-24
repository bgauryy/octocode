//! Outline rows: compact objects, twin members, trait impls and
//! top-level row counts.

use super::*;

#[test]
fn single_file_symbols_are_compact_and_path_free() {
    let root = Fixture::new();
    let source = root.0.join("shapes.rs");
    std::fs::write(
        &source,
        "struct A;\nimpl A {\n    fn run() {}\n}\nstruct B; impl B { fn run() {} }\n",
    )
    .expect("source");
    let out = run(&root.0, json!({"operation":"symbols","path":source})).expect("symbols");
    assert_eq!(out["path"], json!("shapes.rs"));
    assert!(
        out["symbols"]
            .as_array()
            .expect("symbols")
            .iter()
            .all(|row| row.get("path").is_none())
    );
    assert_eq!(out["isPartial"], false, "{out}");
    // One declaration row each: no path, ranges or engine ids; an endLine
    // only when the declaration spans lines; members nested under the `impl`
    // that holds them.
    assert_eq!(
        outline_facts(&out["symbols"], &Value::Null),
        json!([
            {"line": 1, "kind": "struct", "name": "A"},
            {"line": 2, "endLine": 4, "kind": "impl", "name": "A", "members": [
                {"line": 3, "kind": "method", "name": "run"}
            ]},
            {"line": 5, "kind": "struct", "name": "B"},
            {"line": 5, "kind": "impl", "name": "B", "members": [
                {"line": 5, "kind": "method", "name": "run"}
            ]}
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
        let out = run(&root.0, json!({"operation":"symbols","path":source})).expect("symbols");
        assert!(all_objects(&out["symbols"]), "{out}");
        let rows = out["symbols"].as_array().expect("rows");
        assert_eq!(rows.len(), top_level, "{out}");
        assert_eq!(out["pagination"]["totalItems"], top_level, "{out}");
        assert_eq!(outline_names(&out["symbols"]).len(), all, "{out}");
        // Pages of one top-level row each reach every row once, members
        // with their parent.
        let start = json!({"operation":"symbols","path":source});
        let mut paged = Vec::new();
        checks::walker::walk("astSearch", json!({"queries":[start]}), |call| {
            let input =
                crate::tools::test_support::opened_input(crate::id::ToolId::AstSearch, call);
            let row = &input["queries"][0];
            let page = crate::tools::ast_search::test_rows::run_window(&root.0, row.clone(), 1)
                .expect("page");
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
    let out =
        run(&root.0, json!({"operation":"symbols","path":root.0})).expect("directory symbols");
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
fn symbols_outline_places_twin_members_and_columns_only_when_ambiguous() {
    let root = Fixture::new();
    let source = root.0.join("twins.rs");
    std::fs::write(
        &source,
        "struct A;\nimpl A {\n    fn one() {}\n}\nimpl A {\n    fn two() {}\n}\nfn x() {} fn x() {}\n",
    )
    .expect("source");
    let out = run(&root.0, json!({"operation":"symbols","path":source})).expect("symbols");
    // The two `impl A` blocks are two rows with their own ranges, so `two`
    // (line 6) sits in the second; the two `x` on one line differ only by
    // column, so only they carry it.
    assert_eq!(
        outline_facts(&out["symbols"], &Value::Null),
        json!([
            {"line": 1, "kind": "struct", "name": "A"},
            {"line": 2, "endLine": 4, "kind": "impl", "name": "A", "members": [
                {"line": 3, "kind": "method", "name": "one"}
            ]},
            {"line": 5, "endLine": 7, "kind": "impl", "name": "A", "members": [
                {"line": 6, "kind": "method", "name": "two"}
            ]},
            {"line": 8, "kind": "function", "name": "x", "column": 4},
            {"line": 8, "kind": "function", "name": "x", "column": 14}
        ]),
        "{out}"
    );
    assert_eq!(out["pagination"]["totalItems"], 5, "{out}");
    let mut request = json!({"operation":"symbols","path":source});
    let mut rows = Vec::new();
    loop {
        let page =
            crate::tools::ast_search::test_rows::run_window(&root.0, request, 1).expect("page");
        rows.extend(page["symbols"].as_array().expect("rows").iter().cloned());
        let next = &page["next"]["nextPage"]["input"]["queries"][0];
        if next.is_null() {
            break;
        }
        request = next.clone();
    }
    assert_eq!(json!(rows), out["symbols"]);
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
        outline_facts(&out["symbols"], &Value::Null),
        json!([
            {"line": 1, "kind": "struct", "name": "Pipe", "exported": true},
            {"line": 2, "endLine": 7, "kind": "impl", "name": "Pipe", "members": [
                {"line": 3, "kind": "method", "name": "read", "exported": true},
                {"line": 5, "kind": "method", "name": "read_buf", "exported": true}
            ]},
            {"line": 8, "endLine": 10, "kind": "impl", "name": "AsyncWrite for Pipe", "members": [
                {"line": 9, "kind": "method", "name": "poll_flush"}
            ]},
            {"line": 11, "endLine": 13, "kind": "impl", "name": "std::fmt::Debug for Wrap", "members": [
                {"line": 12, "kind": "method", "name": "fmt"}
            ]}
        ]),
        "{out}"
    );
    // The type name still selects every impl of the type; the lead anchors
    // lspSearch on the type, as for an inherent impl.
    let named = run(
        &root.0,
        json!({"operation":"symbols","path":source,"symbolFilter":{"terms":["Pipe"]},"symbolKinds":["impl"]}),
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
        json!({"operation":"symbols","path":source,"symbolFilter":{"terms":["AsyncWrite for Pipe"]}}),
    )
    .expect("trait impl");
    assert_eq!(
        outline_name(&trait_only["symbols"][0]),
        "AsyncWrite for Pipe"
    );
    // The lead exists only where rust-analyzer runs.
    if let Some(lead) = trait_only["hints"].get("references") {
        assert_eq!(
            lead["input"]["queries"][0]["symbolName"], "Pipe",
            "{trait_only}"
        );
    }
}

#[test]
fn directory_symbols_keep_row_paths_without_static_notes() {
    let root = Fixture::new();
    root.write("a.rs", "fn a() {}\n");
    root.write("b.rs", "fn b() {}\n");
    let out = run(&root.0, json!({"operation":"symbols","path":root.0})).expect("symbols");
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
    let out = run(&root.0, json!({"operation":"symbols","path":source})).expect("symbols");
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
