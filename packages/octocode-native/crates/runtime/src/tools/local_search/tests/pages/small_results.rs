//! Small results show every hit and hand off a read of their top file.

use super::*;

/// A result within the page budget and one page of files is shown whole
/// with no paging metadata, however many hits it has; larger results fill
/// their page budget (see
/// `a_small_result_of_many_files_fills_one_page_with_every_row_named`), and
/// a caller cap always wins.
#[test]
fn a_small_result_shows_every_hit_without_paging() {
    let small = "needle\n".repeat(13);
    let body = search_fixture(
        &[("a.txt", &small)],
        ls_query(serde_json::json!({"matchString": "needle"}), None),
    );
    let file = &body["files"][0];
    assert_eq!(file["matches"].as_array().map(Vec::len), Some(13), "{body}");
    assert!(file.get("pagination").is_none(), "{body}");
    assert!(body.get("pagination").is_none(), "{body}");
    assert!(body["next"].get("nextMatchPage").is_none(), "{body}");
    let many_files: Vec<(String, String)> = (0..20)
        .map(|n| (format!("f{n:02}.txt"), "needle\n".to_owned()))
        .collect();
    let refs: Vec<(&str, &str)> = many_files
        .iter()
        .map(|(name, body)| (name.as_str(), body.as_str()))
        .collect();
    let spread = search_fixture(
        &refs,
        ls_query(serde_json::json!({"matchString": "needle"}), None),
    );
    assert_eq!(
        spread["files"].as_array().map(Vec::len),
        Some(20),
        "a page of files is shown whole"
    );
    assert!(spread.get("pagination").is_none(), "{spread}");
    // Far more than one old match page, still well within the budget.
    let moderate = "needle\n".repeat(150);
    let body = search_fixture(
        &[("a.txt", &moderate)],
        ls_query(serde_json::json!({"matchString": "needle"}), None),
    );
    assert_eq!(
        body["files"][0]["matches"].as_array().map(Vec::len),
        Some(150),
        "{body}"
    );
    assert!(body.get("next").is_none(), "{body}");
    let large = "needle\n".repeat(1200);
    let body = search_fixture(
        &[("a.txt", &large)],
        ls_query(serde_json::json!({"matchString": "needle"}), None),
    );
    let shown = body["files"][0]["matches"]
        .as_array()
        .expect("matches")
        .len();
    assert!((100..1200).contains(&shown), "{shown} rows");
    assert!(body["next"]["nextPage"].is_object(), "{body}");
    assert!(body["next"].get("nextMatchPage").is_none(), "{body}");
    assert_eq!(body["files"][0]["pagination"]["hasMore"], true, "{body}");
    let capped = search_fixture(
        &[("a.txt", &small)],
        ls_query(
            serde_json::json!({"matchString": "needle", "matchPageSize": 10}),
            None,
        ),
    );
    assert_eq!(
        capped["files"][0]["matches"].as_array().map(Vec::len),
        Some(10)
    );
}

/// The first page of a complete result hands off one localFetch read: the
/// searched symbol's shown declaration whole, else the top file's hits
/// (±6-line ranges), so the next call reads instead of re-searching;
/// already-contextual results do not.
#[test]
fn a_small_complete_result_hands_off_a_read_of_its_top_file() {
    let body = search_fixture(
        &[("src/a.py", "x = 1\ny = needle\nprint(needle)\n")],
        ls_query(serde_json::json!({"matchString": "needle"}), None),
    );
    let read = &body["next"]["read"];
    assert_eq!(read["tool"], "localFetch", "{body}");
    let query = &read["query"]["queries"][0];
    assert!(
        query["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("src/a.py")),
        "{body}"
    );
    // The read names the hits' ±6-line windows as ranges: exactly the
    // shown hits, whatever localFetch's matchString dialect would match.
    assert_eq!(query["ranges"], serde_json::json!(["1-9"]), "{body}");
    assert!(query.get("matchString").is_none(), "{body}");
    let mut runnable = query.clone();
    runnable["mainGoal"] = serde_json::json!("g");
    runnable["reasoning"] = serde_json::json!("r");
    crate::contracts::validate_query("localFetch", runnable).expect("valid localFetch read");
    let apart: String = (1..=40)
        .map(|line| {
            if line == 3 || line == 30 {
                "needle\n".to_owned()
            } else {
                "x\n".to_owned()
            }
        })
        .collect();
    let apart = search_fixture(
        &[("a.py", &apart)],
        ls_query(serde_json::json!({"matchString": "needle"}), None),
    );
    assert_eq!(
        apart["next"]["read"]["query"]["queries"][0]["ranges"],
        serde_json::json!(["1-9", "24-36"]),
        "{apart}"
    );
    // A declaration search reads the declaring file, wherever its
    // hit count ranks it.
    let mention = "s = 'def needle'\nt = 'def needle'\nu = 'def needle'\n";
    let declared = search_fixture(
        &[
            ("a.py", mention),
            ("b.py", "x = 1\ndef needle():\n    pass\n"),
        ],
        ls_query(serde_json::json!({"matchString": "def needle"}), None),
    );
    assert_eq!(declared["files"][0]["path"], "a.py", "{declared}");
    let read = &declared["next"]["read"]["query"]["queries"][0];
    assert!(
        read["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("b.py")),
        "{declared}"
    );
    // The declaration itself, whole.
    assert_eq!(read["ranges"], serde_json::json!(["2-3"]), "{declared}");
    // A declaration longer than one block read (a large class) is not read
    // whole: the read takes its hit in context, and localFetch offers the
    // member that window cuts.
    let long = format!("x = 1\ndef needle():\n{}", "    pass\n".repeat(450));
    let large = search_fixture(
        &[("b.py", &long)],
        ls_query(serde_json::json!({"matchString": "def needle"}), None),
    );
    assert_eq!(
        large["next"]["read"]["query"]["queries"][0]["ranges"],
        serde_json::json!(["1-8"]),
        "{large}"
    );
    // More windows than one ranges read holds: the read matches the text.
    let scattered: String = (1..=11)
        .map(|_| "needle_a\n".to_owned() + &"x\n".repeat(13))
        .collect();
    let regex = search_fixture(
        &[("a.py", &scattered)],
        ls_query(
            serde_json::json!({"matchString": "needle_(a|b)", "regex": "rust"}),
            None,
        ),
    );
    let read = &regex["next"]["read"]["query"]["queries"][0];
    assert_eq!(read["matchString"], "needle_(a|b)", "{regex}");
    assert_eq!(read["contextLines"], 6, "{regex}");
    assert_eq!(read["regex"], "rust", "{regex}");
    let wide = search_fixture(
        &[
            ("a.py", "needle\n"),
            ("b.py", "needle\n"),
            ("c.py", "needle\n"),
            ("d.py", "needle\n"),
        ],
        ls_query(serde_json::json!({"matchString": "needle"}), None),
    );
    // A wider result still reads its top file: the natural next step.
    assert!(
        wide["next"]["read"]["query"]["queries"][0]["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("a.py")),
        "{wide}"
    );
    let context = search_fixture(
        &[("a.py", "x\nneedle\ny\n")],
        ls_query(
            serde_json::json!({"matchString": "needle", "contextLines": 1}),
            None,
        ),
    );
    assert!(
        context
            .get("next")
            .is_none_or(|next| next.get("read").is_none()),
        "{context}"
    );
}
