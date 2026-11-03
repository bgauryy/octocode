//! Counts, listings, declarations and language-server rows as lines.
use super::*;

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
