//! A repair note names a form to send only for a row that runs: a row that
//! still fails after its repair says only what was read, so the error's own
//! recovery leads and no note advises a form the contract refuses.
use super::*;

fn rows_run(tool: &str, ran: &Value) -> Vec<bool> {
    let sent = ran["queries"].as_array().map_or(0, Vec::len);
    // A batch none of whose rows runs is refused whole.
    prepare_rows(tool, ran.clone()).map_or_else(
        |_| vec![false; sent],
        |rows| rows.iter().map(Result::is_ok).collect(),
    )
}

/// The same sort repair on two rows: the row that runs names the form to
/// send; the row whose repaired sort the contract still refuses says only
/// how its sort was read.
#[test]
fn a_repair_on_a_row_that_still_fails_says_only_what_was_read() {
    let sent = json!({"queries": [
        {"operation": "search", "keywords": ["vite plugin"], "sort": "stars", "limit": 1},
        {"operation": "search", "keywords": ["vite plugin"], "sort": "pushedAt", "limit": 1}
    ]});
    let (ran, notes) = normalized_input("ghSearchRepo", sent.clone());
    assert_eq!(rows_run("ghSearchRepo", &ran), [true, false], "{ran}");
    let runs = |row: usize| format!("`sort: {}`", sketch(&ran["queries"][row]["sort"]));
    assert!(
        notes.iter().any(|note| note.starts_with("queries[0]: ")
            && note.ends_with(&format!(" as {}; send {} next time.", runs(0), runs(0)))),
        "{notes:?}"
    );
    let failed: Vec<&String> = notes
        .iter()
        .filter(|note| note.starts_with("queries[1]: "))
        .collect();
    assert!(
        matches!(failed.as_slice(), [note]
            if note.ends_with(&format!(r#"read `sort: "pushedAt"` as {}."#, runs(1)))),
        "{notes:?}"
    );
    assert!(
        failed.iter().all(|note| !note.contains("send")),
        "{notes:?}"
    );
}

/// A query a clasify source holds is repaired as its tool's direct call,
/// and said by the same rule: its repair names no form while that query
/// still fails.
#[test]
fn a_source_query_that_still_fails_says_only_what_was_read() {
    let query = json!({"path": ".", "matchString": "x", "view": "noSuchView"});
    let direct = normalized_input("localSearch", json!({"queries": [query]}));
    assert_eq!(rows_run("localSearch", &direct.0), [false], "{}", direct.0);
    let sent = json!({"queries": [{"operation": "evaluate",
        "source": {"tool": "localSearch", "query": query},
        "questions": {"r": {"type": "noul", "instructions": "Is it?"}}}]});
    let (ran, notes) = normalized_input("clasify", sent);
    assert_eq!(
        ran["queries"][0]["source"]["query"]["matchString"],
        json!(["x"]),
        "{ran}"
    );
    for notes in [&direct.1, &notes] {
        assert!(
            notes
                .iter()
                .any(|note| note.contains(r#"read `matchString: "x"` as `matchString: ["x"]`."#)),
            "{notes:?}"
        );
        assert!(notes.iter().all(|note| !note.contains("send")), "{notes:?}");
    }
}
