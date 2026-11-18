//! Reader rows and the private progress an issued cursor restored beside
//! them; read leads narrowed to deciding lines.
use super::key::Keying;
use super::*;

/// The `listed` state of a ghSearchCode walk that listed `files` files: one
/// 16-hex digest each, as the walk writes it.
fn listed(files: usize) -> String {
    (0..files).map(|file| format!("{file:016x}")).collect()
}

/// A walk past 625 files carries a `listed` longer than any caller string
/// the input gate admits. Restored beside the public row, it is the runtime's
/// own progress: the row passes the gate with it as written.
#[test]
fn issued_progress_past_625_files_passes_the_gate_as_written() {
    let reader = Keying::new();
    let row = json!({"keywords":["useState"],"owner":"vercel","repo":"next.js"});
    let state = listed(626);
    assert!(state.len() > 10_000, "past the gate's string bound");
    let progress = json!({"listed": state, "page": 2});
    let checked = prepare_row(ToolId::GhSearchCode, &row, progress.as_object(), &reader)
        .expect("restored progress passes the gate");
    assert_eq!(checked["listed"], json!(state));
    assert_eq!(checked["page"], json!(2));
    // The same state written into the row itself is caller text: refused.
    let mut written = row.clone();
    written["listed"] = json!(state);
    assert!(prepare_source(ToolId::GhSearchCode, &written, &reader).is_err());
}

/// Progress names only the reader's private fields: any other field beside
/// the row is no progress and never joins it.
#[test]
fn only_the_readers_private_fields_join_the_row() {
    let reader = Keying::new();
    let row = json!({"keywords":["useState"],"owner":"vercel","repo":"next.js"});
    let progress = json!({"page": 2, "owner": "someone-else"});
    let checked =
        prepare_row(ToolId::GhSearchCode, &row, progress.as_object(), &reader).expect("prepared");
    assert_eq!(checked["owner"], json!("vercel"));
    assert_eq!(checked["page"], json!(2));
}

/// A whole read narrows to the deciding lines: the same reader, path and
/// pin, only its ranges changed; no lines, or no single reader row, has no
/// narrower read.
#[test]
fn narrowed_read_keeps_reader_path_and_pin() {
    let pin = "a1".repeat(32);
    let whole = read_lead(
        ToolId::LocalFetch,
        &json!({"path":"docs/guide.md","ranges":[{"startLine":1,"endLine":220}]}),
        Some(&pin),
    );
    let narrowed = narrowed_read(&whole, &[(60, 61), (90, 95)]).expect("narrowed");
    let (row, saved) = (
        &narrowed["input"]["queries"][0],
        &whole["input"]["queries"][0],
    );
    assert_eq!(
        row["ranges"],
        json!([{"startLine":60,"endLine":61},{"startLine":90,"endLine":95}])
    );
    assert_eq!(narrowed["kind"], whole["kind"]);
    assert_eq!(narrowed["tool"], whole["tool"]);
    assert_eq!(
        (&row["path"], &row["snapshot"]),
        (&saved["path"], &saved["snapshot"])
    );
    assert!(narrowed_read(&whole, &[]).is_none());
    assert!(narrowed_read(&json!({"input":{"cursor":"c"}}), &[(1, 2)]).is_none());
}
