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
    // `path:N` is a line: a count is a named field.
    assert_eq!(lines.out, "a  [matchCount 3]");
    assert_eq!(lines.notes, "# matchCount 3");
    let empty = json!({"results":[{"index":0,"status":"empty","data":{"matchCount":0,
            "hints":["No matches.","includeSkipped: localSearch {\"queries\":[{\"path\":\".\"}]}"]}}]});
    let lines = render(ToolId::LocalSearch, &empty, &json!({}), None).expect("lines");
    // Hints follow the status on stdout, as they do for every row shape.
    assert_eq!(
        lines.out,
        "# status: empty\n# hints: No matches.\n# hints: includeSkipped: localSearch {\"queries\":[{\"path\":\".\"}]}"
    );
    assert_eq!(lines.notes, "# matchCount 0");
    assert!(
        !lines.out.contains("octocode localSearch"),
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
        "a.rs\n31: function render  [31-113 docStartLine 29]\n34: impl Visit  [34-43]\n37:   method fresh  [exported true]"
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
            "a.rs\n9: constant PREFIX\n94: impl Cursor  [94-162]\n115:   method encode  [115-124]"
        );
        assert!(!lines.notes.contains("symbols"), "{}", lines.notes);
    }
    let listed = json!({"results":[{"index":0,"data":{"path":"src","files":[
            {"dir":"src","files":["a (1).rs (12)","b.rs (34, symlink)","sub/"]}],
            "summary":{"fileCount":2,"dirCount":1,"bytes":46}}}]});
    // Each `dir/` once, its entries under it exactly as the result spells
    // them, whichever tool, operation or page lists them.
    for (tool, asked) in [
        (
            ToolId::StructureSearch,
            json!({"queries":[{"operation":"files","path":"src"}]}),
        ),
        (ToolId::StructureSearch, json!({"cursor":"issued"})),
        (
            ToolId::GhStructure,
            json!({"queries":[{"operation":"tree","owner":"o","repo":"r"}]}),
        ),
    ] {
        let lines = render(tool, &listed, &asked, None).expect("lines");
        assert_eq!(
            lines.out, "src/\n  a (1).rs (12)\n  b.rs (34, symlink)\n  sub/",
            "{tool:?} {asked}"
        );
        assert!(
            lines
                .notes
                .contains("# summary: fileCount 2 dirCount 1 bytes 46"),
            "{}",
            lines.notes
        );
    }
    let named = json!({"results":[{"index":0,"data":{"files":[
            {"dir":".","files":["report (123)","dir (x)/"]},
            {"dir":"dir (x)","files":["report (draft).md"]}]}}]});
    let paths = json!({"queries":[{"operation":"files","path":".","view":"paths"}]});
    let lines = render(ToolId::StructureSearch, &named, &paths, None).expect("lines");
    assert_eq!(
        lines.out,
        "./\n  report (123)\n  dir (x)/\ndir (x)/report (draft).md"
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
    // `view:"files"`: the path once, then every site's line; a count that
    // only repeats their number is not restated, any other is named.
    let grouped = json!({"results":[{"index":0,"data":{"matchCount":3,"files":[
            {"path":"a.rs","matchCount":2,"matchedLines":[14,275]},{"path":"b.rs","matchCount":1,"matchedLines":[3]},
            {"path":"c.rs","matchCount":4,"matchedLines":[8]}]}}]});
    let lines = render(ToolId::LspSearch, &grouped, &json!({}), None).expect("lines");
    assert_eq!(lines.out, "a.rs:14,275\nb.rs:3\nc.rs:8  [matchCount 4]");
    assert_eq!(lines.notes, "# matchCount 3");
    // A declaration body prints as an `rg --heading` block; its
    // omission marker is a note.
    let definition = json!({"results":[{"index":0,"data":{"kind":"definition","matches":[
            {"path":"w.rs","line":73,"column":8,
             "content":"72\t/// doc\n73\tpub fn f(\n74\t) {}\n… 40 more lines omitted (source lines 133-172)."},
            {"path":"w.rs","line":90,"column":4,"content":"90\tf();\n"}]}}]});
    let lines = render(ToolId::LspSearch, &definition, &json!({}), None).expect("lines");
    assert_eq!(
        lines.out,
        "w.rs\n72-/// doc\n73:pub fn f(  [column 8]\n74-) {}\n--\n90:f();  [column 4]\n# file w.rs:73: … 40 more lines omitted (source lines 133-172)."
    );
    // One-line locations print as hits; symbols as declarations.
    let single = json!({"results":[{"index":0,"data":{"matches":[
            {"path":"w.rs","line":23,"column":10,"content":"23\t#[derive(Clone)]","isDefinition":true}]}}]});
    let lines = render(ToolId::LspSearch, &single, &json!({}), None).expect("lines");
    assert_eq!(
        lines.out,
        "w.rs:23:#[derive(Clone)]  [column 10 isDefinition true]"
    );
    let symbols = json!({"results":[{"index":0,"data":{"kind":"symbols","matches":[
            {"symbolName":"walk","kind":"function","path":"m.rs","line":79,"column":15}]}}]});
    let lines = render(ToolId::LspSearch, &symbols, &json!({}), None).expect("lines");
    assert_eq!(lines.out, "m.rs:79: function walk  [column 15]");
    // Content the line format cannot place prints as `key: value` text.
    let unavailable = json!({"results":[{"index":0,"data":{"matches":[
            {"path":"w.rs","line":3,"column":1,"content":"[content unavailable: binary]"}]}}]});
    let lines = render(ToolId::LspSearch, &unavailable, &json!({}), None).expect("text");
    assert!(
        lines.out.contains("[content unavailable: binary]") && !lines.out.starts_with("w.rs:3:"),
        "{}",
        lines.out
    );
}

/// A row's flat list (`labels`, `topics`) is one table cell, so each row
/// stays one line: words joined by `,`, and a list with free text (a label
/// `feat: ssr`) by `, ` as the row's one free-text cell, last. A list of
/// objects keeps the block layout, which shows every field.
#[test]
fn flat_lists_print_inline_in_a_table_row() {
    let render_out = |data: Value| {
        render(
            ToolId::GhSearchHistory,
            &json!({"results":[{"index":0,"data":data}]}),
            &json!({}),
            None,
        )
        .expect("lines")
        .out
    };
    let out = render_out(json!({"issues":[
            {"number":11,"labels":["bug","p3-minor"],"state":"closed"},
            {"number":12,"labels":[],"state":"closed"},
            {"number":13,"labels":["docs"],"state":"closed"}]}));
    assert!(
        out.contains(
            "issues[]: number labels  [state closed]\n  11 bug,p3-minor\n  12 []\n  13 docs"
        ),
        "{out}"
    );
    let out = render_out(json!({"issues":[
            {"number":11,"labels":["p3-minor-bug","feat: ssr"],"state":"closed"},
            {"number":12,"labels":["docs"],"state":"closed"}]}));
    assert!(
        out.contains(
            "issues[]: number labels  [state closed]\n  11 p3-minor-bug, feat: ssr\n  12 docs"
        ),
        "{out}"
    );
    let nested = json!({"issues":[
            {"number":11,"labels":[{"name":"bug"}]},{"number":12,"labels":[{"name":"docs"}]}]});
    assert!(!render_out(nested).contains("issues[]:"));
}

/// A row ends at its last field: a column only some rows carry (the github
/// source's `observedAt`) leaves a short row's tail out, never a trailing
/// `-`. A `-` still marks an absent field that a later cell follows.
#[test]
fn a_row_without_its_trailing_fields_ends_early() {
    let render_out = |tool: ToolId, data: Value| {
        render(
            tool,
            &json!({"results":[{"index":0,"data":data}]}),
            &json!({}),
            None,
        )
        .expect("lines")
        .out
    };
    let out = render_out(
        ToolId::SecurityCheck,
        json!({"answer":"a","sources":[
            {"source":"osv","status":"found"},
            {"source":"github","status":"found","observedAt":"2026-11-15T10:00:00Z"}]}),
    );
    assert!(
        out.contains(
            "sources[]: source observedAt  [status found]\n  osv\n  github 2026-11-15T10:00:00Z"
        ),
        "{out}"
    );
    assert!(!out.contains("osv -"), "{out}");
    // A field a later cell follows keeps its mark.
    let out = render_out(
        ToolId::GhSearchHistory,
        json!({"issues":[
            {"number":11,"author":"amy","labels":["bug"]},
            {"number":12,"labels":["docs"]}]}),
    );
    assert!(
        out.contains("issues[]: number author labels\n  11 amy bug\n  12 - docs"),
        "{out}"
    );
}

/// A sorted listing whose groups share a directory states it once, as hits
/// do (`# root:`), and each group's `dir/` reads relative to it: joined to
/// the root, every entry is the path the result names.
#[test]
fn listing_groups_sharing_a_directory_print_relative_to_it() {
    let sorted = json!({"results":[{"index":0,"data":{"files":[
            {"dir":"pkg/crates/cli/src/cli/lines/tests","files":["done_facts.rs (totalLines=612)"]},
            {"dir":"pkg/crates/cli/src/cli","files":["lines.rs (totalLines=530)","tests.rs (totalLines=511)"]},
            {"dir":"pkg/crates/cli/src/cli/lines/tests","files":["stated_once.rs (totalLines=505)"]}]}}]});
    let asked = json!({"queries":[{"operation":"files","path":"pkg","sort":{"by":"totalLines","order":"desc"}}]});
    let lines = render(ToolId::StructureSearch, &sorted, &asked, None).expect("lines");
    assert_eq!(
        lines.out,
        "# root: pkg/crates/cli/src/cli\nlines/tests/done_facts.rs (totalLines=612)\n./\n  lines.rs (totalLines=530)\n  tests.rs (totalLines=511)\nlines/tests/stated_once.rs (totalLines=505)"
    );
}

/// A boolean prints its value like every other field, so a run of fields
/// reads one way: `key true`, `key false`, never a bare key beside a pair.
#[test]
fn booleans_print_their_values() {
    let built = json!({"results":[{"index":0,"data":{"walk":{
            "defaultExcludes":true,"includeIgnored":true,"includeHidden":false}}}]});
    let lines = render(ToolId::GraphBuild, &built, &json!({}), None).expect("lines");
    let said = format!("{}\n{}", lines.out, lines.notes);
    assert!(
        said.contains("walk: defaultExcludes true includeIgnored true includeHidden false"),
        "{said}"
    );
}
