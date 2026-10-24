//! Name filters and the leads a named or single-file outline offers.

use super::*;

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
        json!({"operation":"symbols","path":source,"symbolFilter":{"terms":["complete","try_read_output"]}}),
    )
    .expect("list");
    assert_eq!(names(&list), ["complete", "try_read_output"], "{list}");
    let single = run(
        &root.0,
        json!({"operation":"symbols","path":source,"symbolFilter":{"terms":["complete"]}}),
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
        let mut row = json!({"operation":"symbols","path":source});
        row["symbolFilter"] = json!({"terms":query});
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
    assert_eq!(names(json!(["get"])), ["get"]);
    assert_eq!(names(json!(["ge"])), ["get", "aget", "get_or_create"]);
    // All-lowercase text ignores case; any capital makes it exact-case.
    assert_eq!(names(json!(["native"])), ["Native"]);
    assert!(names(json!(["NATIVE"])).is_empty());
}

/// An exact `symbolName` match says what it left out: the declarations that
/// only contain the name are counted, and `broadenSearch` lists every one of
/// them (no name is dropped from a notice). Later pages do not repeat it.
#[test]
fn an_exact_name_reports_the_declarations_it_hides() {
    let root = Fixture::new();
    let source = root.0.join("hide.rs");
    std::fs::write(
        &source,
        "fn get() {}
fn aget() {}
fn get_or_create() {}
fn get_or_create() {}
",
    )
    .expect("source");
    let run_row = |row: serde_json::Value| run(&root.0, row).expect("symbols");
    let base = json!({"operation":"symbols","path":source});
    let mut exact_row = base.clone();
    exact_row["symbolFilter"] = json!({"terms":["get"]});
    let exact = run_row(exact_row.clone());
    let warning = exact["warnings"][0].as_str().unwrap_or_default();
    assert!(
        warning.ends_with(
            "so 3 declarations that only contain it are not listed: aget, get_or_create. Pass these names to list them."
        ),
        "{exact}"
    );
    let broaden = &crate::tools::test_support::checks::related_call(&exact, "astSearch", |_| true)
        ["input"]["queries"][0];
    assert_eq!(broaden["symbolFilter"]["terms"], json!(["get"]), "{exact}");
    let broadened = run_row(broaden.clone());
    assert_eq!(
        outline_names(&broadened["symbols"]),
        ["get", "aget", "get_or_create", "get_or_create"],
        "{broadened}"
    );
    assert!(broadened.get("warnings").is_none(), "{broadened}");
    // A substring search hides nothing, so it says nothing.
    let mut substring = base;
    substring["symbolFilter"] = json!({"terms":["ge"],"mode":"substring"});
    assert!(run_row(substring).get("warnings").is_none());
}

/// `kinds` never turns an exact `symbolName` into a substring search: a name
/// declared only with kinds the filter leaves out stays exact, the row says
/// which kinds hold it, and `broadenSearch` repeats the query with them.
/// Kinds match the shown `kind` exactly in every language (`function` is not
/// `method`).
#[test]
fn kinds_that_leave_out_an_exact_name_say_so_instead_of_matching_substrings() {
    let root = Fixture::new();
    let source = root.0.join("decide.rs");
    std::fs::write(
        &source,
        "struct Engine;\nimpl Engine {\n    fn decide(&self) {}\n}\nstruct Other;\nimpl Other {\n    fn decide(&self) {}\n}\nfn matcher_options_decide_the_match_count() {}\n",
    )
    .expect("source");
    let row = |path: &std::path::Path, kinds: serde_json::Value| json!({"operation":"symbols","path":path,"symbolFilter":{"terms":["decide"]},"symbolKinds":kinds});
    for path in [source.as_path(), root.0.as_path()] {
        let out = run(&root.0, row(path, json!(["function"]))).expect("symbols");
        // A file outline lists `symbols`; a directory groups them in `files`.
        let listed = |out: &serde_json::Value| {
            let files = out["files"].as_array().cloned().unwrap_or_default();
            std::iter::once(&out["symbols"])
                .chain(files.iter().map(|file| &file["symbols"]))
                .filter(|rows| rows.is_array())
                .flat_map(|rows| outline_names(rows).into_iter().map(str::to_owned))
                .collect::<Vec<_>>()
        };
        // No substring stands in for the exact name.
        assert!(listed(&out).is_empty(), "{out}");
        let warnings = out["warnings"].to_string();
        assert!(
            warnings.contains(
                "kinds function leaves out every declaration named decide: 2 of kind method. Add method to kinds to list them."
            ),
            "{out}"
        );
        // The substring declaration the exact name hides is disclosed too.
        assert!(
            warnings.contains("matcher_options_decide_the_match_count"),
            "{out}"
        );
        let broaden =
            &crate::tools::test_support::checks::related_call(&out, "astSearch", |_| true)["input"]
                ["queries"][0];
        assert_eq!(
            broaden["symbolKinds"],
            json!(["function", "method"]),
            "{out}"
        );
        assert_eq!(broaden["symbolFilter"]["terms"], json!(["decide"]), "{out}");
        let broadened = run(&root.0, broaden.clone()).expect("broadened");
        assert_eq!(listed(&broadened), ["decide", "decide"], "{broadened}");
        assert!(
            !broadened["warnings"]
                .to_string()
                .contains("leaves out every"),
            "{broadened}"
        );
        // The second suppressed-name lead follows the kinds lead.
        assert_eq!(
            crate::tools::test_support::checks::related_call(
                &out,
                "astSearch",
                |input| input["queries"][0]["symbolFilter"]["mode"] == "substring"
            )["input"]["queries"][0]["symbolFilter"]["terms"],
            json!(["decide"]),
            "{out}"
        );
    }
    // Kinds that keep one exact declaration say nothing about the others.
    let kept = run(&root.0, row(&source, json!(["method"]))).expect("method kinds");
    assert!(
        !kept["warnings"].to_string().contains("leaves out every"),
        "{kept}"
    );
}

#[test]
fn a_single_named_declaration_leads_to_its_callers() {
    let root = Fixture::new();
    let source = root.0.join("lead.rs");
    std::fs::write(&source, "struct S;\n\nfn helper() {}\nfn helper_two() {}\n").expect("source");
    let symbols = |terms: serde_json::Value| {
        run(
            &root.0,
            json!({"operation":"symbols","path":source,"symbolFilter":{"terms":terms}}),
        )
        .expect("symbols")
    };
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
    let lead = crate::tools::test_support::checks::related_call(&one, "lspSearch", |input| {
        input["queries"][0]["operation"] == "callers"
    });
    assert_eq!(lead["tool"], "lspSearch", "{one}");
    // A function's usual next question is its call sites.
    assert_eq!(lead["input"]["queries"][0]["operation"], "callers", "{one}");
    assert_eq!(lead["input"]["queries"][0]["symbolName"], "helper", "{one}");
    assert_eq!(lead["input"]["queries"][0]["line"], 3, "{one}");
    let uri = lead["input"]["queries"][0]["path"].as_str().expect("uri");
    assert!(
        std::path::Path::new(uri).is_absolute() && uri.ends_with("lead.rs"),
        "{uri}"
    );
    // Several declarations, or no name filter, name no single symbol.
    for many in [
        symbols(json!(["help"])),
        run(&root.0, json!({"operation":"symbols","path":source})).expect("unfiltered"),
    ] {
        assert!(many["next"].get("callers").is_none(), "{many}");
    }
    // A directory scope resolves the row's file for the lead.
    let dir = run(
        &root.0,
        json!({"operation":"symbols","path":root.0,"symbolFilter":{"terms":["helper"]}}),
    )
    .expect("directory symbols");
    let uri = crate::tools::test_support::checks::related_call(&dir, "lspSearch", |input| {
        input["queries"][0]["operation"] == "callers"
    })["input"]["queries"][0]["path"]
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
        json!({"operation":"symbols","path":root.0,"symbolFilter":{"terms":["helper"]}}),
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
        json!({"operation":"symbols","path":root.0,"symbolFilter":{"terms":["helper_case"]}}),
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
    let read = crate::tools::test_support::checks::related_call(&outline, "localFetch", |_| true);
    assert_eq!(read["tool"], "localFetch", "{outline}");
    assert_eq!(
        read["input"]["queries"][0]["ranges"],
        json!([{"startLine":1,"endLine":3}]),
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
