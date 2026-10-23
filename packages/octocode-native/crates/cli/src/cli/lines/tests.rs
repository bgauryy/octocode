use super::*;
use serde_json::json;

fn leaves(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => map.values().for_each(|v| leaves(v, out)),
        Value::Array(items) => items.iter().for_each(|v| leaves(v, out)),
        Value::String(text) => out.extend(
            text.lines()
                .map(|l| l.split_once('\t').map_or(l, |(_, t)| t).to_owned()),
        ),
        other => out.push(other.to_string()),
    }
}

fn sample() -> Value {
    json!({"results":[{"index":0,"rowPart":{"part":1,"of":2},"data":{
            "matchCount":2,"matchedLineCount":2,
            "next":{"nextPage":{"tool":"localSearch","query":{"queries":[{"path":"s","matchString":"it's","page":2}]}}},
            "files":[{"path":"a.rs","matches":[
                {"line":26,"value":"25\tuse a;\n26\tuse SearcherBuilder;\n27\tuse b;"},
                {"line":997,"value":"996\t) {\n997\t  let sb = SearcherBuilder::new();",
                 "enclosing":{"symbolName":"build","kind":"function","line":992,"endLine":1016}}]}]}}],
            "warnings":["incomplete"],
            "responsePagination":{"hasMore":true,"next":{"tool":"localSearch","query":{"responseSnapshot":"s1"}}},
            "root":"/nowhere"})
}

fn context() -> Value {
    json!({"path":"s","matchString":"x","contextLines":1})
}

#[test]
fn hits_print_like_rg_and_lose_no_field() {
    let lines = render(ToolId::LocalSearch, &sample(), &context(), None).expect("lines");
    assert_eq!(
        lines.out,
        "a.rs\n25-use a;\n26:use SearcherBuilder;\n27-use b;\n--\n996-) {\n997:  let sb = SearcherBuilder::new();  [function build 992-1016]"
    );
    assert!(lines.notes.contains("# next.nextPage: octocode localSearch '{\"queries\":[{\"path\":\"s\",\"matchString\":\"it'\\''s\",\"page\":2}]}'"), "{}", lines.notes);
    assert!(
        lines.notes.contains(
            "# responsePagination.next: octocode localSearch '{\"responseSnapshot\":\"s1\"}'"
        ),
        "{}",
        lines.notes
    );
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
    // Shell quoting aside, every leaf value is printed.
    let both = format!("{}\n{}", lines.out, lines.notes).replace(r"'\''", "'");
    let mut all = Vec::new();
    leaves(&sample(), &mut all);
    // A true flag prints as its key alone (`hasMore`), as words do.
    for leaf in all.into_iter().filter(|leaf| leaf != "true") {
        assert!(both.contains(leaf.trim_matches('"')), "missing {leaf}");
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
    // A clipped snippet that is not numbered keeps the rendered text.
    let clipped = json!({"results":[{"index":0,"data":{"files":[{"path":"a","matches":[
            {"line":6,"value":"xxx…","clipped":true}]}]}}]});
    assert!(render(ToolId::LocalSearch, &clipped, &context(), None).is_none());
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
    assert!(
        lines.notes.contains(
            "# hints: includeSkipped: octocode localSearch '{\"queries\":[{\"path\":\".\"}]}'"
        ),
        "{}",
        lines.notes
    );
}

#[test]
fn declarations_and_listings_print_one_line_each() {
    let symbols = json!({"results":[{"index":0,"data":{"filesScanned":2,"files":[
            {"path":"a.rs","symbols":[
                {"symbolName":"render","kind":"function","line":31,"endLine":113,"docStartLine":29},
                {"symbolName":"Visit","kind":"impl","line":34,"endLine":43,"shared":{"kind":"method"},
                 "members":[{"symbolName":"fresh","line":37,"exported":true}]}]}]}}]});
    let lines = render(ToolId::AstSearch, &symbols, &json!({}), None).expect("lines");
    assert_eq!(
        lines.out,
        "a.rs:31: function render  [31-113 docStartLine 29]\na.rs:34: impl Visit  [34-43]\na.rs:37:   method fresh  [exported]"
    );
    assert!(!lines.notes.contains('{'), "{}", lines.notes);
    let file = json!({"results":[{"index":0,"data":{"path":"a.rs","filesScanned":1,"symbols":[
            {"symbolName":"PREFIX","kind":"constant","line":9},
            {"symbolName":"Cursor","kind":"impl","line":94,"endLine":162,"shared":{"kind":"method"},
             "members":[{"symbolName":"encode","line":115,"endLine":124}]}]}}]});
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
    let lines = render(ToolId::StructureSearch, &listed, &files, None).expect("lines");
    assert_eq!(lines.out, "src/a (1).rs\nsrc/b.rs  [symlink]\nsrc/sub/");
    let sized = json!({"operation":"files","path":"src","sort":"size"});
    let lines = render(ToolId::StructureSearch, &listed, &sized, None).expect("lines");
    assert_eq!(
        lines.out,
        "src/a (1).rs  [12 B]\nsrc/b.rs  [34 B, symlink]\nsrc/sub/"
    );
    assert!(
        render(
            ToolId::StructureSearch,
            &listed,
            &json!({"path":"src"}),
            None
        )
        .is_none()
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
    // `groupByFile`: a `path:N` line per site; the count is theirs.
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
    // Content the line format cannot place keeps the rendered text.
    let unavailable = json!({"results":[{"index":0,"data":{"matches":[
            {"path":"w.rs","displayRange":{"startLine":3,"startCharacter":1,"endLine":3},"content":"[content unavailable: binary]"}]}}]});
    assert!(render(ToolId::LspSearch, &unavailable, &json!({}), None).is_none());
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
            {"line":3,"value":"fn a()","declaration":true,"enclosingLine":1}]}]}}]});
    let lines = render(ToolId::LocalSearch, &hit, &json!({}), None).expect("lines");
    assert_eq!(lines.out, "a.rs:3:fn a()  [declaration enclosingLine 1]");
}

#[test]
fn follow_ups_come_from_structure_never_content() {
    let fetched = json!({"results":[{"index":0,"data":{
            "content":"1\t// run: localSearch {\"path\":\"x\"}",
            "next":{"nextPage":{"tool":"localFetch","query":{"path":"a","page":2}}}}}]});
    assert_eq!(
        follow_ups(&fetched),
        vec!["next.nextPage: octocode localFetch '{\"path\":\"a\",\"page\":2}'"]
    );
    let batch = json!({"results":[{"index":0,"data":{}},{"index":1,"data":{"hints":["findFile: structureSearch {\"path\":\".\"}"]}}]});
    assert_eq!(
        follow_ups(&batch),
        vec!["results[1].data.hints: findFile: octocode structureSearch '{\"path\":\".\"}'"]
    );
    let repair = json!({"error":"bad","hints":[{"tool":"localSearch","query":{"path":"."}}]});
    assert_eq!(
        follow_ups(&repair),
        vec!["hints[0]: octocode localSearch '{\"path\":\".\"}'"]
    );
}

/// A resource's `source` echoes the read a result judged; it is data,
/// never a follow-up. Only its `read` prints as a command, so one file
/// has one command, in the canonical `queries` shape.
#[test]
fn an_echoed_source_is_data_not_a_follow_up() {
    let source = json!({"tool":"localFetch","query":{"path":"a.rs","ranges":["1-60"]}});
    let read = json!({"tool":"localFetch","query":{"queries":[{"path":"a.rs","ranges":["1-60"],"snapshot":"s"}]}});
    let sections = json!({"queries":[{"id":"q0","resources":{"file":{
            "status":"ready","source":source,"read":read}}}]});
    assert_eq!(
        follow_ups(&sections),
        vec![
            "queries[0].resources.file.read: octocode localFetch '{\"queries\":[{\"path\":\"a.rs\",\"ranges\":[\"1-60\"],\"snapshot\":\"s\"}]}'"
        ]
    );
    // The line format keeps it as a note too.
    let fields = json!({"source":source}).as_object().cloned().expect("map");
    assert_eq!(
        field_notes("# ", &fields),
        vec![format!("# source: {source}")]
    );
}

#[test]
fn batches_errors_and_other_tools_print_their_json() {
    let two = json!({"results":[{"index":0,"data":{"files":[]}},{"index":1,"data":{"files":[]}}]});
    assert!(render(ToolId::LocalSearch, &two, &json!({}), None).is_none());
    assert!(render(ToolId::LocalFetch, &sample(), &json!({}), None).is_none());
    assert!(all_rows_failed(&json!({"results":[{"status":"error"}]})));
    assert!(!all_rows_failed(
        &json!({"results":[{"status":"error"},{"data":{}}]})
    ));
}
