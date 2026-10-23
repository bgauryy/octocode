use super::*;

/// The paragraph join as it was before B63: it re-classified the whole joined
/// paragraph on every line, which is quadratic in the paragraph length. Kept
/// only as the oracle for output equivalence.
fn quadratic_compact_markdown_newlines(lines: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut fence: Option<FenceState> = None;

    for line in lines {
        if let Some(ref active_fence) = fence {
            if is_fence_close(&line, active_fence) {
                fence = None;
            }
            out.push(line);
            continue;
        }

        if let Some(next_fence) = fence_start(&line) {
            fence = Some(next_fence);
            out.push(line);
            continue;
        }

        if line.trim().is_empty() {
            continue;
        }

        if let Some(prev) = out.last_mut()
            && is_markdown_paragraph_line(prev)
            && is_markdown_paragraph_line(&line)
        {
            prev.push(' ');
            prev.push_str(line.trim());
            continue;
        }

        out.push(line);
    }
    out
}

/// Lines that exercise every branch of the paragraph classifier, including
/// the ones whose class changes once later lines are joined onto them
/// (`-`, `1.`, `[ref`, short marker runs).
const LINE_KINDS: &[&str] = &[
    "plain prose",
    "more prose, with punctuation.",
    "",
    "   ",
    "# Heading",
    "### Deep heading",
    "> quoted",
    "<div>",
    "[ref]: https://example.com",
    "[ref",
    "text]: tail",
    "see ]: here",
    "| a | b |",
    "|---|---|",
    "a|b",
    "---",
    "***",
    "___",
    "- - -",
    "-",
    "--",
    "*",
    "**",
    "_",
    "__",
    "_ _",
    "+",
    "- item",
    "* item",
    "+ item",
    "1. item",
    "12) item",
    "1.",
    "12)",
    "12",
    "3.x",
    "```",
    "```rust",
    "~~~",
    "````",
    "    indented code",
    "    -",
    "trailing space  ",
];

fn lines_of(kinds: &[usize]) -> Vec<String> {
    kinds
        .iter()
        .map(|&k| LINE_KINDS[k % LINE_KINDS.len()].to_owned())
        .collect()
}

#[test]
fn joining_a_long_paragraph_is_linear() {
    // 4000 wrapped lines with no blank line form one paragraph; re-classifying
    // the growing paragraph per line took seconds here and ~68 s in localFetch.
    let source: String = (0..4000)
        .map(|i| format!("Line {i} of a long wrapped paragraph without blank lines.\n"))
        .collect();
    let (out, elapsed) = crate::test_cpu::cpu_time_of(|| minify_markdown_core(&source));
    assert!(!out.contains('\n'), "one joined paragraph");
    assert!(out.starts_with("Line 0 of"), "{}", &out[..40]);
    assert!(out.ends_with("Line 3999 of a long wrapped paragraph without blank lines."));
    assert!(
        elapsed < std::time::Duration::from_millis(500),
        "took {elapsed:?}"
    );
}

#[test]
fn paragraph_joins_match_the_reclassifying_join_on_mixed_documents() {
    let documents = [
        "# Title\n\nFirst line\nsecond line\nthird line\n\n- a\n- b\ncontinued\n\n\
         | k | v |\n|---|---|\n| x | 1 |\nafter table\n\n---\nnext\n\n\
         ```rust\nfn a() {}\nstill code\n```\nprose after fence\nand more\n\n\
         > quote\nlazy line\n\n<div>\nhtml body\n</div>\n",
        "Intro\n1.\nnumbered after\nthird\n\n-\ndash start\nnext line\n",
        "[ref\ntext]: tail\nafter\n\n--\n--\nmore\n\n**\n**\nend\n",
        "Setext\n======\nbody\nbody two\n\n    indented\nprose\n\n~~~\nx\n~~~\ny\nz\n",
    ];
    for document in documents {
        let lines: Vec<String> = document.split('\n').map(str::to_owned).collect();
        assert_eq!(
            compact_markdown_newlines(lines.clone()),
            quadratic_compact_markdown_newlines(lines),
            "{document:?}"
        );
    }
}

#[test]
fn paragraph_joins_keep_the_documented_shape() {
    let lines: Vec<String> = [
        "Intro line",
        "wraps here",
        "- item",
        "1.",
        "after marker",
        "is a new line",
        "# Links",
        "[ref",
        "text]: tail",
        "stops here",
        "# Rules",
        "--",
        "--",
        "breaks the run",
    ]
    .map(str::to_owned)
    .to_vec();
    assert_eq!(
        compact_markdown_newlines(lines),
        [
            "Intro line wraps here",
            "- item",
            "1. after marker",
            "is a new line",
            "# Links",
            "[ref text]: tail",
            "stops here",
            "# Rules",
            "-- --",
            "breaks the run",
        ]
    );
}

proptest::proptest! {
    #[test]
    fn paragraph_joins_match_the_reclassifying_join(
        kinds in proptest::collection::vec(0_usize..LINE_KINDS.len(), 0..48)
    ) {
        let lines = lines_of(&kinds);
        proptest::prop_assert_eq!(
            compact_markdown_newlines(lines.clone()),
            quadratic_compact_markdown_newlines(lines)
        );
    }
}
