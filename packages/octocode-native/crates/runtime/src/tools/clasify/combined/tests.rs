//! The combination rules of a sectioned source, one per question type.
use super::*;

fn section(start: u64, end: u64, cells: Vec<Value>) -> JudgedSection {
    JudgedSection {
        lines: json!({"startLine":start,"endLine":end}),
        cells: cells
            .into_iter()
            .map(|answer| Some(json!({"status":"answered","answer":answer})))
            .collect(),
    }
}

#[test]
fn noul_is_the_highest_section_and_names_its_lines() {
    let sections = [
        section(1, 10, vec![json!({"noul":0.02})]),
        section(11, 20, vec![json!({"noul":0.97})]),
        section(21, 30, vec![json!({"noul":0.4})]),
    ];
    let answer = combine(&json!({"type":"noul"}), &sections, 0).unwrap();
    assert_eq!(
        answer,
        json!({"noul":0.97,"lines":{"startLine":11,"endLine":20}})
    );
}

fn pick(label: &str, confidence: f64) -> Value {
    json!({"choice":label,"confidence":confidence})
}

/// One section holds the evidence and the rest do not: the sections
/// that chose `unknown` abstain, so the one that knows decides, and the
/// shares still show how many abstained.
#[test]
fn one_knowing_section_is_not_outvoted_by_abstaining_ones() {
    let mut sections: Vec<JudgedSection> = (0..18)
        .map(|at| section(at * 10 + 1, at * 10 + 10, vec![pick("unknown", 0.95)]))
        .collect();
    sections.insert(7, section(500, 520, vec![pick("rocksdb", 0.8)]));
    let question = json!({"type":"choice","criteria":{
        "rocksdb":"RocksDB","sqlite":"SQLite","unknown":"Not defined here"}});
    let answer = combine(&question, &sections, 0).unwrap();
    assert_eq!(answer["choice"], "rocksdb");
    assert_eq!(answer["confidence"], 0.8);
    assert_eq!(answer["lines"], json!({"startLine":500,"endLine":520}));
    assert_eq!(
        answer["probabilities"],
        json!({"rocksdb":0.05,"sqlite":0.0,"unknown":0.95})
    );
}

/// Sections that give different substantive labels: the most confident
/// section decides, whatever the count, and each label's share stays.
#[test]
fn conflicting_substantive_sections_take_the_most_confident() {
    let sections = [
        section(1, 10, vec![pick("types", 0.6)]),
        section(11, 20, vec![pick("logic", 0.9)]),
        section(21, 30, vec![pick("types", 0.7)]),
        section(31, 40, vec![pick("unknown", 1.0)]),
    ];
    let question = json!({"type":"choice","criteria":{"types":"t","logic":"l","unknown":"u"}});
    let answer = combine(&question, &sections, 0).unwrap();
    assert_eq!(answer["choice"], "logic");
    assert_eq!(answer["confidence"], 0.9);
    assert_eq!(
        answer["probabilities"],
        json!({"types":0.5,"logic":0.25,"unknown":0.25})
    );
    assert_eq!(answer["lines"], json!({"startLine":11,"endLine":20}));
}

/// Every section abstained: the answer is `unknown`, and it decides no
/// lines to read.
#[test]
fn all_abstaining_sections_answer_unknown_without_lines() {
    let sections = [
        section(1, 10, vec![pick("Unknown", 0.7)]),
        section(11, 20, vec![pick("Unknown", 0.9)]),
    ];
    let question = json!({"type":"choice","criteria":{"yes":"y","no":"n","Unknown":"u"}});
    let answer = combine(&question, &sections, 0).unwrap();
    assert_eq!(answer["choice"], "Unknown");
    assert_eq!(answer["confidence"], 0.9);
    assert_eq!(
        answer["probabilities"],
        json!({"yes":0.0,"no":0.0,"Unknown":1.0})
    );
    assert!(answer.get("lines").is_none());
    let source = json!({"tool":"localFetch","query":{"path":"big.rs"}});
    let answers = json!({"kind":answer});
    assert!(deciding_read(&source, &json!({}), &answers, false).is_none());
}

#[test]
fn score_is_the_highest_level_with_the_rows_legend() {
    let level =
        |score: f64| json!({"score":score,"confidence":1.0,"probabilities":{"0":0.0,"1":1.0}});
    let sections = [
        section(1, 10, vec![level(0.1)]),
        section(11, 20, vec![level(1.0)]),
    ];
    let question = json!({"type":"score","criteria":["low","high"]});
    let answer = combine(&question, &sections, 0).unwrap();
    assert_eq!(answer["score"], 1.0);
    assert_eq!(answer["legend"], json!({"0":"low","1":"high"}));
    assert_eq!(answer["lines"], json!({"startLine":11,"endLine":20}));
}

/// A failed section does not hide the others; the coverage counts it,
/// and a question no section answered has no answer.
#[test]
fn failed_sections_are_counted_and_never_answer() {
    let failed = json!({"status":"error","error":{"errorCode":"timeout","error":"late"}});
    let sections = [
        section(1, 10, vec![json!({"noul":0.8})]),
        JudgedSection {
            lines: json!({"startLine":11,"endLine":20}),
            cells: vec![Some(failed.clone())],
        },
        JudgedSection {
            lines: json!({"startLine":21,"endLine":30}),
            cells: vec![None],
        },
    ];
    assert_eq!(
        coverage(&sections),
        json!({"totalSections":3,"answered":1,"error":1,"unassessed":1})
    );
    let answer = combine(&json!({"type":"noul"}), &sections, 0).unwrap();
    assert_eq!(answer["noul"], 0.8);
    let none = [JudgedSection {
        lines: json!({"startLine":1,"endLine":2}),
        cells: vec![Some(failed)],
    }];
    assert!(combine(&json!({"type":"noul"}), &none, 0).is_none());
}

/// The read holds the yes sections and every choice or score section,
/// merged; a `noul` no adds nothing to read.
#[test]
fn the_read_covers_the_deciding_lines_only() {
    let source = json!({"tool":"localFetch","query":{"path":"big.rs"}});
    let answers = json!({
        "yes":{"noul":0.9,"lines":{"startLine":11,"endLine":20}},
        "no":{"noul":0.1,"lines":{"startLine":500,"endLine":600}},
        "kind":{"choice":"types","lines":{"startLine":21,"endLine":30}},
    });
    let read = deciding_read(&source, &json!({}), &answers, false).unwrap();
    assert_eq!(
        read["input"]["queries"][0]["ranges"],
        json!([{"startLine":11,"endLine":30}])
    );
    let none = json!({"no":{"noul":0.1,"lines":{"startLine":1,"endLine":2}}});
    assert!(deciding_read(&source, &json!({}), &none, false).is_none());
    let page = json!({"url":"https://example.com/a"});
    assert!(deciding_read(&page, &json!({}), &answers, false).is_none());
}
