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

/// A direct call's repair of a query that still fails names no form; the
/// same query in a billed clasify source is not repaired at all
/// ([`super::a_billed_calls_source_query_is_validated_as_sent`]): the row is
/// refused before anything is read or sent, naming the field to fix.
#[test]
fn a_source_query_that_still_fails_says_only_what_was_read() {
    let query = json!({"path": ".", "matchString": "x", "view": "noSuchView"});
    let direct = normalized_input("localSearch", json!({"queries": [query]}));
    assert_eq!(rows_run("localSearch", &direct.0), [false], "{}", direct.0);
    assert!(
        direct
            .1
            .iter()
            .any(|note| note.contains(r#"read `matchString: "x"` as `matchString: ["x"]`."#)),
        "{:?}",
        direct.1
    );
    assert!(
        direct.1.iter().all(|note| !note.contains("send")),
        "{:?}",
        direct.1
    );
    let sent = json!({"queries": [{"operation": "evaluate",
        "source": {"tool": "localSearch", "query": query},
        "questions": {"r": {"type": "noul", "instructions": "Is it?"}}}]});
    let (ran, notes) = normalized_input("clasify", sent.clone());
    assert_eq!(ran, sent, "nothing of a billed call is repaired");
    assert!(notes.is_empty(), "{notes:?}");
    // Refused before anything is read or sent, naming the field to fix.
    let refused = match crate::contracts::prepare_rows("clasify", ran) {
        Err(error) => error,
        Ok(mut rows) => rows.remove(0).expect_err("refused as sent"),
    };
    let shown =
        crate::contracts::format_input_error("clasify", &refused, &[], &Value::Null).to_string();
    assert!(shown.contains("source.query.matchString"), "{shown}");
}
