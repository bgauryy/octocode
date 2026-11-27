//! Counts, listings, declarations and language-server rows as lines.
use super::*;
use octocode_test_support::json_fixture;

#[test]
fn lists_counts_and_empty_results() {
    let files = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {"files": [{"path": "a"}, {"path": "b"}]}
                }
            ]
        }"#,
    );
    assert_eq!(
        render(ToolId::LocalSearch, &files, &json!({}), None)
            .expect("lines")
            .out,
        "a\nb"
    );
    let counts = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {"matchCount": 3, "files": [{"path": "a", "matchCount": 3}]}
                }
            ]
        }"#,
    );
    let lines = render(ToolId::LocalSearch, &counts, &json!({}), None).expect("lines");
    // `path:N` is a line: a count is a named field.
    assert_eq!(lines.out, "a  [matchCount 3]");
    assert_eq!(lines.notes, "# matchCount 3");
    let empty = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "status": "empty",
                    "data": {
                        "matchCount": 0,
                        "hints": [
                            "No matches.",
                            "includeSkipped: localSearch {\"queries\":[{\"path\":\".\"}]}"
                        ]
                    }
                }
            ]
        }"#,
    );
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
    let symbols = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "filesScanned": 2,
                        "files": [
                            {
                                "path": "a.rs",
                                "symbols": [
                                    {
                                        "symbolName": "render",
                                        "kind": "function",
                                        "line": 31,
                                        "endLine": 113,
                                        "docStartLine": 29
                                    },
                                    {
                                        "symbolName": "Visit",
                                        "kind": "impl",
                                        "line": 34,
                                        "endLine": 43,
                                        "members": [
                                            {
                                                "kind": "method",
                                                "symbolName": "fresh",
                                                "line": 37,
                                                "exported": true
                                            }
                                        ]
                                    }
                                ]
                            }
                        ]
                    }
                }
            ]
        }"#,
    );
    let lines = render(ToolId::AstSearch, &symbols, &json!({}), None).expect("lines");
    assert_eq!(
        lines.out,
        "a.rs\n31: function render  [31-113 docStartLine 29]\n34: impl Visit  [34-43]\n37:   method fresh  [exported true]"
    );
    assert!(!lines.notes.contains('{'), "{}", lines.notes);
    let file = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "path": "a.rs",
                        "filesScanned": 1,
                        "symbols": [
                            {"symbolName": "PREFIX", "kind": "constant", "line": 9},
                            {
                                "symbolName": "Cursor",
                                "kind": "impl",
                                "line": 94,
                                "endLine": 162,
                                "members": [
                                    {
                                        "kind": "method",
                                        "symbolName": "encode",
                                        "line": 115,
                                        "endLine": 124
                                    }
                                ]
                            }
                        ]
                    }
                }
            ]
        }"#,
    );
    for tool in [ToolId::AstSearch, ToolId::LspSearch] {
        let lines = render(tool, &file, &json!({}), None).expect("lines");
        assert_eq!(
            lines.out,
            "a.rs\n9: constant PREFIX\n94: impl Cursor  [94-162]\n115:   method encode  [115-124]"
        );
        assert!(!lines.notes.contains("symbols"), "{}", lines.notes);
    }
    let listed = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "path": "src",
                        "files": [
                            {
                                "dir": "src",
                                "files": ["a (1).rs (12)", "b.rs (34, symlink)", "sub/"]
                            }
                        ],
                        "summary": {"fileCount": 2, "dirCount": 1, "bytes": 46}
                    }
                }
            ]
        }"#,
    );
    // Each `dir/` once, its entries under it exactly as the result spells
    // them, whichever tool, operation or page lists them.
    for (tool, asked) in [
        (
            ToolId::StructureSearch,
            json_fixture(r#"{"queries": [{"operation": "files", "path": "src"}]}"#),
        ),
        (
            ToolId::StructureSearch,
            json_fixture(r#"{"cursor": "issued"}"#),
        ),
        (
            ToolId::GhStructure,
            json_fixture(
                r#"{
                    "queries": [{"operation": "tree", "owner": "o", "repo": "r"}]
                }"#,
            ),
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
    let named = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "files": [
                            {"dir": ".", "files": ["report (123)", "dir (x)/"]},
                            {"dir": "dir (x)", "files": ["report (draft).md"]}
                        ]
                    }
                }
            ]
        }"#,
    );
    let paths = json_fixture(
        r#"{
            "queries": [{"operation": "files", "path": ".", "view": "paths"}]
        }"#,
    );
    let lines = render(ToolId::StructureSearch, &named, &paths, None).expect("lines");
    assert_eq!(
        lines.out,
        "./\n  report (123)\n  dir (x)/\ndir (x)/report (draft).md"
    );
}

#[test]
fn lsp_reference_sites_print_as_hit_lines() {
    let sites = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "kind": "references",
                        "files": [
                            {
                                "path": "a.rs",
                                "matches": [{"line": 7, "column": 3, "value": "run();"}]
                            }
                        ]
                    }
                }
            ]
        }"#,
    );
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
    let calls = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "kind": "callers",
                        "files": [
                            {
                                "path": "m.rs",
                                "matches": [
                                    {
                                        "symbolName": "walk",
                                        "kind": "function",
                                        "line": 79,
                                        "endLine": 89,
                                        "sites": [{"line": 83, "column": 36}, {"line": 85, "column": 4}]
                                    }
                                ]
                            }
                        ]
                    }
                }
            ]
        }"#,
    );
    let lines = render(ToolId::LspSearch, &calls, &json!({}), None).expect("lines");
    assert_eq!(
        lines.out,
        "m.rs:79: function walk  [79-89 sites 83:36,85:4]"
    );
    // A callee declared in another file prints under its own file, with the
    // sites under the row's, so every location is the real one.
    let callees = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "kind": "callees",
                        "files": [
                            {
                                "path": "src/calls.ts",
                                "matches": [
                                    {
                                        "symbolName": "target",
                                        "kind": "function",
                                        "line": 9,
                                        "endLine": 11,
                                        "path": "src/lib.ts",
                                        "sites": [
                                            {"line": 19, "column": 14},
                                            {"line": 21, "column": 12}
                                        ]
                                    }
                                ]
                            }
                        ]
                    }
                }
            ]
        }"#,
    );
    let lines = render(ToolId::LspSearch, &callees, &json!({}), None).expect("lines");
    assert_eq!(
        lines.out,
        "src/lib.ts:9: function target  [9-11 sites src/calls.ts:19:14,src/calls.ts:21:12]"
    );
    assert!(!lines.notes.contains("path src/lib.ts"), "{}", lines.notes);
    // `view:"files"`: the path once, then every site's line; a count that
    // only repeats their number is not restated, any other is named.
    let grouped = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "matchCount": 3,
                        "files": [
                            {"path": "a.rs", "matchCount": 2, "matchedLines": [14, 275]},
                            {"path": "b.rs", "matchCount": 1, "matchedLines": [3]},
                            {"path": "c.rs", "matchCount": 4, "matchedLines": [8]}
                        ]
                    }
                }
            ]
        }"#,
    );
    let lines = render(ToolId::LspSearch, &grouped, &json!({}), None).expect("lines");
    assert_eq!(lines.out, "a.rs:14,275\nb.rs:3\nc.rs:8  [matchCount 4]");
    assert_eq!(lines.notes, "# matchCount 3");
    // A declaration body prints as an `rg --heading` block; its
    // omission marker is a note.
    let definition = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "kind": "definition",
                        "matches": [
                            {
                                "path": "w.rs",
                                "line": 73,
                                "column": 8,
                                "value": "72\t/// doc\n73\tpub fn f(\n74\t) {}\n… 40 more lines omitted (source lines 133-172)."
                            },
                            {
                                "path": "w.rs",
                                "line": 90,
                                "column": 4,
                                "value": "90\tf();\n"
                            }
                        ]
                    }
                }
            ]
        }"#,
    );
    let lines = render(ToolId::LspSearch, &definition, &json!({}), None).expect("lines");
    assert_eq!(
        lines.out,
        "w.rs\n72-/// doc\n73:pub fn f(  [column 8]\n74-) {}\n--\n90:f();  [column 4]\n# file w.rs:73: … 40 more lines omitted (source lines 133-172)."
    );
    // One-line locations print as hits; symbols as declarations.
    let single = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "matches": [
                            {
                                "path": "w.rs",
                                "line": 23,
                                "column": 10,
                                "value": "23\t#[derive(Clone)]",
                                "isDefinition": true
                            }
                        ]
                    }
                }
            ]
        }"#,
    );
    let lines = render(ToolId::LspSearch, &single, &json!({}), None).expect("lines");
    assert_eq!(
        lines.out,
        "w.rs:23:#[derive(Clone)]  [column 10 isDefinition true]"
    );
    let symbols = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "kind": "symbols",
                        "matches": [
                            {
                                "symbolName": "walk",
                                "kind": "function",
                                "path": "m.rs",
                                "line": 79,
                                "column": 15
                            }
                        ]
                    }
                }
            ]
        }"#,
    );
    let lines = render(ToolId::LspSearch, &symbols, &json!({}), None).expect("lines");
    assert_eq!(lines.out, "m.rs:79: function walk  [column 15]");
    // Content the line format cannot place prints as `key: value` text.
    let unavailable = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "matches": [
                            {
                                "path": "w.rs",
                                "line": 3,
                                "column": 1,
                                "value": "[content unavailable: binary]"
                            }
                        ]
                    }
                }
            ]
        }"#,
    );
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
    let out = render_out(json_fixture(
        r#"{
            "issues": [
                {
                    "number": 11,
                    "labels": ["bug", "p3-minor"],
                    "state": "closed"
                },
                {"number": 12, "labels": [], "state": "closed"},
                {"number": 13, "labels": ["docs"], "state": "closed"}
            ]
        }"#,
    ));
    assert!(
        out.contains(
            "issues[]: number labels  [state closed]\n  11 bug,p3-minor\n  12 []\n  13 docs"
        ),
        "{out}"
    );
    let out = render_out(json_fixture(
        r#"{
            "issues": [
                {
                    "number": 11,
                    "labels": ["p3-minor-bug", "feat: ssr"],
                    "state": "closed"
                },
                {"number": 12, "labels": ["docs"], "state": "closed"}
            ]
        }"#,
    ));
    assert!(
        out.contains(
            "issues[]: number labels  [state closed]\n  11 p3-minor-bug, feat: ssr\n  12 docs"
        ),
        "{out}"
    );
    let nested = json_fixture(
        r#"{
            "issues": [
                {"number": 11, "labels": [{"name": "bug"}]},
                {"number": 12, "labels": [{"name": "docs"}]}
            ]
        }"#,
    );
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
        json_fixture(
            r#"{
                "answer": "a",
                "sources": [
                    {"source": "osv", "status": "found"},
                    {
                        "source": "github",
                        "status": "found",
                        "observedAt": "2026-11-15T10:00:00Z"
                    }
                ]
            }"#,
        ),
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
        json_fixture(
            r#"{
                "issues": [
                    {"number": 11, "author": "amy", "labels": ["bug"]},
                    {"number": 12, "labels": ["docs"]}
                ]
            }"#,
        ),
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
    let sorted = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "files": [
                            {
                                "dir": "pkg/crates/cli/src/cli/lines/tests",
                                "files": ["done_facts.rs (totalLines=612)"]
                            },
                            {
                                "dir": "pkg/crates/cli/src/cli",
                                "files": ["lines.rs (totalLines=530)", "tests.rs (totalLines=511)"]
                            },
                            {
                                "dir": "pkg/crates/cli/src/cli/lines/tests",
                                "files": ["stated_once.rs (totalLines=505)"]
                            }
                        ]
                    }
                }
            ]
        }"#,
    );
    let asked = json_fixture(
        r#"{
            "queries": [
                {
                    "operation": "files",
                    "path": "pkg",
                    "sort": {"by": "totalLines", "order": "desc"}
                }
            ]
        }"#,
    );
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
    let built = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "walk": {
                            "defaultExcludes": true,
                            "includeIgnored": true,
                            "includeHidden": false
                        }
                    }
                }
            ]
        }"#,
    );
    let lines = render(ToolId::GraphBuild, &built, &json!({}), None).expect("lines");
    let said = format!("{}\n{}", lines.out, lines.notes);
    assert!(
        said.contains("walk: defaultExcludes true includeIgnored true includeHidden false"),
        "{said}"
    );
}

/// The `(dir, entry)` pairs a listing's lines state, read as a person reads
/// them: an unindented line ending in `/` heads the entries indented under
/// it, any other unindented line is one entry after its last `/`, and each
/// directory joins the `# root:` line when there is one. Headings are
/// returned too, in print order.
fn listed_pairs(out: &str) -> (Vec<(String, String)>, Vec<String>) {
    let root = out
        .lines()
        .find_map(|line| line.strip_prefix("# root: "))
        .map(str::to_owned);
    let joined = |dir: &str| match (&root, dir) {
        (Some(root), ".") => root.clone(),
        (Some(root), dir) => format!("{root}/{dir}"),
        (None, dir) => dir.to_owned(),
    };
    let (mut pairs, mut headings) = (Vec::new(), Vec::new());
    for line in out.lines().filter(|line| !line.starts_with("# ")) {
        if let Some(entry) = line.strip_prefix("  ") {
            let dir = headings.last().cloned().expect("an entry under a heading");
            pairs.push((dir, entry.to_owned()));
        } else if let Some(dir) = line.strip_suffix('/') {
            headings.push(joined(dir));
        } else {
            let (dir, entry) = line.rsplit_once('/').expect("dir/entry");
            pairs.push((joined(dir), entry.to_owned()));
        }
    }
    (pairs, headings)
}

/// A directory whose one entry is a directory keeps its own heading, as a
/// directory with several entries does: `dir/sub/` on one line reads as the
/// heading of `sub`, which a deeper listing prints next, so the parent's
/// heading would be lost and `sub`'s printed twice. Every `(dir, entry)` of
/// the result reads back from the lines, each heading once, whether or not
/// the listing goes below that directory, with or without entry fields.
#[test]
fn a_directory_holding_one_directory_keeps_its_heading() {
    let tree =
        |groups: Value| json!({"results": [{"index": 0, "data": {"path": "fx", "files": groups}}]});
    for (groups, asked) in [
        (
            json!([
                {"dir": "fx", "files": ["README.md", "docs/", "src/"]},
                {"dir": "fx/docs", "files": ["guide.md"]},
                {"dir": "fx/src", "files": ["deep/", "index.js", "lib/"]},
                {"dir": "fx/src/deep", "files": ["er/"]},
                {"dir": "fx/src/deep/er", "files": ["a.min.js", "b.min.js"]},
                {"dir": "fx/src/lib", "files": ["util.js"]}
            ]),
            json!({"queries": [{"operation": "tree", "path": "fx", "maxDepth": 5}]}),
        ),
        (
            json!([
                {"dir": "fx", "files": ["src/"]},
                {"dir": "fx/src", "files": ["deep/ (modified=2026-11-18T07:00:00Z)"]}
            ]),
            json!({"queries": [{"operation": "tree", "path": "fx", "maxDepth": 2, "view": "details"}]}),
        ),
    ] {
        let lines =
            render(ToolId::StructureSearch, &tree(groups.clone()), &asked, None).expect("lines");
        let (pairs, headings) = listed_pairs(&lines.out);
        let expected: Vec<(String, String)> = groups
            .as_array()
            .expect("groups")
            .iter()
            .flat_map(|group| {
                let dir = group["dir"].as_str().expect("dir");
                group["files"]
                    .as_array()
                    .expect("entries")
                    .iter()
                    .map(move |entry| (dir.to_owned(), entry.as_str().expect("entry").to_owned()))
            })
            .collect();
        assert_eq!(pairs, expected, "{}", lines.out);
        let mut unique = headings.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), headings.len(), "{}", lines.out);
    }
}

/// TC7: ghStructure refs as `"name sha"` strings print one ref per line,
/// each commit beside its name, as the objects they stand for did.
#[test]
fn refs_as_name_and_commit_strings_print_one_per_line() {
    let refs = [
        "v14.1.1 4649aa9f0b2c7d1e",
        "v14.1.0 0a1b2c3d4e5f6a7b",
        "main 9f8e7d6c5b4a3f2e",
    ];
    let output = json!({"results": [{"index": 0, "data": {
        "tags": [refs[0], refs[1]], "branches": [refs[2]]}}]});
    let query = json!({"queries": [{"operation": "refs", "owner": "o", "repo": "r"}]});
    let lines = render(ToolId::GhStructure, &output, &query, None).expect("lines");
    for name in refs {
        assert!(
            lines
                .out
                .lines()
                .any(|line| line.trim_start().trim_start_matches("- ").ends_with(name)),
            "{name}: {}",
            lines.out
        );
    }
    eprintln!("{}", lines.out);
}

/// Q5: an outline of line-first strings prints each string as the row it
/// is, `N: text` under the file's path: text adds, renames and drops nothing
/// the string states, so it is never larger than the JSON.
#[test]
fn an_outline_of_line_first_strings_prints_each_row_as_stated() {
    let symbols = json!([
        {"symbolName": "Pager", "kind": "struct", "line": 70, "endLine": 120, "members": [
            {"symbolName": "new", "kind": "method", "line": 74, "endLine": 80}]},
        {"symbolName": "drop", "kind": "function", "line": 130, "docStartLine": 128},
    ]);
    let lines = octocode_runtime_base::response::symbol_outline::outline_lines(
        symbols.as_array().expect("symbols"),
    );
    let output = json!({"results": [{"index": 0, "data": {"files": [{"path": "src/pager.rs", "symbols": lines}]}}]});
    let query = json!({"queries": [{"path": "src", "operation": "symbols"}]});
    let printed = render(ToolId::AstSearch, &output, &query, None).expect("lines");
    let mut rows = printed.out.lines();
    assert_eq!(rows.next(), Some("src/pager.rs"), "{}", printed.out);
    for line in &lines {
        let (number, text) = line.split_once('\t').expect("a line-first row");
        assert_eq!(
            rows.next(),
            Some(format!("{number}: {text}").as_str()),
            "{}",
            printed.out
        );
    }
    assert_eq!(rows.next(), None, "{}", printed.out);
    assert!(
        printed.out.len() <= output.to_string().len(),
        "{}",
        printed.out
    );
}

/// `files` as one row of one tool's listing, with any other `data` fields.
fn listing_of(files: Value, mut data: Value) -> Value {
    data["files"] = files;
    json!({"results": [{"index": 0, "data": data}]})
}

/// The lines of a rendered listing's entries (not `# …` notes), and the
/// root they are relative to, when one is stated.
fn entry_lines(lines: &Lines) -> (Option<&str>, Vec<&str>) {
    let root = lines
        .out
        .lines()
        .chain(lines.notes.lines())
        .find_map(|line| line.strip_prefix("# root: "));
    let entries = lines
        .out
        .lines()
        .filter(|line| !line.starts_with('#'))
        .collect();
    (root, entries)
}

/// A flat listing row (a string naming its entry by path) prints as exactly
/// one line, a file and a directory alike, in the page's order: the row as
/// the result spells it, from the root the lines state (`listed` when they
/// state none), so the printed root, `/` and the line are the entry's path
/// from `listed`'s parent, and no line carries a prefix (`./`) the row does
/// not have. Both text forms print the same entries.
fn assert_one_line_each(tool: ToolId, query: &Value, data: Value, rows: &[&str], listed: &str) {
    let structured = listing_of(json!(rows), data);
    let terminal = render(tool, &structured, query, None).expect("terminal");
    let mcp = render_as(tool, &structured, query, None, Form::MCP).expect("mcp");
    let join = |dir: &str, entry: &str| match dir {
        "" | "." => entry.to_owned(),
        dir => format!("{dir}/{entry}"),
    };
    for text in [&terminal, &mcp] {
        let (root, entries) = entry_lines(text);
        assert_eq!(entries.len(), rows.len(), "one line a row: {}", text.out);
        for (line, row) in entries.iter().zip(rows) {
            assert_eq!(
                join(root.unwrap_or_default(), line),
                join(listed, row),
                "{}",
                text.out
            );
            assert!(!line.starts_with("./"), "no prefix of its own: {line}");
        }
        // A root is stated only where it shortens the lines, and lies at or
        // under the listed path.
        if let Some(root) = root {
            assert!(rows.len() > 1, "{}", text.out);
            assert!(join(root, "").starts_with(&join(listed, "")), "{root}");
        }
    }
}

/// A ranked `structureSearch` page lists flat rows that are row paths (the
/// form every local row takes, whatever path the request listed): files of
/// the listed directory itself, nested files and directories (which rank
/// last) each print as one line under the one root they share, and the
/// root, `/` and the line are the path the result states.
#[test]
fn a_ranked_local_listing_prints_one_line_per_row_under_one_root() {
    let under = |dir: &str, rows: &[&str]| {
        rows.iter()
            .map(|row| format!("{dir}/{row}"))
            .collect::<Vec<_>>()
    };
    let asked = |path: &str| json!({"queries": [{"operation": "files", "path": path, "sort": {"by": "bytes"}}]});
    let rows = under(
        "pkg/src/cli",
        &[
            "lines/tests/big.rs (900, totalLines=30)",
            "mid.rs (500)",
            "lines/small.rs (10)",
            "lines/",
            "lines/tests/",
        ],
    );
    let rows = rows.iter().map(String::as_str).collect::<Vec<_>>();
    // The same lines whether the request spelled the path as the rows do, or
    // absolutely, or the page is a cursor's: the rows state their own paths.
    for query in [
        asked("pkg/src/cli"),
        asked("/abs/workspace/pkg/src/cli"),
        json!({"cursor": "cursor2.listing-page"}),
    ] {
        assert_one_line_each(ToolId::StructureSearch, &query, json!({}), &rows, ".");
        let lines = render(
            ToolId::StructureSearch,
            &listing_of(json!(rows), json!({})),
            &query,
            None,
        )
        .expect("lines");
        let (root, entries) = entry_lines(&lines);
        let root = root.unwrap_or_else(|| panic!("a shared root: {}", lines.out));
        let stated = entries
            .iter()
            .map(|line| format!("{root}/{line}"))
            .collect::<Vec<_>>();
        assert_eq!(stated, rows, "root/line is the row");
    }
    // A listed directory with no subdirectories: its files, from it.
    let files = under(
        "pkg/src/cli/server",
        &["handler.rs (9397)", "warm.rs (3708)", "reply.rs (2268)"],
    );
    assert_one_line_each(
        ToolId::StructureSearch,
        &asked("pkg/src/cli/server/"),
        json!({}),
        &files.iter().map(String::as_str).collect::<Vec<_>>(),
        ".",
    );
    // One row: its whole path on its line.
    assert_one_line_each(
        ToolId::StructureSearch,
        &asked("pkg/src"),
        json!({}),
        &["pkg/src/only.rs (1)"],
        ".",
    );
    // The workspace root listed: rows that share no directory state no root.
    assert_one_line_each(
        ToolId::StructureSearch,
        &asked("."),
        json!({}),
        &[
            "src/a/big.rs (900)",
            "README.md (40)",
            "docs/",
            "top (draft).md",
        ],
        ".",
    );
    // Rows outside the workspace are named from the root the row states.
    let outside = ["src/a.rs (9)", "src/deep/b.rs (8)"];
    let lines = render(
        ToolId::StructureSearch,
        &listing_of(json!(outside), json!({"root": "/clones/o/r"})),
        &asked("/clones/o/r"),
        None,
    )
    .expect("lines");
    let (root, entries) = entry_lines(&lines);
    let root = root.expect("a stated root prints");
    assert!(root.starts_with("/clones/o/r"), "{}", lines.out);
    for (line, row) in entries.iter().zip(outside) {
        assert_eq!(format!("{root}/{line}"), format!("/clones/o/r/{row}"));
    }
}

/// A `ghStructure` page mixes groups with flat rows (a directory listing
/// one entry is that entry's repository-relative path): each flat file row
/// is its own line beside the groups, a symlink's target path staying in its
/// fields; a page of flat rows only prints one line a row, from the
/// directory they share.
#[test]
fn a_repository_listing_prints_each_flat_row_as_one_line() {
    let query = json!({"queries": [{"operation": "tree", "owner": "o", "repo": "r"}]});
    let flat = [
        "docs/guide.md (812)",
        "src/deep/",
        "src/deep/er/current (link -> ../../lib/v2)",
        "top (draft).md",
    ];
    let mixed = json!([
        {"dir": ".", "files": ["LICENSE (1071)", "src/"]},
        flat[0],
        flat[1],
        {"dir": "src/deep", "files": ["a.rs (10)", "b.rs (20)"]},
        flat[2],
        flat[3]
    ]);
    let lines = render(
        ToolId::GhStructure,
        &listing_of(mixed, json!({"ref": "main"})),
        &query,
        None,
    )
    .expect("lines");
    let (root, entries) = entry_lines(&lines);
    assert_eq!(root, None, "{}", lines.out);
    // Beside groups a `dir/` line is a heading, so the flat directory row
    // keeps the form its group of one has; every other flat row is its line.
    assert_eq!(
        entries,
        [
            "./",
            "  LICENSE (1071)",
            "  src/",
            flat[0],
            "src/",
            "  deep/",
            "src/deep/",
            "  a.rs (10)",
            "  b.rs (20)",
            flat[2],
            flat[3],
        ],
        "{}",
        lines.out
    );
    assert_one_line_each(ToolId::GhStructure, &query, json!({}), &flat, ".");
    assert_one_line_each(
        ToolId::GhStructure,
        &query,
        json!({}),
        &["src/deep/a.rs (1)", "src/deep/er/", "src/deep/er/b.rs (2)"],
        ".",
    );
}

/// A flat local row is its own path: with no request beside it (a bare
/// cursor) and no directory shared, every row prints as stated.
#[test]
fn a_flat_row_with_no_listed_path_prints_as_stated() {
    let rows = ["lines/big.rs (900)", "mid.rs (500)"];
    let lines = render(
        ToolId::StructureSearch,
        &listing_of(json!(rows), json!({})),
        &json!({"queries": [{"cursor": "cursor2.listing-page"}]}),
        None,
    )
    .expect("lines");
    for row in rows {
        assert!(
            lines
                .out
                .lines()
                .any(|line| line.trim_start().trim_start_matches("- ") == row),
            "{row}: {}",
            lines.out
        );
    }
}
