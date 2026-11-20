use super::*;

const PY: &str = "import os\n\ndef first(a):\n    x = 1\n    return a\n\n\ndef second(b):\n    if b:\n        return 1\n    return 2\n";

mod recovery;

/// The lines a block widens up over are comments in the file's own
/// grammar, attributes and decorators: a Python `#` comment leads its
/// def, a Rust line that merely starts with `--` does not lead its fn.
#[test]
fn widening_follows_the_files_comment_syntax() {
    let py = "x = 1\n# Adds.\n@cache\ndef add(a, b):\n    s = a + b\n    return s\n";
    let widened = widen_ranges(
        py,
        "m.py",
        vec![LineRange { start: 5, end: 5 }],
        &mut vec![],
        &mut NextCalls::default(),
    );
    assert_eq!(widened, vec![LineRange { start: 2, end: 6 }]);
    let rs = "const S: &str = \"a\n-- b\";\n#[inline]\nfn f() -> u8 {\n    1\n}\n";
    let widened = widen_ranges(
        rs,
        "lib.rs",
        vec![LineRange { start: 5, end: 5 }],
        &mut vec![],
        &mut NextCalls::default(),
    );
    assert_eq!(widened, vec![LineRange { start: 3, end: 6 }]);
}

#[test]
fn ranges_widen_to_the_enclosing_declaration() {
    let mut warnings = vec![];
    let widened = widen_ranges(
        PY,
        "m.py",
        vec![
            LineRange { start: 9, end: 9 },
            LineRange { start: 4, end: 4 },
        ],
        &mut warnings,
        &mut NextCalls::default(),
    );
    assert_eq!(
        widened,
        vec![
            LineRange { start: 8, end: 11 },
            LineRange { start: 3, end: 5 }
        ]
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    // Outside every declaration the range stays, with a note.
    let kept = widen_ranges(
        PY,
        "m.py",
        vec![LineRange { start: 1, end: 1 }],
        &mut warnings,
        &mut NextCalls::default(),
    );
    assert_eq!(kept, vec![LineRange { start: 1, end: 1 }]);
    assert!(warnings[0].contains("line 1"), "{warnings:?}");
}

#[test]
fn unsupported_files_keep_the_window_with_a_note() {
    let mut warnings = vec![];
    let kept = widen_ranges(
        "a\nb\n",
        "notes.unknownext",
        vec![LineRange { start: 1, end: 1 }],
        &mut warnings,
        &mut NextCalls::default(),
    );
    assert_eq!(kept, vec![LineRange { start: 1, end: 1 }]);
    assert!(
        warnings[0].starts_with("block: no declaration outline"),
        "{warnings:?}"
    );
}

/// No fixed cap on a declaration's length: a block read selects the whole
/// declaration and the read's pages bound what one response holds.
#[test]
fn a_long_declaration_is_selected_whole() {
    let body: String = (0..450).map(|i| format!("    x{i} = {i}\n")).collect();
    let source = format!("def big():\n{body}    return 0\n");
    let whole = vec![LineRange { start: 1, end: 452 }];
    let mut warnings = vec![];
    let widened = widen_ranges(
        &source,
        "m.py",
        vec![LineRange { start: 10, end: 12 }],
        &mut warnings,
        &mut NextCalls::default(),
    );
    assert_eq!(widened, whole);
    let mut blocks = vec![];
    let windows = widen_matches(
        &source,
        "m.py",
        &[20],
        vec![LineRange { start: 18, end: 22 }],
        &mut warnings,
        &mut blocks,
        &mut NextCalls::default(),
    );
    assert_eq!(windows, whole);
    assert_eq!(blocks.len(), 1, "{blocks:?}");
    assert!(warnings.is_empty(), "{warnings:?}");
}

/// A declaration's leading doc comments and attributes belong to it:
/// a block read starts at the first of them, not at the `fn` line.
#[test]
fn blocks_include_leading_doc_comments_and_attributes() {
    let source =
        "use std::fmt;\n\n/// Doc one.\n/// Doc two.\n#[inline]\nfn target() {\n    body();\n}\n";
    let mut warnings = vec![];
    let widened = widen_ranges(
        source,
        "m.rs",
        vec![LineRange { start: 7, end: 7 }],
        &mut warnings,
        &mut NextCalls::default(),
    );
    assert_eq!(
        widened,
        vec![LineRange { start: 3, end: 8 }],
        "{warnings:?}"
    );
    // A match on the declaration's own head reads it as written; with
    // a head hit present, a match inside a body keeps its window.
    let windows = widen_matches(
        source,
        "m.rs",
        &[6, 7],
        vec![
            LineRange { start: 6, end: 6 },
            LineRange { start: 7, end: 7 },
        ],
        &mut warnings,
        &mut vec![],
        &mut NextCalls::default(),
    );
    assert_eq!(
        windows,
        vec![
            LineRange { start: 3, end: 8 },
            LineRange { start: 7, end: 7 }
        ]
    );
    // A blank line ends the doc run; a preprocessor line is not a doc.
    let c = "#define LIMIT 4\nint f(int a) {\n    return a;\n}\n";
    let widened = widen_ranges(
        c,
        "m.c",
        vec![LineRange { start: 3, end: 3 }],
        &mut warnings,
        &mut NextCalls::default(),
    );
    assert_eq!(widened, vec![LineRange { start: 2, end: 4 }]);
}

#[test]
fn matches_widen_to_their_declaration() {
    let mut warnings = vec![];
    let windows = widen_matches(
        PY,
        "m.py",
        &[10],
        vec![LineRange { start: 9, end: 11 }],
        &mut warnings,
        &mut vec![],
        &mut NextCalls::default(),
    );
    assert_eq!(windows, vec![LineRange { start: 8, end: 11 }]);
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn a_match_outside_every_declaration_says_block_did_not_apply() {
    let mut warnings = vec![];
    let windows = widen_matches(
        PY,
        "m.py",
        &[1, 10],
        vec![
            LineRange { start: 1, end: 2 },
            LineRange { start: 9, end: 11 },
        ],
        &mut warnings,
        &mut vec![],
        &mut NextCalls::default(),
    );
    assert_eq!(
        windows,
        vec![
            LineRange { start: 1, end: 2 },
            LineRange { start: 8, end: 11 }
        ]
    );
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].starts_with("block: 1 match(es) sit outside any declaration;"),
        "{warnings:?}"
    );
}

/// A hit on a one-line declaration is not outside every declaration:
/// the warning says there is nothing to widen.
#[test]
fn one_line_declarations_are_named_as_such_not_as_outside() {
    let source =
        "pub struct Id(u8);\npub struct Name(String);\n\nfn body() {\n    1;\n}\n// trailing\n";
    let mut warnings = vec![];
    let windows = widen_matches(
        source,
        "m.rs",
        &[1, 2, 7],
        vec![
            LineRange { start: 1, end: 1 },
            LineRange { start: 2, end: 2 },
            LineRange { start: 7, end: 7 },
        ],
        &mut warnings,
        &mut vec![],
        &mut NextCalls::default(),
    );
    assert_eq!(windows[0], LineRange { start: 1, end: 1 });
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(
        warnings[0].starts_with("block: 2 match(es) are one-line declarations"),
        "{warnings:?}"
    );
    assert!(
        warnings[1].starts_with("block: 1 match(es) sit outside any declaration"),
        "{warnings:?}"
    );
    let mut warnings = vec![];
    widen_ranges(
        source,
        "m.rs",
        vec![LineRange { start: 2, end: 2 }],
        &mut warnings,
        &mut NextCalls::default(),
    );
    assert_eq!(
        warnings,
        [
            "block: line 2 holds a one-line declaration, nothing to widen; returned the requested lines."
        ]
    );
}

/// Several unenclosed ranges of one recovered parse share one warning,
/// which names the parse recovery once.
#[test]
fn unenclosed_ranges_state_the_recovered_parse_once() {
    let flow = "// @flow\ntype Dispatch<A> = A => void;\nfirst();\nsecond();\nexport function useState<S>(init: S): [S, Dispatch<S>] {\n  return [init, () => {}];\n}\n";
    let mut warnings = vec![];
    widen_ranges(
        flow,
        "Hooks.js",
        vec![
            LineRange { start: 1, end: 1 },
            LineRange { start: 3, end: 3 },
            LineRange { start: 4, end: 4 },
        ],
        &mut warnings,
        &mut NextCalls::default(),
    );
    let notes = warnings
        .iter()
        .filter(|warning| warning.contains("could not parse lines"))
        .count();
    assert_eq!(notes, 1, "{warnings:?}");
    assert!(
        warnings
            .iter()
            .any(|w| w.starts_with("block: no declaration encloses lines 1, ")),
        "{warnings:?}"
    );
}

#[test]
fn javascript_member_assigned_functions_are_blocks() {
    let source = "var res = module.exports = {};\n\nres.redirect = function redirect(url) {\n  var status = 302;\n  if (url) {\n    status = 301;\n  }\n  return status;\n};\n";
    let mut warnings = vec![];
    let windows = widen_matches(
        source,
        "lib/response.js",
        &[3],
        vec![LineRange { start: 1, end: 5 }],
        &mut warnings,
        &mut vec![],
        &mut NextCalls::default(),
    );
    assert_eq!(windows, vec![LineRange { start: 3, end: 9 }]);
    assert!(warnings.is_empty(), "{warnings:?}");
}

/// A call-site hit keeps its window when a declaration hit exists; the
/// widened declaration is named.
#[test]
fn call_site_hits_keep_window_when_a_declaration_hit_exists() {
    let source = PY.replace("return 1", "return first(1)");
    let mut warnings = vec![];
    let mut blocks = vec![];
    let windows = widen_matches(
        &source,
        "m.py",
        &[3, 10],
        vec![
            LineRange { start: 3, end: 3 },
            LineRange { start: 10, end: 10 },
        ],
        &mut warnings,
        &mut blocks,
        &mut NextCalls::default(),
    );
    assert_eq!(
        windows,
        vec![
            LineRange { start: 3, end: 5 },
            LineRange { start: 10, end: 10 }
        ]
    );
    assert_eq!(blocks.len(), 1);
    assert_eq!(
        warnings,
        [
            "block: 1 match(es) are inside declarations away from their heads; those keep their context windows because this read also matched declaration heads."
        ]
    );
    assert_eq!(
        (
            blocks[0].symbol_name.as_str(),
            blocks[0].line,
            blocks[0].end_line
        ),
        ("first", 3, 5)
    );
    // No declaration hit: a call site still widens to its caller.
    let windows = widen_matches(
        PY,
        "m.py",
        &[10],
        vec![LineRange { start: 10, end: 10 }],
        &mut warnings,
        &mut vec![],
        &mut NextCalls::default(),
    );
    assert_eq!(windows, vec![LineRange { start: 8, end: 11 }]);
}

/// A hit on an attribute line, on the name line and on the item's first
/// line all read the declaration as written: docs, attributes and body.
#[test]
fn a_hit_on_an_attribute_line_reads_the_whole_declaration() {
    let source = "/// Doc.\n#[inline]\n#[must_use]\nfn target() {\n    body();\n}\n";
    let mut warnings = vec![];
    for hit in [2, 3, 4] {
        let windows = widen_matches(
            source,
            "m.rs",
            &[hit],
            vec![LineRange {
                start: hit,
                end: hit,
            }],
            &mut warnings,
            &mut vec![],
            &mut NextCalls::default(),
        );
        assert_eq!(
            windows,
            vec![LineRange { start: 1, end: 6 }],
            "hit {hit}: {warnings:?}"
        );
    }
}

/// A one-line attributed member is a two-line declaration: a hit on its
/// attribute line or its body reads both lines, and call sites elsewhere in
/// the read keep their windows.
#[test]
fn a_hit_on_a_one_line_attributed_member_reads_its_header_and_body() {
    let source = "struct S;\nimpl S {\n    #[inline]\n    fn f() {}\n    fn g() { S::f(); }\n}\n";
    let mut warnings = vec![];
    for hit in [3, 4] {
        let windows = widen_matches(
            source,
            "m.rs",
            &[hit],
            vec![LineRange {
                start: hit,
                end: hit,
            }],
            &mut warnings,
            &mut vec![],
            &mut NextCalls::default(),
        );
        assert_eq!(
            windows,
            vec![LineRange { start: 3, end: 4 }],
            "hit {hit}: {warnings:?}"
        );
    }
}

/// A Markdown block is the heading's section: it ends on the line before
/// the outline's next heading of the same or a higher level (a deeper one
/// stays inside), or on the file's last line.
#[test]
fn a_markdown_block_is_its_heading_section() {
    let md = "# Guide\n\nIntro.\n\n## Configuration file\n\nSet it.\n\n### Keys\n\nA key.\n\n## Tutorial\n\nStep.\n";
    let headings =
        octocode_runtime_base::content::markdown_headings(md, "README.md").expect("headings");
    let last = md.lines().count();
    for (at, heading) in headings.iter().enumerate() {
        let end = headings[at + 1..]
            .iter()
            .find(|next| next.level <= heading.level)
            .map_or(last, |next| next.line - 1);
        let mut blocks = vec![];
        let windows = widen_matches(
            md,
            "README.md",
            &[heading.line],
            vec![LineRange {
                start: heading.line,
                end: heading.line,
            }],
            &mut vec![],
            &mut blocks,
            &mut NextCalls::default(),
        );
        assert_eq!(
            windows,
            vec![LineRange {
                start: heading.line,
                end
            }],
            "{heading:?}"
        );
        assert_eq!(blocks[0].symbol_name, heading.text);
    }
}

/// A file no grammar outlines (Ruby) keeps the requested lines, says so,
/// and leads on: `next.read` reads the same file from the first hit to its
/// last line, which pages, so the method is read to its end.
#[test]
fn a_block_with_no_outline_leads_to_the_rest_of_the_file() {
    let rb = "class App\n  def route(verb)\n    verb\n  end\nend\n";
    let mut warnings = vec![];
    let mut next = NextCalls::default();
    let windows = widen_matches(
        rb,
        "lib/app.rb",
        &[2],
        vec![LineRange { start: 2, end: 2 }],
        &mut warnings,
        &mut vec![],
        &mut next,
    );
    assert_eq!(windows, vec![LineRange { start: 2, end: 2 }]);
    assert!(warnings[0].contains("next.read"), "{warnings:?}");
    let read = next.read.expect("a read on");
    assert_eq!(
        local_fetch_query::line_ranges(&read.query),
        vec![LineRange {
            start: 2,
            end: rb.lines().count()
        }]
    );
    assert_eq!(local_fetch_query::path(&read.query), "lib/app.rb");
}
