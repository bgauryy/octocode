//! A whole-window text cut into parts, and the lines its answers name.
use super::*;
use octocode_test_support::json_fixture;

fn noul() -> Value {
    json_fixture(r#"{"type": "noul", "instructions": "Does a test fail?"}"#)
}

fn source() -> Target {
    Target::Source(json_fixture(
        r#"{"tool": "localFetch", "query": {"path": "out.log"}}"#,
    ))
}

fn log(lines: usize) -> String {
    (1..=lines).map(|n| format!("line {n}\n")).collect()
}

/// The label a part has: the lines it holds, as the judge answers them.
fn label((start, end): (usize, usize)) -> String {
    format!("lines {start}-{end}")
}

/// The span a marker line names (`<lines 4-9>`, `</lines 4-9>`,
/// `<lines 4-9 continues>`) and whether it opens a part; `None` for a line
/// of the text.
fn named(line: &str) -> Option<((usize, usize), bool)> {
    let line = line.trim();
    let (inner, opens) = match line.strip_prefix("</") {
        Some(inner) => (inner, false),
        None => (line.strip_prefix('<')?, true),
    };
    let inner = inner.strip_suffix('>')?;
    let inner = inner.strip_suffix(" continues").unwrap_or(inner);
    let (start, end) = inner.strip_prefix("lines ")?.split_once('-')?;
    Some(((start.parse().ok()?, end.parse().ok()?), opens))
}

/// The part label a marker line names; `None` for a line of the text.
fn marker(line: &str) -> Option<String> {
    named(line).map(|(span, _)| label(span))
}

/// The labels of the parts whose opening marker says they carry on the
/// block the part before them ends in.
fn continued(marked: &str) -> Vec<String> {
    marked
        .lines()
        .filter(|line| line.starts_with('<') && line.ends_with(" continues>"))
        .filter_map(marker)
        .collect()
}

/// A text of blocks separated by blank lines, the kinds a source shows: a
/// heading, a hard-wrapped paragraph (some ending with `:` as a lead-in), a
/// Markdown table, a fenced code block
/// with a blank line inside, a function whose body has a blank line, and
/// one table of `huge` rows in the middle (when not 0). Returns the text and
/// each block's first and last line.
fn blocks(count: usize, huge: usize) -> (String, Vec<(usize, usize)>) {
    let mut text = String::new();
    let mut spans = Vec::new();
    let mut line = 0;
    for index in 0..count {
        let body = match index % 5 {
            0 => format!("## Section {index}\n"),
            // Every other paragraph is a lead-in: it ends with `:` and
            // introduces the table after it.
            1 => {
                let rows = 2 + index % 4;
                (0..rows)
                    .map(|row| {
                        let end = if row + 1 == rows && index % 10 == 1 {
                            ':'
                        } else {
                            '.'
                        };
                        format!("Paragraph {index} wraps its sentence over line {row}{end}\n")
                    })
                    .collect()
            }
            2 => {
                let middle = huge > 0 && index / 5 == count / 10;
                let rows = if middle { huge } else { 2 + index % 3 };
                let mut table = String::from("| name | value |\n|---|---|\n");
                table.extend((0..rows).map(|row| format!("| row {index}.{row} | {row} |\n")));
                table
            }
            3 => format!("```rust\nlet a{index} = 1;\n\nlet b{index} = a{index};\n```\n"),
            _ => format!("fn f{index}() {{\n    let a = {index};\n\n    a\n}}\n"),
        };
        if index > 0 {
            text.push('\n');
            line += 1;
        }
        let lines = body.lines().count();
        spans.push((line + 1, line + lines));
        line += lines;
        text.push_str(&body);
    }
    (text, spans)
}

/// Companion questions enough that the read page alone sets the share
/// ([`part_count`]): the block layout tests cut at a share they choose.
const PAGED: usize = LABELS;

/// The rendered size of lines `start..=end` (1-based) of `text`, and the
/// share of the whole one part aims at for `page` and `companions`.
fn sizes(
    text: &str,
    page: usize,
    companions: usize,
) -> (impl Fn(usize, usize) -> usize + '_, usize) {
    let all: Vec<&str> = text.split_inclusive('\n').collect();
    let gutter = octocode_tools_shared::source_view::pagination::gutter_chars(all.len());
    let total = rendered_chars(text);
    let parts = part_count(total, all.len(), page, companions);
    let size = move |start: usize, end: usize| -> usize {
        (start..=end).map(|n| rendered(all[n - 1], gutter)).sum()
    };
    (size, total.div_ceil(parts))
}

/// Every line in exactly one part, in order.
fn assert_covers(parts: &[(usize, usize)], lines: usize) {
    assert_eq!(parts.first().map(|part| part.0), Some(1));
    assert_eq!(parts.last().map(|part| part.1), Some(lines));
    for pair in parts.windows(2) {
        assert_eq!(pair[0].1 + 1, pair[1].0, "{parts:?}");
    }
}

/// Parts end at block edges: no part boundary falls inside a table, a fenced
/// block, a paragraph or a function body, and no part opens as a
/// continuation, while every part stays within its share of the text.
#[test]
fn parts_end_at_block_edges_and_never_inside_a_block() {
    let (text, blocks) = blocks(60, 0);
    let lines = text.lines().count();
    for page in [600, 900, 1_500] {
        let (size, share) = sizes(&text, page, PAGED);
        assert!(
            blocks.iter().all(|&(a, b)| size(a, b) < share),
            "the fixture's blocks must fit a part at {page}"
        );
        let (marked, Parts { spans: parts, .. }) = mark(&text, page, None, PAGED).expect("parts");
        assert_covers(&parts, lines);
        assert!(parts.len() > 2, "{page}: {parts:?}");
        for &(_, end) in &parts[..parts.len() - 1] {
            let inside = blocks.iter().find(|&&(a, b)| a <= end && end < b);
            assert!(
                inside.is_none(),
                "{page}: part ends at {end} inside {inside:?}"
            );
        }
        for &(start, end) in &parts {
            assert!(
                size(start, end) <= share,
                "{page}: {start}-{end} over {share}"
            );
        }
        assert!(continued(&marked).is_empty(), "{page}: {marked}");
    }
}

/// A rendered reference page (Markdown from HTML): headings and paragraphs
/// with no blank line between them, blank lines only inside its fenced
/// examples. Each of `methods` method sections opens with its `####`
/// signature and holds `#####` subsections (its errors, its examples); every
/// `per_impl` methods an `###` impl heading opens a group. Returns the text
/// and each method section's first and last line.
fn reference_page(methods: usize, per_impl: usize) -> (String, Vec<(usize, usize)>) {
    let mut rows: Vec<String> = vec!["# Struct Lock".into(), "A lock of a value.".into()];
    let mut sections = Vec::new();
    for index in 0..methods {
        if index % per_impl == 0 {
            rows.push(format!("### impl Lock<T{index}>"));
        }
        rows.push("1.0.0 · Source".into());
        let first = rows.len() + 1;
        rows.push(format!("#### pub fn method{index}(&self) -> Result<T, E>"));
        rows.push(format!(
            "Does what method{index} does, and returns its value."
        ));
        rows.push("##### §Errors".into());
        rows.push(format!(
            "method{index} returns an error when the lock is poisoned."
        ));
        rows.push("##### §Examples".into());
        rows.push("```".into());
        rows.push(format!("let value = lock.method{index}();"));
        rows.push(String::new());
        rows.push("assert!(value.is_ok());".into());
        rows.push("```".into());
        sections.push((first, rows.len()));
    }
    (
        rows.iter().map(|row| format!("{row}\n")).collect(),
        sections,
    )
}

/// A page whose only blank lines sit in its code blocks is not a text of
/// blank-line blocks: it is cut at its headings, before the highest one
/// that leaves a part half its share, so a section that fits a part is never
/// split from its heading (a method's errors stay in the part that names the
/// method), and no part opens as a continuation.
#[test]
fn a_page_is_cut_before_its_headings_and_keeps_each_section_whole() {
    let (text, sections) = reference_page(40, 6);
    let lines = text.lines().count();
    for page in [900, 1_500, 3_000] {
        let (size, share) = sizes(&text, page, PAGED);
        assert!(
            sections.iter().all(|&(a, b)| 2 * size(a, b) < share),
            "the fixture's sections must fit half a part at {page}"
        );
        let (marked, Parts { spans: parts, .. }) = mark(&text, page, None, PAGED).expect("parts");
        assert_covers(&parts, lines);
        assert!(parts.len() > 2, "{page}: {parts:?}");
        for &(_, end) in &parts[..parts.len() - 1] {
            let inside = sections.iter().find(|&&(a, b)| a <= end && end < b);
            assert!(
                inside.is_none(),
                "{page}: part ends at {end} inside the section {inside:?}"
            );
        }
        assert!(continued(&marked).is_empty(), "{page}: {marked}");
    }
}

/// A lead-in stays with what it introduces: no part ends on a heading or
/// on a line ending with `:`, unless it is the last part or the next part
/// carries on a cut block.
#[test]
fn no_part_ends_on_a_heading_or_a_lead_in() {
    let lead_in = |line: &str| {
        let line = line.trim();
        line.ends_with(':') || line.starts_with('#')
    };
    for (count, huge, page) in [(60, 0, 600), (60, 0, 900), (60, 0, 1_500), (40, 400, 900)] {
        let (text, _) = blocks(count, huge);
        let rows: Vec<&str> = text.lines().collect();
        assert!(
            rows.iter().any(|row| row.ends_with(':')),
            "the fixture has lead-ins"
        );
        let (marked, Parts { spans: parts, .. }) = mark(&text, page, None, 1).expect("parts");
        let continues = continued(&marked);
        for (index, &(start, end)) in parts[..parts.len() - 1].iter().enumerate() {
            if continues.contains(&label(parts[index + 1])) {
                continue;
            }
            let last = rows[start - 1..end]
                .iter()
                .rev()
                .find(|row| !row.trim().is_empty())
                .expect("a part holds text");
            assert!(
                !lead_in(last),
                "{page}: part {start}-{end} ends on {last:?}"
            );
        }
    }
}

/// A block larger than a part is the only one cut, at a line, and the part
/// after each cut opens saying it carries on that block.
#[test]
fn only_a_block_larger_than_a_part_is_cut_and_the_next_part_says_so() {
    let page = 900;
    let mut rows = 1;
    let (text, blocks) = loop {
        let (text, blocks) = blocks(40, rows);
        let large = {
            let (size, share) = sizes(&text, page, PAGED);
            blocks.iter().any(|&(a, b)| size(a, b) > 2 * share)
        };
        if large {
            break (text, blocks);
        }
        rows *= 2;
    };
    let (size, share) = sizes(&text, page, PAGED);
    let (marked, Parts { spans: parts, .. }) = mark(&text, page, None, PAGED).expect("parts");
    assert_covers(&parts, text.lines().count());
    let continues = continued(&marked);
    let mut cuts = 0;
    for (index, &(_, end)) in parts[..parts.len() - 1].iter().enumerate() {
        let Some(&(a, b)) = blocks.iter().find(|&&(a, b)| a <= end && end < b) else {
            assert!(!continues.contains(&label(parts[index + 1])), "{marked}");
            continue;
        };
        cuts += 1;
        assert!(
            size(a, b) > share,
            "{a}-{b} fits a part but was cut at {end}"
        );
        assert!(
            continues.contains(&label(parts[index + 1])),
            "p{} after the cut at {end}",
            index + 2
        );
    }
    assert!(cuts > 0, "the large block was never cut: {parts:?}");
    assert_eq!(cuts, continues.len(), "{continues:?}");
}

/// The text without its marker lines, and each part's label once, in order.
fn unmarked(marked: &str) -> (String, Vec<String>) {
    let mut text = String::new();
    let mut labels: Vec<String> = Vec::new();
    for line in marked.split_inclusive('\n') {
        match marker(line) {
            Some(label) => {
                if labels.last() != Some(&label) {
                    labels.push(label);
                }
            }
            None => text.push_str(line),
        }
    }
    (text, labels)
}

/// A judge reads which part owns a line from the marker next to it, above or
/// below. Every line of the marked text must be owned the same way from both
/// sides: the nearest marker above and the nearest marker below name one
/// part, and that part's span holds the line. A bare marker before each part
/// fails this: the line above `[p2]` reads as p2's.
#[test]
fn every_line_is_owned_by_its_own_part_from_either_side() {
    for (lines, page) in [(2, PAGE), (40, 1), (3_000, PAGE), (LABELS * 3 + 7, 1)] {
        let text = log(lines);
        let (marked, Parts { spans: parts, .. }) = mark(&text, page, None, 1).expect("parts");
        let rows: Vec<&str> = marked.split_inclusive('\n').collect();
        let mut line = 0;
        for (at, row) in rows.iter().enumerate() {
            if marker(row).is_some() {
                continue;
            }
            line += 1;
            let above = rows[..at].iter().rev().find_map(|row| marker(row));
            let below = rows[at + 1..].iter().find_map(|row| marker(row));
            let owner = parts
                .iter()
                .find(|&&(start, end)| (start..=end).contains(&line))
                .map(|&span| label(span));
            assert!(owner.is_some(), "{lines}: line {line} in no part");
            assert_eq!(above, owner, "{lines}: marker above line {line}");
            assert_eq!(below, owner, "{lines}: marker below line {line}");
        }
        assert_eq!(line, lines);
    }
}

/// Each part is named by the lines it holds: its opening and closing
/// markers state its first and last line exactly, the text between them is
/// those lines of the source, and the companion lists exactly those labels
/// (and `unknown`).
#[test]
fn each_marker_names_exactly_the_lines_it_holds() {
    let (blocky, _) = blocks(40, 400);
    for (text, page) in [(log(3_000), PAGE), (log(40), 1), (blocky, 900)] {
        let original: Vec<&str> = text.split_inclusive('\n').collect();
        let (marked, Parts { spans: parts, .. }) = mark(&text, page, None, 1).expect("parts");
        let (mut open, mut seen, mut line) = (None, Vec::new(), 0);
        for row in marked.split_inclusive('\n') {
            match named(row) {
                Some((span, true)) => {
                    assert!(open.is_none(), "{row} opens inside a part");
                    assert_eq!(span.0, line + 1, "{row} after line {line}");
                    open = Some(span);
                }
                Some((span, false)) => {
                    assert_eq!(open.take(), Some(span), "{row}");
                    assert_eq!(span.1, line, "{row} after line {line}");
                    seen.push(span);
                }
                None => {
                    line += 1;
                    assert!(open.is_some(), "line {line} outside a part");
                    assert_eq!(row.trim_end(), original[line - 1].trim_end(), "line {line}");
                }
            }
        }
        assert_eq!(seen, parts);
        let state = json!({"source":{"path":"out.log"},"text":text});
        let located = locate(&source(), &state, &[noul()], page).expect("located");
        let mut criteria: Vec<String> = located.companions[0]["criteria"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        criteria.sort();
        let mut expected: Vec<String> = parts.iter().map(|&span| label(span)).collect();
        expected.push(NO_PART.into());
        expected.sort();
        assert_eq!(criteria, expected);
    }
}

/// The automatic read page a part is sized by.
const PAGE: usize = octocode_runtime_base::execution::RuntimeLimits::DEFAULT.auto_page_chars;

/// The characters a read renders of `text`: each line's JSON text and its
/// gutter.
fn rendered_chars(text: &str) -> usize {
    let gutter = octocode_tools_shared::source_view::pagination::gutter_chars(text.lines().count());
    text.split_inclusive('\n')
        .map(|line| rendered(line, gutter))
        .sum()
}

/// Every line sits in exactly one part, in order; a part is a run of whole
/// lines of at most one share of the text as a read renders it
/// ([`part_count`]: never more than one read page, at least two parts and at
/// most the labels one choice may list), or one line larger than that; so
/// there are at least as many parts as pages. Past the label limit the last
/// part holds the rest.
#[test]
fn parts_cover_every_line_once_at_most_one_read_page_each() {
    for (lines, page) in [
        (2, PAGE),
        (3, PAGE),
        (40, PAGE),
        (2_798, PAGE),
        (5_000, 100),
        (LABELS * 3 + 7, 1),
    ] {
        let text = log(lines);
        let (marked, Parts { spans: parts, .. }) = mark(&text, page, None, 1).expect("parts");
        let (restored, labels) = unmarked(&marked);
        assert_eq!(restored, text, "{lines} lines");
        let pages = rendered_chars(&text)
            .div_ceil(page)
            .clamp(2, LABELS - 1)
            .min(lines);
        assert!(
            (pages..LABELS).contains(&parts.len()),
            "{lines} lines at {page}: {} parts",
            parts.len()
        );
        assert_eq!(labels.len(), parts.len());
        assert_covers(&parts, lines);
        let (size, share) = sizes(&text, page, 1);
        let bounded = if parts.len() == LABELS - 1 {
            &parts[..parts.len() - 1]
        } else {
            &parts[..]
        };
        for &(start, end) in bounded {
            assert!(
                size(start, end) <= share || start == end,
                "{lines}: {start}-{end} over {share}"
            );
        }
        assert_eq!(labels[0], label(parts[0]));
    }
    // A last line without its newline keeps its text; the marker never joins it.
    let (marked, Parts { spans: parts, .. }) = mark("a\nb", PAGE, None, 1).expect("two lines");
    assert_eq!(
        marked,
        "<lines 1-1>\na\n</lines 1-1>\n<lines 2-2>\nb\n</lines 2-2>\n"
    );
    assert_eq!(parts, vec![(1, 1), (2, 2)]);
    // A line longer than a page is one part; the text still has two.
    let lopsided = format!("a\n{}\n", "b".repeat(10 * PAGE));
    assert_eq!(
        mark(&lopsided, PAGE, None, 1).expect("two parts").1.spans,
        vec![(1, 1), (2, 2)]
    );
    assert!(mark("one line\n", PAGE, None, 1).is_none());
}

/// A selection of `ranges` is located in the file's own lines: each part and
/// each marker names the lines the read shows, not the text's own.
#[test]
fn a_range_selection_is_located_in_file_lines() {
    let target = Target::Source(json_fixture(
        r#"{
            "tool": "localFetch",
            "query": {
                "path": "out.log",
                "ranges": [{"startLine": 100, "endLine": 139}]
            }
        }"#,
    ));
    let state = json!({"source":{"path":"out.log"},"text":log(40)});
    let located = locate(&target, &state, &[noul()], PAGE).expect("a range names lines");
    assert!(located.parts.iter().all(|&(s, e)| 100 <= s && e <= 139));
    let marked = located.state["text"].as_str().expect("text");
    assert!(marked.contains("<lines 100-"), "{marked}");
    // Several ranges map line by line, across the gap.
    let target = Target::Source(json_fixture(
        r#"{
            "tool": "localFetch",
            "query": {
                "path": "out.log",
                "ranges": [
                    {"startLine": 10, "endLine": 29},
                    {"startLine": 200, "endLine": 219}
                ]
            }
        }"#,
    ));
    let located = locate(&target, &state, &[noul()], PAGE).expect("two ranges");
    let (first, last) = (located.parts[0].0, located.parts.last().expect("part").1);
    assert_eq!((first, last), (10, 219));
}

#[test]
fn only_a_text_of_lines_with_a_question_lines_can_answer_is_located() {
    let state = json!({"source":{"path":"out.log"},"text":log(40)});
    let questions = [noul()];
    assert!(locate(&source(), &state, &questions, PAGE).is_some());
    let content = Target::Content(json!(log(40)));
    assert!(locate(&content, &json!(log(40)), &questions, PAGE).is_some());
    for (target, state) in [
        (
            source(),
            json_fixture(r#"{"source": {"path": "out.log"}, "text": "one line\n"}"#),
        ),
        (
            Target::Source(json_fixture(
                r#"{
                    "tool": "localFetch",
                    "query": {
                        "path": "out.log",
                        "ranges": [{"startLine": 5, "endLine": 9}]
                    }
                }"#,
            )),
            state.clone(),
        ),
        (
            Target::Content(json_fixture(r#"{"a": 1}"#)),
            json_fixture(r#"{"a": 1}"#),
        ),
        (
            Target::Content(json_fixture(r#"["a\n", "b\n"]"#)),
            json_fixture(r#"["a\n", "b\n"]"#),
        ),
    ] {
        assert!(
            locate(&target, &state, &questions, PAGE).is_none(),
            "{state}"
        );
    }
    let boxed = [json_fixture(
        r#"{"type": "box", "instructions": "Where is the logo?"}"#,
    )];
    assert!(locate(&source(), &state, &boxed, PAGE).is_none());
}

/// One companion per question that can name lines: a choice of every part
/// plus `unknown`, naming the question it serves; the state keeps its
/// source and carries the marked text.
#[test]
fn each_line_question_gets_one_companion_over_every_part() {
    let state = json!({"source":{"path":"out.log"},"text":log(600)});
    let questions = [
        noul(),
        json_fixture(r#"{"type": "box", "instructions": "Where?"}"#),
        json_fixture(
            r#"{
                "type": "choice",
                "instructions": "Which kind?",
                "criteria": {"a": null, "unknown": null}
            }"#,
        ),
        json_fixture(
            r#"{
                "type": "score",
                "instructions": "How bad?",
                "criteria": ["none", "some"]
            }"#,
        ),
    ];
    let located = locate(&source(), &state, &questions, PAGE).expect("located");
    assert_eq!(located.serves, vec![0, 2, 3]);
    assert_eq!(located.companions.len(), 3);
    assert_eq!(located.state["source"], state["source"]);
    let (text, labels) = unmarked(located.state["text"].as_str().unwrap());
    assert_eq!(text, log(600));
    for companion in &located.companions {
        assert_eq!(companion["type"], "choice");
        let criteria = companion["criteria"].as_object().unwrap();
        assert_eq!(criteria.len(), labels.len() + 1);
        assert!(criteria.len() <= LABELS);
        assert!(criteria.contains_key(NO_PART));
        assert!(labels.iter().all(|label| criteria.contains_key(label)));
    }
    assert!(
        located.companions[0]["instructions"]
            .as_str()
            .unwrap()
            .contains("Does a test fail?")
    );
}

/// The text is screened whole before markers cut it: a key block spread
/// over lines never reaches a part, and the screened lines keep their places.
#[test]
fn a_secret_over_several_lines_is_screened_before_the_text_is_cut() {
    let body = "MIIEpQIBAAKCAQEAsplitKeyBodyAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let text = format!(
        "{}-----BEGIN RSA PRIVATE KEY-----\n{body}\n{body}\n-----END RSA PRIVATE KEY-----\n{}",
        log(300),
        log(300)
    );
    let located =
        locate(&Target::Content(json!(text)), &json!(text), &[noul()], PAGE).expect("located");
    let marked = located.state.as_str().unwrap();
    assert!(!marked.contains(body), "key body reached a part");
    let (restored, _) = unmarked(marked);
    assert_eq!(restored.lines().count(), text.lines().count());
}

/// The lines an answer names: any `noul` (a low one keeps its chosen part), a
/// choice other than `unknown`, a score; and only when its companion chose a
/// part.
#[test]
fn an_answer_names_its_part_by_the_sectioned_rule() {
    let state = json!({"source":{"path":"out.log"},"text":log(10)});
    let questions = [
        noul(),
        noul(),
        json_fixture(
            r#"{
                "type": "choice",
                "instructions": "Kind?",
                "criteria": {"a": null, "unknown": null}
            }"#,
        ),
        json_fixture(
            r#"{
                "type": "choice",
                "instructions": "Kind?",
                "criteria": {"a": null, "unknown": null}
            }"#,
        ),
        json_fixture(
            r#"{
                "type": "score",
                "instructions": "How bad?",
                "criteria": ["none", "some"]
            }"#,
        ),
        noul(),
        noul(),
        noul(),
        noul(),
    ];
    // A one-character page: each line is its own part.
    let located = locate(&source(), &state, &questions, 1).expect("located");
    let mut answers = vec![
        json_fixture(r#"{"type": "noul", "noul": 0.9}"#),
        json_fixture(r#"{"type": "noul", "noul": 0.3}"#),
        json_fixture(r#"{"type": "choice", "choice": "a"}"#),
        json_fixture(r#"{"type": "choice", "choice": "Unknown"}"#),
        json_fixture(r#"{"type": "score", "score": 0.0}"#),
        json_fixture(r#"{"type": "noul", "noul": 0.8}"#),
        json_fixture(r#"{"type": "noul", "noul": 0.8}"#),
        json_fixture(r#"{"type": "noul", "noul": 0.8}"#),
        json_fixture(r#"{"type": "noul", "noul": 0.8}"#),
    ];
    let pick = |label: &str| json!({"type":"choice","choice":label});
    let companions = [
        pick("lines 4-4"),
        pick("lines 4-4"),
        pick("lines 7-7"),
        pick("lines 7-7"),
        pick("lines 10-10"),
        pick("unknown"),
        Value::Null,
        pick("lines 3-5"),
        pick("p4"),
    ];
    located.attach(&mut answers, &companions);
    let lines = |index: usize| answers[index].get("lines").cloned();
    assert_eq!(
        lines(0),
        Some(json_fixture(r#"{"startLine": 4, "endLine": 4}"#))
    );
    assert_eq!(
        lines(1),
        Some(json_fixture(r#"{"startLine": 4, "endLine": 4}"#)),
        "a low noul keeps the part its judge chose"
    );
    assert_eq!(
        lines(2),
        Some(json_fixture(r#"{"startLine": 7, "endLine": 7}"#))
    );
    assert_eq!(lines(3), None, "an unknown choice names no lines");
    assert_eq!(
        lines(4),
        Some(json_fixture(r#"{"startLine": 10, "endLine": 10}"#))
    );
    assert_eq!(lines(5), None, "no part decided it");
    assert_eq!(lines(6), None, "a dropped companion names nothing");
    // Only a part's own label names lines: a span no part has, or another
    // label form, names none.
    assert_eq!(lines(7), None, "a span no part has");
    assert_eq!(lines(8), None, "not a part's label");
    // Fewer companions than questions (the window split the request): the
    // answers stay as they are.
    let mut kept = vec![json_fixture(r#"{"type": "noul", "noul": 0.9}"#)];
    located.attach(&mut kept, &[]);
    assert!(kept[0].get("lines").is_none());
}

/// A JSON manifest of `lines` lines whose every line but the braces is
/// indented, the shape of a `package.json`: no block edge inside it.
fn manifest(lines: usize) -> String {
    let mut text = String::from("{\n");
    text.extend((2..lines).map(|n| format!("  \"field{n}\": \"value of field {n}\",\n")));
    text.push_str("}\n");
    text
}

/// A text under one read page is cut finer than in halves: a fact on two of
/// its 192 lines is read with a part of them, not with half the file.
#[test]
fn a_text_under_one_page_is_cut_finer_than_halves() {
    let text = manifest(192);
    assert!(
        rendered_chars(&text) < PAGE,
        "the fixture is under one page"
    );
    let state = json!({"source":{"path":"package.json"},"text":text});
    let located = locate(&source(), &state, &[noul()], PAGE).expect("located");
    assert!(located.parts.len() > 2, "{:?}", located.parts);
    let deciding = located
        .parts
        .iter()
        .find(|&&(start, end)| start <= 60 && 61 <= end)
        .or_else(|| {
            located
                .parts
                .iter()
                .find(|&&(start, end)| start <= 60 && 60 <= end)
        })
        .expect("a part holds line 60");
    assert!(
        deciding.1 - deciding.0 + 1 < 192 / 2,
        "{deciding:?} of {:?}",
        located.parts
    );
    assert_covers(&located.parts, 192);
}

/// Every companion lists every part, so more companion questions buy fewer,
/// larger parts; never fewer than one per read page.
#[test]
fn each_companion_question_coarsens_the_parts() {
    let text = manifest(192);
    let lines = text.lines().count();
    let total = rendered_chars(&text);
    let counts = [1, 2, 4, 8].map(|companions| part_count(total, lines, PAGE, companions));
    assert!(
        counts.windows(2).all(|pair| pair[0] >= pair[1]),
        "{counts:?}"
    );
    assert!(counts.iter().all(|&count| count >= 2), "{counts:?}");
    let long = log(20_000);
    let (total, lines) = (rendered_chars(&long), 20_000);
    let pages = total.div_ceil(PAGE);
    assert!(part_count(total, lines, PAGE, 255) >= pages);
}

/// A URL capture is located in the lines of its saved file, which its read
/// reads: its answers name lines as a file's do.
#[test]
fn a_url_capture_is_located_on_its_saved_lines() {
    let target = Target::Source(json_fixture(r#"{"url": "https://example.test/a"}"#));
    let state = json!({"source":{"url":"https://example.test/a","section":null},"text":log(40)});
    let located = locate(&target, &state, &[noul()], 1).expect("a capture names lines");
    assert_covers(&located.parts, 40);
}

/// A URL capture's read narrows to the deciding lines of its saved file,
/// at the capture's snapshot: `readChars` counts exactly those lines.
#[test]
fn a_url_capture_reads_only_its_deciding_lines() {
    let text = log(40);
    let whole = source_capture::read_lead(
        octocode_runtime_base::id::ToolId::LocalFetch,
        &json!({"path":"/cache/capture/url/d1/source.txt","ranges":[{"startLine":1,"endLine":40}]}),
        Some(&"a1".repeat(32)),
    );
    let mut captured = Captured {
        state: Value::Null,
        read: whole.clone(),
        read_chars: text.chars().count(),
        read_pages: false,
        source: None,
        pin: None,
    };
    let source = json!({"url":"https://example.test/a"});
    let state = json!({"source":{"url":"https://example.test/a"},"text":text});
    let answers = [json!({"type":"noul","noul":0.9,"lines":{"startLine":3,"endLine":4}})];
    narrow_read(&mut captured, &source, &state, &answers, true, PAGE);
    let (row, saved) = (
        &captured.read["input"]["queries"][0],
        &whole["input"]["queries"][0],
    );
    assert_eq!(row["ranges"], json!([{"startLine":3,"endLine":4}]));
    assert_eq!(
        (&row["path"], &row["snapshot"]),
        (&saved["path"], &saved["snapshot"])
    );
    assert_eq!(captured.read_chars, "line 3\nline 4\n".chars().count());
}

/// With no yes, the read cites the lines the answers named, the nearest
/// miss, so a negative is checked where it came closest; a yes alone
/// decides the read when there is one; no named lines keep the whole read.
#[test]
fn with_no_yes_the_read_cites_the_nearest_miss() {
    let text = log(40);
    let whole = source_capture::read_lead(
        octocode_runtime_base::id::ToolId::LocalFetch,
        &json!({"path":"out.log","ranges":[{"startLine":1,"endLine":40}]}),
        Some(&"a1".repeat(32)),
    );
    let captured = || Captured {
        state: Value::Null,
        read: whole.clone(),
        read_chars: text.chars().count(),
        read_pages: false,
        source: None,
        pin: None,
    };
    let source = json!({"tool":"localFetch","query":{"path":"out.log"}});
    let state = json!({"source":{"path":"out.log"},"text":text});
    let no = |start: u64, end: u64| json!({"type":"noul","noul":0.2,"lines":{"startLine":start,"endLine":end}});
    let ranges = |answers: &[Value]| {
        let mut read = captured();
        narrow_read(&mut read, &source, &state, answers, true, PAGE);
        read.read["input"]["queries"][0]["ranges"].clone()
    };
    assert_eq!(
        ranges(&[no(12, 14), no(30, 31)]),
        json!([{"startLine":12,"endLine":14},{"startLine":30,"endLine":31}])
    );
    let yes = json!({"type":"noul","noul":0.9,"lines":{"startLine":3,"endLine":4}});
    assert_eq!(
        ranges(&[no(12, 14), yes]),
        json!([{"startLine":3,"endLine":4}])
    );
    assert_eq!(
        ranges(&[json!({"type":"noul","noul":0.2})]),
        whole["input"]["queries"][0]["ranges"]
    );
}
