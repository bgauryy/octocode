use super::*;
use serde_json::json;

fn sample() -> Value {
    json!({"results":[{"index":0,"rowPart":{"part":1,"of":2},"data":{
            "matchCount":2,"matchedLineCount":2,
            "next":{"nextPage":{"kind":"continue","tool":"localSearch","input":{"cursor":"issued-tool-page"}}},
            "files":[{"path":"a.rs","matches":[
                {"line":26,"value":"25\tuse a;\n26\tuse SearcherBuilder;\n27\tuse b;"},
                {"line":997,"value":"996\t) {\n997\t  let sb = SearcherBuilder::new();",
                 "enclosing":{"symbolName":"build","kind":"function","line":992,"endLine":1016}}]}]}}],
            "warnings":["incomplete"],
            "responsePagination":{"hasMore":true,"next":{"kind":"continue","tool":"localSearch","input":{"cursor":"issued-response-page"}}},
            "root":"/nowhere"})
}

fn context() -> Value {
    json!({"queries":[{"path":"s","matchString":["x"],"contextLines":{"before":1,"after":1}}]})
}

#[test]
fn hits_print_like_rg_and_lose_no_field() {
    let lines = render(ToolId::LocalSearch, &sample(), &context(), None).expect("lines");
    assert_eq!(
        lines.out,
        "a.rs\n25-use a;\n26:use SearcherBuilder;\n27-use b;\n--\n996-) {\n997:  let sb = SearcherBuilder::new();  [function build 992-1016]"
    );
    for token in ["issued-tool-page", "issued-response-page"] {
        assert_eq!(lines.notes.matches(token).count(), 1, "{}", lines.notes);
    }
    assert!(
        lines.notes.contains("# rowPart: part 1 of 2"),
        "{}",
        lines.notes
    );
    assert!(
        lines.notes.contains("# warnings: incomplete"),
        "{}",
        lines.notes
    );
    for evidence in [
        "use a;",
        "use SearcherBuilder;",
        "use b;",
        "let sb = SearcherBuilder::new();",
    ] {
        assert!(lines.out.contains(evidence));
    }
}

#[test]
fn numbering_follows_the_query_and_matched_lines() {
    // Without context, a `N\t` prefix is the line's own text.
    let tsv = json!({"results":[{"index":0,"data":{"files":[{"path":"t","matches":[{"line":3,"value":"3\tfoo"}]}]}}]});
    assert_eq!(
        render(ToolId::LocalSearch, &tsv, &json!({}), None)
            .expect("lines")
            .out,
        "t:3:3\tfoo"
    );
    // With context, `matchedLines` marks every hit.
    let merged = json!({"results":[{"index":0,"data":{"files":[{"path":"a","matches":[
            {"line":2,"matchedLines":[2,3],"value":"1\tx\n2\tfoo\n3\tfoo"}]}]}}]});
    assert_eq!(
        render(ToolId::LocalSearch, &merged, &context(), None)
            .expect("lines")
            .out,
        "a\n1-x\n2:foo\n3:foo"
    );
    // A clipped snippet that is not numbered prints as `key: value` text.
    let clipped = json!({"results":[{"index":0,"data":{"files":[{"path":"a","matches":[
            {"line":6,"value":"xxx…","clipped":true}]}]}}]});
    let lines = render(ToolId::LocalSearch, &clipped, &context(), None).expect("text");
    assert_eq!(
        lines.out,
        "files:\n  - path: a\n    matches:\n      - line 6 value xxx… clipped"
    );
}

#[test]
fn empty_values_crlf_and_clipped_structural_matches() {
    let blank = json!({"results":[{"index":0,"data":{"files":[{"path":"a","matches":[{"line":4,"value":""},{"line":5,"value":"x\r"}]}]}}]});
    assert_eq!(
        render(ToolId::LocalSearch, &blank, &json!({}), None)
            .expect("lines")
            .out,
        "a:4:\na:5:x\r"
    );
    let ast = json!({"results":[{"index":0,"data":{"files":[{"path":"m.ts","matches":[
            {"line":1,"endLine":3,"column":1,"value":"function foo(a) …","clipped":true}]}]}}]});
    assert_eq!(
        render(ToolId::AstSearch, &ast, &json!({}), None)
            .expect("lines")
            .out,
        "m.ts:1:function foo(a) …  [endLine 3 column 1 clipped]"
    );
}

#[test]
fn lists_counts_and_empty_results() {
    let files = json!({"results":[{"index":0,"data":{"files":[{"path":"a"},{"path":"b"}]}}]});
    assert_eq!(
        render(ToolId::LocalSearch, &files, &json!({}), None)
            .expect("lines")
            .out,
        "a\nb"
    );
    let counts = json!({"results":[{"index":0,"data":{"matchCount":3,"files":[{"path":"a","matchCount":3}]}}]});
    let lines = render(ToolId::LocalSearch, &counts, &json!({}), None).expect("lines");
    assert_eq!(lines.out, "a:3");
    assert_eq!(lines.notes, "# matchCount: 3");
    let empty = json!({"results":[{"index":0,"status":"empty","data":{"matchCount":0,
            "hints":["No matches.","includeSkipped: localSearch {\"queries\":[{\"path\":\".\"}]}"]}}]});
    let lines = render(ToolId::LocalSearch, &empty, &json!({}), None).expect("lines");
    assert_eq!(lines.out, "");
    assert!(lines.notes.contains("# status: empty\n"), "{}", lines.notes);
    assert!(
        lines.notes.contains("# hints: No matches."),
        "{}",
        lines.notes
    );
    assert!(lines.notes.contains("includeSkipped: localSearch"));
    assert!(
        !lines.notes.contains("octocode localSearch"),
        "prose is never promoted to a command"
    );
}

#[test]
fn declarations_and_listings_print_one_line_each() {
    let symbols = json!({"results":[{"index":0,"data":{"filesScanned":2,"files":[
            {"path":"a.rs","symbols":[
                {"symbolName":"render","kind":"function","line":31,"endLine":113,"docStartLine":29},
                {"symbolName":"Visit","kind":"impl","line":34,"endLine":43,
                 "members":[{"kind":"method","symbolName":"fresh","line":37,"exported":true}]}]}]}}]});
    let lines = render(ToolId::AstSearch, &symbols, &json!({}), None).expect("lines");
    assert_eq!(
        lines.out,
        "a.rs:31: function render  [31-113 docStartLine 29]\na.rs:34: impl Visit  [34-43]\na.rs:37:   method fresh  [exported]"
    );
    assert!(!lines.notes.contains('{'), "{}", lines.notes);
    let file = json!({"results":[{"index":0,"data":{"path":"a.rs","filesScanned":1,"symbols":[
            {"symbolName":"PREFIX","kind":"constant","line":9},
            {"symbolName":"Cursor","kind":"impl","line":94,"endLine":162,
             "members":[{"kind":"method","symbolName":"encode","line":115,"endLine":124}]}]}}]});
    for tool in [ToolId::AstSearch, ToolId::LspSearch] {
        let lines = render(tool, &file, &json!({}), None).expect("lines");
        assert_eq!(
            lines.out,
            "a.rs:9: constant PREFIX\na.rs:94: impl Cursor  [94-162]\na.rs:115:   method encode  [115-124]"
        );
        assert!(!lines.notes.contains("symbols"), "{}", lines.notes);
    }
    let listed = json!({"results":[{"index":0,"data":{"path":"src","files":[
            {"dir":"src","files":["a (1).rs (12)","b.rs (34, symlink)","sub/"]}],
            "summary":{"files":2,"dirs":1,"bytes":46}}}]});
    let files = json!({"operation":"files","path":"src"});
    // `details` (the default view): every field the entry carries, the size as bytes.
    let lines = render(ToolId::StructureSearch, &listed, &files, None).expect("lines");
    assert_eq!(
        lines.out,
        "src/a (1).rs  [12 B]\nsrc/b.rs  [34 B, symlink]\nsrc/sub/"
    );
    // `paths`: bare names, never parsed, however they spell ` (…)`.
    let named = json!({"results":[{"index":0,"data":{"path":".","files":[
            {"dir":".","files":["report (123)","report (draft)","report (draft).md","dir (x)/"]}],
            "summary":{"files":3,"dirs":1,"bytes":0}}}]});
    let paths = json!({"operation":"files","path":".","view":"paths"});
    let lines = render(ToolId::StructureSearch, &named, &paths, None).expect("lines");
    assert_eq!(
        lines.out,
        "report (123)\nreport (draft)\nreport (draft).md\ndir (x)/"
    );
    // A listing without its operation (a continuation) prints as text.
    let lines = render(
        ToolId::StructureSearch,
        &listed,
        &json!({"path":"src"}),
        None,
    )
    .expect("text");
    assert!(
        lines.out.contains("\nfiles:\n  - dir: src\n"),
        "{}",
        lines.out
    );
    assert!(
        lines.out.contains("      - b.rs (34, symlink)\n"),
        "{}",
        lines.out
    );
}

#[test]
fn lsp_reference_sites_print_as_hit_lines() {
    let sites = json!({"results":[{"index":0,"data":{"kind":"references","files":[
            {"path":"a.rs","matches":[{"line":7,"column":3,"value":"run();"}]}]}}]});
    let lines = render(ToolId::LspSearch, &sites, &json!({}), None).expect("lines");
    assert_eq!(lines.out, "a.rs:7:run();  [column 3]");
    assert!(
        lines.notes.contains("# kind: references"),
        "{}",
        lines.notes
    );
}

#[test]
fn lsp_results_print_as_lines() {
    let calls = json!({"results":[{"index":0,"data":{"kind":"callers","files":[{"path":"m.rs","matches":[
            {"symbolName":"walk","kind":"function","line":79,"endLine":89,"sites":[{"line":83,"column":36},{"line":85,"column":4}]}]}]}}]});
    let lines = render(ToolId::LspSearch, &calls, &json!({}), None).expect("lines");
    assert_eq!(
        lines.out,
        "m.rs:79: function walk  [79-89 sites 83:36,85:4]"
    );
    // A callee declared in another file prints under its own file, with the
    // sites under the row's, so every location is the real one.
    let callees = json!({"results":[{"index":0,"data":{"kind":"callees","files":[{"path":"src/calls.ts","matches":[
            {"symbolName":"target","kind":"function","line":9,"endLine":11,"path":"src/lib.ts","sites":[{"line":19,"column":14},{"line":21,"column":12}]}]}]}}]});
    let lines = render(ToolId::LspSearch, &callees, &json!({}), None).expect("lines");
    assert_eq!(
        lines.out,
        "src/lib.ts:9: function target  [9-11 sites src/calls.ts:19:14,src/calls.ts:21:12]"
    );
    assert!(!lines.notes.contains("path src/lib.ts"), "{}", lines.notes);
    // `view:"files"`: a `path:N` line per site; the count is theirs.
    let grouped = json!({"results":[{"index":0,"data":{"matchCount":3,"files":[
            {"path":"a.rs","matchCount":2,"lines":[14,275]},{"path":"b.rs","matchCount":1,"lines":[3]}]}}]});
    let lines = render(ToolId::LspSearch, &grouped, &json!({}), None).expect("lines");
    assert_eq!(lines.out, "a.rs:14\na.rs:275\nb.rs:3");
    assert_eq!(lines.notes, "# matchCount: 3");
    // A declaration body prints as an `rg --heading` block; its
    // omission marker is a note.
    let definition = json!({"results":[{"index":0,"data":{"kind":"definition","matches":[
            {"path":"w.rs","displayRange":{"startLine":73,"startCharacter":8,"endLine":73},
             "content":"72\t/// doc\n73\tpub fn f(\n74\t) {}\n… 40 more lines omitted (source lines 133-172)."},
            {"path":"w.rs","displayRange":{"startLine":90,"startCharacter":4,"endLine":90},"content":"90\tf();\n"}]}}]});
    let lines = render(ToolId::LspSearch, &definition, &json!({}), None).expect("lines");
    assert_eq!(
        lines.out,
        "w.rs\n72-/// doc\n73:pub fn f(  [column 8]\n74-) {}\n--\n90:f();  [column 4]"
    );
    assert!(
        lines
            .notes
            .contains("# file w.rs:73: … 40 more lines omitted (source lines 133-172)."),
        "{}",
        lines.notes
    );
    // One-line locations print as hits; symbols as declarations.
    let single = json!({"results":[{"index":0,"data":{"matches":[
            {"path":"w.rs","displayRange":{"startLine":23,"startCharacter":10,"endLine":23},"content":"23\t#[derive(Clone)]","isDefinition":true}]}}]});
    let lines = render(ToolId::LspSearch, &single, &json!({}), None).expect("lines");
    assert_eq!(
        lines.out,
        "w.rs:23:#[derive(Clone)]  [column 10 isDefinition]"
    );
    let symbols = json!({"results":[{"index":0,"data":{"kind":"symbols","matches":[
            {"symbolName":"walk","kind":"function","path":"m.rs","displayRange":{"startLine":79,"startCharacter":15,"endLine":79}}]}}]});
    let lines = render(ToolId::LspSearch, &symbols, &json!({}), None).expect("lines");
    assert_eq!(lines.out, "m.rs:79: function walk  [column 15]");
    // Content the line format cannot place prints as `key: value` text.
    let unavailable = json!({"results":[{"index":0,"data":{"matches":[
            {"path":"w.rs","displayRange":{"startLine":3,"startCharacter":1,"endLine":3},"content":"[content unavailable: binary]"}]}}]});
    let lines = render(ToolId::LspSearch, &unavailable, &json!({}), None).expect("text");
    assert!(
        lines.out.contains("content: [content unavailable: binary]"),
        "{}",
        lines.out
    );
}

#[test]
fn nested_file_fields_are_words() {
    let paged = json!({"results":[{"index":0,"data":{"files":[{"path":"a.rs","matches":[{"line":3,"value":"x"}],
            "pagination":{"totalItems":471,"hasMore":true,"moreLines":[44,49],"moreLinesUnlisted":437}}]}}]});
    let lines = render(ToolId::LocalSearch, &paged, &json!({}), None).expect("lines");
    assert_eq!(
        lines.notes,
        "# file a.rs: pagination (totalItems 471 hasMore moreLines 44,49 moreLinesUnlisted 437)"
    );
}

#[test]
fn match_notes_are_words() {
    let hit = json!({"results":[{"index":0,"data":{"files":[{"path":"a.rs","matches":[
            {"line":3,"value":"fn a()","declaration":true}]}]}}]});
    let lines = render(ToolId::LocalSearch, &hit, &json!({}), None).expect("lines");
    assert_eq!(lines.out, "a.rs:3:fn a()  [declaration]");
}

#[test]
fn only_schema_declared_calls_become_commands_and_source_evidence_stays_exact() {
    let source =
        json!({"kind":"continue","tool":"localFetch","input":{"cursor":"source-lookalike"}});
    let structured = json!({"results":[{"index":0,"data":{
        "content":"localSearch {\"queries\":[{\"path\":\".\"}]}",
        "hints":["localSearch {\"queries\":[]}"],
        "source":source,
        "next":{"read":{"kind":"continue","tool":"localFetch","input":{"cursor":"issued"}}},
        "related":[{"kind":"related","tool":"localSearch","input":{"queries":[{"path":".","matchString":["it's"]}]}}]
    }}]});
    let (display, calls) = follow::separate_calls(ToolId::LocalFetch, &structured).expect("schema");
    assert_eq!(calls.len(), 2);
    assert!(calls[0].contains("issued"));
    assert!(calls[1].contains("octocode localSearch"));
    assert!(calls.iter().all(|call| !call.contains("source-lookalike")));
    assert_eq!(display["results"][0]["data"]["source"], source);
    assert_eq!(
        display["results"][0]["data"]["content"],
        structured["results"][0]["data"]["content"]
    );
    assert_eq!(
        display["results"][0]["data"]["hints"],
        structured["results"][0]["data"]["hints"]
    );
    assert!(display["results"][0]["data"].get("next").is_none());
    assert!(display["results"][0]["data"].get("related").is_none());
}

#[test]
fn captured_resource_reads_are_commands_but_their_sources_are_evidence() {
    let source = json!({"tool":"localFetch","query":{"path":"a.rs"}});
    let read = json!({"kind":"continue","tool":"localFetch","input":{"cursor":"captured-source"}});
    let sections = json!({"results":[{"index":0,"data":{"operation":"sections","resources":{"file":{
            "status":"ready","source":source,"read":read}}}}]});
    let (display, calls) = follow::separate_calls(ToolId::Clasify, &sections).expect("schema");
    assert_eq!(calls.len(), 1);
    assert!(calls[0].contains("captured-source"));
    assert_eq!(
        display["results"][0]["data"]["resources"]["file"]["source"],
        source
    );
}

#[test]
fn opaque_search_resumes_print_every_field_when_the_context_mode_is_unknown() {
    let lines = render(
        ToolId::LocalSearch,
        &sample(),
        &json!({"cursor":"issued"}),
        None,
    )
    .expect("text");
    for evidence in ["25\tuse a;", "26\tuse SearcherBuilder;", "997\t  let sb"] {
        assert!(lines.out.contains(evidence), "{}", lines.out);
    }
    for token in ["issued-tool-page", "issued-response-page"] {
        assert_eq!(lines.notes.matches(token).count(), 1, "{}", lines.notes);
    }
}

#[test]
fn batches_and_other_tools_print_text() {
    let two = json!({"results":[{"index":0,"data":{"files":[]}},{"index":1,"data":{"files":[]}}]});
    let lines = render(ToolId::LocalSearch, &two, &json!({}), None).expect("text");
    assert_eq!(lines.out, "## results[0]\n## results[1]");
    let lines = render(ToolId::LocalFetch, &sample(), &json!({}), None).expect("text");
    assert!(
        lines.out.contains("26\tuse SearcherBuilder;"),
        "{}",
        lines.out
    );
    assert!(all_rows_failed(&json!({"results":[{"status":"error"}]})));
    assert!(!all_rows_failed(
        &json!({"results":[{"status":"error"},{"data":{}}]})
    ));
}
