//! A whole-window text cut into parts, and the lines its answers name.
use super::*;

fn noul() -> Value {
    json!({"type":"noul","instructions":"Does a test fail?"})
}

fn source() -> Target {
    Target::Source(json!({"tool":"localFetch","query":{"path":"out.log"}}))
}

fn log(lines: usize) -> String {
    (1..=lines).map(|n| format!("line {n}\n")).collect()
}

/// The text without its marker lines, and the marker labels in order.
fn unmarked(marked: &str) -> (String, Vec<String>) {
    let mut text = String::new();
    let mut labels = Vec::new();
    for line in marked.split_inclusive('\n') {
        match line
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix("]\n"))
        {
            Some(label) if label.starts_with('p') => labels.push(label.to_owned()),
            _ => text.push_str(line),
        }
    }
    (text, labels)
}

/// The automatic read page a part is sized by.
const PAGE: usize = crate::execution::RuntimeLimits::DEFAULT.auto_page_chars;

/// The characters a read renders of `text`: each line's JSON text and its
/// gutter.
fn rendered_chars(text: &str) -> usize {
    let gutter = crate::tools::source_view::pagination::gutter_chars(text.lines().count());
    text.split_inclusive('\n')
        .map(|line| rendered(line, gutter))
        .sum()
}

/// Every line sits in exactly one part, in order; a part is a run of whole
/// lines of about one read page, so the part count is the line pages the
/// text fills as a read renders it, at least two and at most the labels one choice may list.
#[test]
fn parts_cover_every_line_once_at_about_one_read_page_each() {
    for (lines, page) in [
        (2, PAGE),
        (3, PAGE),
        (40, PAGE),
        (2_798, PAGE),
        (5_000, 100),
        (LABELS * 3 + 7, 1),
    ] {
        let text = log(lines);
        let (marked, parts) = mark(&text, page).expect("parts");
        let (restored, labels) = unmarked(&marked);
        assert_eq!(restored, text, "{lines} lines");
        let pages = rendered_chars(&text).div_ceil(page).clamp(2, LABELS - 1);
        assert_eq!(parts.len(), pages.min(lines), "{lines} lines at {page}");
        assert_eq!(labels.len(), parts.len());
        assert_eq!(parts.first().map(|part| part.0), Some(1));
        assert_eq!(parts.last().map(|part| part.1), Some(lines));
        for pair in parts.windows(2) {
            assert_eq!(pair[0].1 + 1, pair[1].0, "{lines}: {parts:?}");
        }
        // Balanced: no part holds much more than its share of a page.
        let all: Vec<&str> = text.split_inclusive('\n').collect();
        let gutter = crate::tools::source_view::pagination::gutter_chars(lines);
        let share = rendered_chars(&text).div_ceil(parts.len());
        for &(start, end) in &parts {
            let size: usize = (start..=end).map(|n| rendered(all[n - 1], gutter)).sum();
            assert!(
                size <= share + rendered(all[end - 1], gutter),
                "{lines}: {start}-{end}"
            );
        }
        assert_eq!(labels[0], "p1");
    }
    // A last line without its newline keeps its text; the marker never joins it.
    let (marked, parts) = mark("a\nb", PAGE).expect("two lines");
    assert_eq!(marked, "[p1]\na\n[p2]\nb\n");
    assert_eq!(parts, vec![(1, 1), (2, 2)]);
    // A line longer than a page is one part; the text still has two.
    let lopsided = format!("a\n{}\n", "b".repeat(10 * PAGE));
    assert_eq!(
        mark(&lopsided, PAGE).expect("two parts").1,
        vec![(1, 1), (2, 2)]
    );
    assert!(mark("one line\n", PAGE).is_none());
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
            json!({"source":{"path":"out.log"},"text":"one line\n"}),
        ),
        (
            Target::Source(json!({"url":"https://example.test/a"})),
            state.clone(),
        ),
        (
            Target::Source(
                json!({"tool":"localFetch","query":{"path":"out.log","ranges":[{"startLine":5,"endLine":9}]}}),
            ),
            state.clone(),
        ),
        (Target::Content(json!({"a":1})), json!({"a":1})),
        (
            Target::Content(json!(["a\n", "b\n"])),
            json!(["a\n", "b\n"]),
        ),
    ] {
        assert!(
            locate(&target, &state, &questions, PAGE).is_none(),
            "{state}"
        );
    }
    let boxed = [json!({"type":"box","instructions":"Where is the logo?"})];
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
        json!({"type":"box","instructions":"Where?"}),
        json!({"type":"choice","instructions":"Which kind?","criteria":{"a":null,"unknown":null}}),
        json!({"type":"score","instructions":"How bad?","criteria":["none","some"]}),
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

/// The lines an answer names follow the sectioned rule: a `noul` yes, a
/// choice other than `unknown`, a score; and only when its companion chose a
/// part.
#[test]
fn an_answer_names_its_part_by_the_sectioned_rule() {
    let state = json!({"source":{"path":"out.log"},"text":log(10)});
    let questions = [
        noul(),
        noul(),
        json!({"type":"choice","instructions":"Kind?","criteria":{"a":null,"unknown":null}}),
        json!({"type":"choice","instructions":"Kind?","criteria":{"a":null,"unknown":null}}),
        json!({"type":"score","instructions":"How bad?","criteria":["none","some"]}),
        noul(),
        noul(),
    ];
    // A one-character page: each line is its own part.
    let located = locate(&source(), &state, &questions, 1).expect("located");
    let mut answers = vec![
        json!({"type":"noul","noul":0.9}),
        json!({"type":"noul","noul":0.3}),
        json!({"type":"choice","choice":"a"}),
        json!({"type":"choice","choice":"Unknown"}),
        json!({"type":"score","score":0.0}),
        json!({"type":"noul","noul":0.8}),
        json!({"type":"noul","noul":0.8}),
    ];
    let pick = |label: &str| json!({"type":"choice","choice":label});
    let companions = [
        pick("p4"),
        pick("p4"),
        pick("p7"),
        pick("p7"),
        pick("p10"),
        pick("unknown"),
        Value::Null,
    ];
    located.attach(&mut answers, &companions);
    let lines = |index: usize| answers[index].get("lines").cloned();
    assert_eq!(lines(0), Some(json!({"startLine":4,"endLine":4})));
    assert_eq!(lines(1), None, "a no names no lines");
    assert_eq!(lines(2), Some(json!({"startLine":7,"endLine":7})));
    assert_eq!(lines(3), None, "an unknown choice names no lines");
    assert_eq!(lines(4), Some(json!({"startLine":10,"endLine":10})));
    assert_eq!(lines(5), None, "no part decided it");
    assert_eq!(lines(6), None, "a dropped companion names nothing");
    // Fewer companions than questions (the window split the request): the
    // answers stay as they are.
    let mut kept = vec![json!({"type":"noul","noul":0.9})];
    located.attach(&mut kept, &[]);
    assert!(kept[0].get("lines").is_none());
}
