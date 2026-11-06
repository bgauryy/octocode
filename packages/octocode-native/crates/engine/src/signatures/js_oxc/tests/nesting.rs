//! Nesting past the deep-stack worker never reaches oxc: a stack overflow
//! aborts the process, so these run every way to nest at the admitted limit
//! (oxc must hold it) and past it (the tree-sitter lane reads it and says why).

use super::super::nesting::{MAX_NESTING, syntax_depth, too_deep};
use crate::graph::GraphFactCode;

/// One way to nest: a file whose syntax tree deepens with `reps`.
pub(super) struct Construct {
    pub(super) name: &'static str,
    pub(super) ext: &'static str,
    pub(super) source: fn(usize) -> String,
}

const PREFIX: &str = "function keep() {}\n";

fn wrap(head: &str, open: &str, middle: &str, close: &str, reps: usize) -> String {
    format!(
        "{PREFIX}{head}{}{middle}{}\n",
        open.repeat(reps),
        close.repeat(reps)
    )
}

pub(super) const CONSTRUCTS: &[Construct] = &[
    Construct {
        name: "paren",
        ext: "js",
        source: |n| wrap("", "(", "1", ")", n),
    },
    Construct {
        name: "array",
        ext: "js",
        source: |n| wrap("", "[", "1", "]", n),
    },
    Construct {
        name: "object",
        ext: "js",
        source: |n| wrap("x = ", "{a:", "1", "}", n),
    },
    Construct {
        name: "block",
        ext: "js",
        source: |n| wrap("", "{", "", "}", n),
    },
    Construct {
        name: "func",
        ext: "js",
        source: |n| wrap("", "function f(){", "", "}", n),
    },
    Construct {
        name: "unary",
        ext: "js",
        source: |n| wrap("", "!", "1", "", n),
    },
    Construct {
        name: "binary",
        ext: "js",
        source: |n| wrap("", "1+", "1", "", n),
    },
    Construct {
        name: "member",
        ext: "js",
        source: |n| format!("{PREFIX}a{}\n", ".a".repeat(n)),
    },
    Construct {
        name: "call",
        ext: "js",
        source: |n| format!("{PREFIX}a{}\n", "()".repeat(n)),
    },
    Construct {
        name: "ifelse",
        ext: "js",
        source: |n| wrap("", "if(a){}else ", "{}", "", n),
    },
    Construct {
        name: "ternary",
        ext: "js",
        source: |n| wrap("", "a?1:", "1", "", n),
    },
    Construct {
        name: "assign",
        ext: "js",
        source: |n| wrap("", "a=", "a", "", n),
    },
    Construct {
        name: "arrow",
        ext: "js",
        source: |n| wrap("", "x=>", "1", "", n),
    },
    Construct {
        name: "new",
        ext: "js",
        source: |n| wrap("", "new ", "A", "", n),
    },
    Construct {
        name: "template",
        ext: "js",
        source: |n| wrap("", "`${", "1", "}`", n),
    },
    Construct {
        name: "jsx",
        ext: "tsx",
        source: |n| wrap("x = ", "<a>", "", "</a>", n),
    },
    Construct {
        name: "jsxMember",
        ext: "tsx",
        source: |n| format!("{PREFIX}x = <a{}/>;\n", ".a".repeat(n)),
    },
    Construct {
        name: "typeArgs",
        ext: "ts",
        source: |n| wrap("type T = ", "A<", "B", "> ", n),
    },
    Construct {
        name: "union",
        ext: "ts",
        source: |n| wrap("type T = ", "A|", "A", "", n),
    },
];

fn depth(construct: &Construct, reps: usize) -> usize {
    let source = (construct.source)(reps);
    syntax_depth(&source, construct.ext).expect("measured").0
}

/// The most repetitions whose tree stays within [`MAX_NESTING`]: depth grows
/// by a fixed step per repetition, so two samples give it.
pub(super) fn reps_at_limit(construct: &Construct) -> usize {
    let (low, high) = (100, 200);
    let (at_low, at_high) = (depth(construct, low), depth(construct, high));
    let step = (at_high - at_low) / (high - low);
    assert!(step > 0, "{} does not nest", construct.name);
    (MAX_NESTING - (at_low - low * step)) / step
}

fn path(construct: &Construct) -> String {
    format!("{}.{}", construct.name, construct.ext)
}

fn declares_keep(facts: &crate::graph::GraphFactsDocument) -> bool {
    facts
        .declarations
        .iter()
        .any(|declaration| declaration.name == "keep")
}

#[test]
fn every_way_to_nest_up_to_the_limit_is_read_by_oxc() {
    for construct in CONSTRUCTS {
        let reps = reps_at_limit(construct);
        let source = (construct.source)(reps);
        assert!(depth(construct, reps) <= MAX_NESTING, "{}", construct.name);
        assert_eq!(too_deep(&source, construct.ext), None, "{}", construct.name);
        let facts = crate::signatures::extract_graph_facts(&source, &path(construct))
            .unwrap_or_else(|| panic!("{} facts", construct.name));
        assert!(
            !facts
                .diagnostics
                .iter()
                .any(|d| d.code == GraphFactCode::ParserFallback),
            "{} at the limit is read by oxc: {:?}",
            construct.name,
            facts.diagnostics
        );
        assert!(declares_keep(&facts), "{}", construct.name);
        let declarations =
            crate::signatures::extract_declarations_document(&source, &path(construct));
        assert!(
            declarations.is_some_and(|facts| declares_keep(&facts)),
            "{}",
            construct.name
        );
    }
}

#[test]
fn nesting_past_the_limit_is_read_by_tree_sitter_and_says_why() {
    for construct in CONSTRUCTS {
        let reps = reps_at_limit(construct) * 2;
        let source = (construct.source)(reps);
        let deep = too_deep(&source, construct.ext)
            .unwrap_or_else(|| panic!("{} past the limit", construct.name));
        assert!(
            deep.depth.is_some_and(|depth| depth > MAX_NESTING),
            "{}",
            construct.name
        );
        let facts = crate::signatures::extract_graph_facts(&source, &path(construct))
            .unwrap_or_else(|| panic!("{} facts", construct.name));
        let fallback = facts
            .diagnostics
            .iter()
            .find(|d| d.code == GraphFactCode::ParserFallback)
            .unwrap_or_else(|| panic!("{} discloses the fallback", construct.name));
        assert!(
            fallback.message.contains(&deep.reason()),
            "{}: {}",
            construct.name,
            fallback.message
        );
        assert_eq!(fallback.line, deep.line, "{}", construct.name);
        assert!(declares_keep(&facts), "{}", construct.name);
    }
}

#[test]
fn a_file_no_longer_than_the_limit_is_never_measured() {
    let source = "(".repeat(MAX_NESTING);
    assert_eq!(too_deep(&source, "js"), None);
    assert!(too_deep(&format!("{source}("), "js").is_some());
}

/// Input tree-sitter cannot parse is one flat `ERROR` node, where oxc still
/// recurses once per token before it reaches the end of the file.
#[test]
fn unparsable_nesting_counts_every_token_it_holds() {
    for open in ["(", "[", "!", "x=>", "a?b:"] {
        let source = format!("{PREFIX}{}", open.repeat(MAX_NESTING * 4));
        assert!(too_deep(&source, "js").is_some(), "{open}");
        let facts = crate::signatures::extract_graph_facts(&source, "unclosed.js")
            .unwrap_or_else(|| panic!("{open} facts"));
        assert!(
            facts
                .diagnostics
                .iter()
                .any(|d| d.code == GraphFactCode::ParserFallback),
            "{open}"
        );
    }
}

/// Every analysis of one text reads its nesting from one measurement: facts,
/// declarations and reference sites alike, again and again.
#[test]
fn a_text_is_measured_once_for_every_analysis() {
    let construct = &CONSTRUCTS[0];
    // Past the one-pass bound, within the limit: oxc reads it after a
    // measured parse.
    let source = format!(
        "{}\n// measured once\n",
        (construct.source)(reps_at_limit(construct))
    );
    let file = path(construct);
    assert_eq!(super::super::nesting::measures::of(&source), 0);
    for _ in 0..2 {
        assert!(crate::signatures::extract_graph_facts(&source, &file).is_some());
        assert!(crate::signatures::extract_declarations_document(&source, &file).is_some());
        assert!(crate::signatures::reference_sites(&source, &file, &[]).is_some());
    }
    assert_eq!(super::super::nesting::measures::of(&source), 1);
}
