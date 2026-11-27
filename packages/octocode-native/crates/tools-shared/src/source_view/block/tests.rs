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
        &mut vec![],
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
        &mut vec![],
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
        &mut vec![],
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
        &mut vec![],
        &mut NextCalls::default(),
    );
    assert_eq!(widened, whole);
    let mut blocks = vec![];
    let windows = widen_matches(
        &source,
        "m.py",
        &[20],
        vec![LineRange { start: 20, end: 20 }],
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
        &mut vec![],
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
        &mut vec![],
        &mut NextCalls::default(),
    );
    assert_eq!(widened, vec![LineRange { start: 2, end: 4 }]);
}

/// L11: one extent per declaration. A match on any of its lines (its
/// attribute, its name line, its body) and a `ranges` block read of any of
/// them select the same lines, docs included, and each names the
/// declaration in `blocks` once, with the line its shown lines start at
/// when that is above its name line.
#[test]
fn a_block_has_one_extent_wherever_the_hit_is() {
    let source =
        "use std::fmt;\n\n/// Doc one.\n/// Doc two.\n#[inline]\nfn target() {\n    body();\n}\n";
    let whole = vec![LineRange { start: 3, end: 8 }];
    let named = |blocks: &[crate::source_view::types::DeclaredBlock]| {
        blocks
            .iter()
            .map(|block| {
                // The block's shown lines start where the read's do.
                assert_eq!(block.doc_start_line, Some(whole[0].start), "{block:?}");
                assert!(whole[0].start < block.line, "{block:?}");
                (block.symbol_name.clone(), block.line, block.end_line)
            })
            .collect::<Vec<_>>()
    };
    let target = vec![("target".to_owned(), 6, 8)];
    for line in 5..=8 {
        let at = vec![LineRange {
            start: line,
            end: line,
        }];
        let (mut warnings, mut blocks) = (vec![], vec![]);
        let matched = widen_matches(
            source,
            "m.rs",
            &[line],
            at.clone(),
            &mut warnings,
            &mut blocks,
            &mut NextCalls::default(),
        );
        assert_eq!(matched, whole, "match on line {line}: {warnings:?}");
        assert_eq!(named(&blocks), target, "match on line {line}");
        let mut blocks = vec![];
        let ranged = widen_ranges(
            source,
            "m.rs",
            at,
            &mut warnings,
            &mut blocks,
            &mut NextCalls::default(),
        );
        assert_eq!(ranged, whole, "range at line {line}: {warnings:?}");
        assert_eq!(named(&blocks), target, "range at line {line}");
    }
}

/// L10: `contextLines` sizes the window of a match that keeps one. When
/// every match is widened to its declaration it sized nothing, and the
/// read says so; a read with a match that keeps its window, or one that
/// asked for no context, does not.
#[test]
fn context_lines_that_size_nothing_are_named() {
    let unused = |hits: &[usize], context: usize| {
        let mut warnings = vec![];
        let windows = hits
            .iter()
            .map(|&hit| LineRange {
                start: hit - context,
                end: hit + context,
            })
            .collect();
        widen_matches(
            PY,
            "m.py",
            hits,
            windows,
            &mut warnings,
            &mut vec![],
            &mut NextCalls::default(),
        );
        warnings.iter().any(|warning| warning == CONTEXT_UNUSED)
    };
    assert!(unused(&[4, 10], 1), "every match widened");
    assert!(!unused(&[4, 10], 0), "no context asked");
    // Line 1 sits outside every declaration: its window is the context.
    assert!(!unused(&[2, 10], 1), "a match keeps its window");
}

#[test]
fn matches_widen_to_their_declaration() {
    let mut warnings = vec![];
    let mut blocks = vec![];
    let windows = widen_matches(
        PY,
        "m.py",
        &[10],
        vec![LineRange { start: 10, end: 10 }],
        &mut warnings,
        &mut blocks,
        &mut NextCalls::default(),
    );
    assert_eq!(windows, vec![LineRange { start: 8, end: 11 }]);
    assert!(warnings.is_empty(), "{warnings:?}");
    // No doc comment above it: the block starts at its name line, and the
    // entry states no other start (nor a key for one).
    assert_eq!(blocks.len(), 1, "{blocks:?}");
    assert_eq!(blocks[0].line, windows[0].start, "{blocks:?}");
    let wire = serde_json::to_value(&blocks[0]).expect("block");
    assert!(wire.get("docStartLine").is_none(), "{wire}");
}

/// A match outside every declaration of a clean parse keeps its window
/// silently: `blocks` against `matchedLines` shows it was not widened, and
/// nothing bounds a claim about it.
#[test]
fn a_match_outside_every_declaration_of_a_clean_parse_adds_no_warning() {
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
    assert!(warnings.is_empty(), "{warnings:?}");
}

/// A match outside every declaration of a parse the parser recovered from
/// is named with the skipped spans that start before it, and only those: a
/// later span cannot hide what encloses it.
#[test]
fn an_unenclosed_match_names_only_the_recovered_spans_before_it() {
    let flow = "// @flow\ntype Dispatch<A> = A => void;\nfirst();\nexport function useState<S>(init: S): [S, Dispatch<S>] {\n  return [init, () => {}];\n}\ntype Later<B> = B => void;\n";
    let read = |hits: &[usize]| {
        let mut warnings = vec![];
        widen_matches(
            flow,
            "Hooks.js",
            hits,
            hits.iter()
                .map(|&hit| LineRange {
                    start: hit,
                    end: hit,
                })
                .collect(),
            &mut warnings,
            &mut vec![],
            &mut NextCalls::default(),
        );
        warnings
    };
    let before = read(&[3]);
    assert_eq!(before.len(), 1, "{before:?}");
    assert!(
        before[0].starts_with("block: no declaration encloses line 3; it kept its context window. The parser could not parse lines 2"),
        "{before:?}"
    );
    assert!(
        !before[0].contains('7'),
        "a later span is not named: {before:?}"
    );
    // A hit before every skipped span: the parse hid nothing that encloses it.
    assert!(read(&[1]).is_empty(), "{:?}", read(&[1]));
}

/// A hit on a one-line declaration already holds it whole: nothing is
/// unread, so it adds no warning; a hit outside every declaration of a
/// clean parse adds none either.
#[test]
fn one_line_declarations_and_clean_unenclosed_hits_add_no_warning() {
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
    assert!(warnings.is_empty(), "{warnings:?}");
    let mut warnings = vec![];
    widen_ranges(
        source,
        "m.rs",
        vec![LineRange { start: 2, end: 2 }],
        &mut warnings,
        &mut vec![],
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
        &mut vec![],
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
        vec![LineRange { start: 3, end: 3 }],
        &mut warnings,
        &mut vec![],
        &mut NextCalls::default(),
    );
    assert_eq!(windows, vec![LineRange { start: 3, end: 9 }]);
    assert!(warnings.is_empty(), "{warnings:?}");
}

/// A call site inside the declaration another hit returns whole is already
/// read: no warning sends the reader to widen it again.
#[test]
fn a_call_site_inside_a_returned_declaration_is_not_named() {
    let mut warnings = vec![];
    let windows = widen_matches(
        PY,
        "m.py",
        &[3, 4],
        vec![
            LineRange { start: 3, end: 3 },
            LineRange { start: 4, end: 4 },
        ],
        &mut warnings,
        &mut vec![],
        &mut NextCalls::default(),
    );
    assert_eq!(windows[0], LineRange { start: 3, end: 5 });
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
    // The call site is named by its line, with the read that widens it.
    assert_eq!(
        warnings,
        [
            "block: line 10 kept its window: it is inside a declaration, not its head, and other matches are heads; a ranges read of it with block:true reads that declaration."
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

/// A file no grammar outlines (Ruby) widens to the indentation-derived edge
/// of the block, and the warning says that edge is an estimate.
#[test]
fn a_block_with_no_outline_ends_where_the_indentation_does() {
    let rb = "class App\n  def route(verb)\n    verb\n\n    other\n  end\n\n  def next_one\n  end\nend\n";
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
    assert_eq!(windows, vec![LineRange { start: 2, end: 6 }]);
    assert!(warnings[0].contains("indentation"), "{warnings:?}");
    assert!(warnings[0].contains("estimate"), "{warnings:?}");
    assert!(next.read.is_none());
}

/// With no later line at the head's indent or less the edge is unknown: the
/// window stays and the related read reads to the file's last line, which pages.
#[test]
fn a_block_with_no_outline_and_no_edge_leads_to_the_rest_of_the_file() {
    let rb = "  def route(verb)\n    verb\n    other\n";
    let mut warnings = vec![];
    let mut next = NextCalls::default();
    let windows = widen_matches(
        rb,
        "lib/app.rb",
        &[1],
        vec![LineRange { start: 1, end: 1 }],
        &mut warnings,
        &mut vec![],
        &mut next,
    );
    assert_eq!(windows, vec![LineRange { start: 1, end: 1 }]);
    assert!(warnings[0].contains("related read"), "{warnings:?}");
    let read = next.read.expect("a read on");
    assert_eq!(
        local_fetch_query::line_ranges(&read.query),
        vec![LineRange {
            start: 1,
            end: rb.lines().count()
        }]
    );
    assert_eq!(local_fetch_query::path(&read.query), "lib/app.rb");
}

/// The block a no-outline file's hit widens to: its window's end line.
fn indented_block_end(source: &str, path: &str, hit: usize) -> usize {
    let mut warnings = vec![];
    let windows = widen_matches(
        source,
        path,
        &[hit],
        vec![LineRange {
            start: hit,
            end: hit,
        }],
        &mut warnings,
        &mut vec![],
        &mut NextCalls::default(),
    );
    assert!(warnings[0].contains("estimate"), "{warnings:?}");
    windows[0].end
}

/// A brace on its own line under the head (Allman) opens the body: the block
/// runs to the brace that closes it, not to the head line.
#[test]
fn an_allman_brace_block_runs_to_its_closing_brace() {
    let php = "<?php\nclass Str\n{\n    public static function slug($title)\n    {\n        $title = trim($title);\n\n        return $title;\n    }\n\n    public static function other()\n    {\n        return 1;\n    }\n}\n";
    assert_eq!(indented_block_end(php, "src/Str.php", 4), 9);
    assert_eq!(indented_block_end(php, "src/Str.php", 2), 15);
    let cs = "namespace A\n{\n    class B\n    {\n        void M()\n        {\n            Go();\n        }\n    }\n}\n";
    assert_eq!(indented_block_end(cs, "src/B.zig", 5), 8);
}

/// A wrapped parameter list closes at the head's indent: the body brace after
/// it (on the same line or the next) carries the block on.
#[test]
fn a_wrapped_signature_does_not_end_a_brace_block() {
    let same = "class A\n{\n    function f(\n        $a,\n        $b\n    ) {\n        return 1;\n    }\n\n    function g()\n    {\n    }\n}\n";
    assert_eq!(indented_block_end(same, "src/A.php", 3), 8);
    let next =
        "class A\n{\n    function f(\n        $a\n    ): int\n    {\n        return 1;\n    }\n}\n";
    assert_eq!(indented_block_end(next, "src/A.php", 3), 8);
}

/// A chain closing and opening again (`} else {`) stays one block.
#[test]
fn an_else_chain_stays_one_block() {
    let zig = "fn f() void {\n    if (a) {\n        b();\n    } else {\n        c();\n    }\n}\n";
    assert_eq!(indented_block_end(zig, "src/f.zig", 2), 6);
}

/// A one-line declaration followed by a K&R one is not merged with it.
#[test]
fn a_one_line_declaration_does_not_swallow_the_next_brace_block() {
    let php = "class A\n{\n    function a() { return 1; }\n    function b() {\n        return 2;\n    }\n}\n";
    assert_eq!(indented_block_end(php, "src/A.php", 3), 3);
}
