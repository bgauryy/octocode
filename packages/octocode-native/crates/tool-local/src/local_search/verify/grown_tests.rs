//! What localSearch serves of a file written while it is searched. A live
//! log appended to after every read of it is searched as of the bytes one
//! read took, the start of one version of it: its rows are served, and the
//! row says the file grew and how much was searched. A save in place after
//! the search is never taken for such a start, and a file that keeps
//! changing is refused, saying so, with a recovery that fits. Read from the
//! call's served result. The secret is fake.
use octocode_engine::search::text_search::DURING_SEARCH_READ;
use octocode_runtime_base::execution::cancel::NeverCancel;
use octocode_runtime_base::id::ToolId;
use octocode_test_support::json_fixture;
use serde_json::{Value, json};
use std::path::Path;

/// A fake GitHub token's body (after `ghp_`).
const BODY: &str = "FAKEgrownTok3nQ7wX9mZ2pL4vR8sN1cY6bU";

/// The log's lines when it is first searched: every one a `needle` line.
fn log() -> String {
    (1..=12).map(|n| format!("needle request {n}\n")).collect()
}

/// The query shapes a page shows rows in: whole lines (rebuilt from the
/// file), context windows and matchOnly spans (kept by the scan).
fn shapes() -> [Value; 3] {
    [
        json_fixture(r#"{"matchString": ["needle"]}"#),
        json_fixture(r#"{"matchString": ["needle"], "contextLines": {"before": 1, "after": 1}}"#),
        json_fixture(
            r#"{"matchString": ["needle \\w+ \\d+"], "regexEngine": "rust", "view": "matchOnly"}"#,
        ),
    ]
}

/// One localSearch call over `path`, as an agent receives its result: a
/// refused call as its error row.
fn served(path: &Path, root: &Path, fields: &Value) -> Value {
    let (paths, security) = octocode_runtime_base::test_support::simple_policy(root);
    let mut row = fields.clone();
    row["path"] = json!(path);
    let query = serde_json::from_value(row).expect("query");
    let data = match super::super::execute_local_search(
        octocode_runtime_base::test_support::BUILD_IDENTITY,
        &query,
        &paths,
        &security,
        &NeverCancel,
        None,
        None,
    ) {
        Ok(result) => {
            let mut data = result.to_row().expect("row");
            if result.status == super::super::SearchStatus::Empty {
                data["status"] = json!("empty");
            }
            data
        }
        Err(error) => octocode_runtime_base::response::outcome::DomainResult::from(error).data,
    };
    octocode_runtime_base::test_support::staged_response(ToolId::LocalSearch, vec![data])
}

/// Appends `text` to the file at `path`, as a log writer does.
fn append(path: &Path, text: &str) {
    use std::io::Write as _;
    std::fs::OpenOptions::new()
        .append(true)
        .open(path)
        .and_then(|mut file| file.write_all(text.as_bytes()))
        .expect("append");
}

/// Registers `write` for the file at `path`, made after each read of it
/// ends and before the read is proved, while it returns `true`; returns the
/// inode it is registered under.
#[cfg(unix)]
fn after_each_read(path: &Path, write: impl FnMut() -> bool + Send + 'static) -> u64 {
    use std::os::unix::fs::MetadataExt as _;
    let inode = std::fs::metadata(path).expect("metadata").ino();
    octocode_engine::text::BEFORE_PROOF
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push((inode, Box::new(write)));
    inode
}

/// Drops what is still registered for `inode`.
#[cfg(unix)]
fn unregister_read(inode: u64) {
    octocode_engine::text::BEFORE_PROOF
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .retain(|(registered, _)| *registered != inode);
}

/// Each served row's text, joined.
fn shown(out: &Value) -> String {
    out["results"][0]["data"]["files"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|entry| entry["matches"].as_array().cloned().unwrap_or_default())
        .map(|row| row["value"].as_str().unwrap_or_default().to_owned())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The row's warnings, joined.
fn warnings(out: &Value) -> String {
    out["results"][0]["data"]["warnings"].to_string()
}

/// Fails with every shape's broken rules: `check` names what one served
/// result breaks.
fn assert_each_shape(mut check: impl FnMut(&Value) -> (Value, Vec<String>)) {
    let broken = shapes()
        .iter()
        .flat_map(|fields| {
            let (out, broken) = check(fields);
            broken
                .into_iter()
                .map(move |rule| format!("{fields}: {rule}: {out}"))
        })
        .collect::<Vec<_>>();
    assert!(broken.is_empty(), "{}", broken.join("\n\n"));
}

/// A log appended to after every read of it, the search's and each read
/// after it, never holds still for a read, but each read is the start of one
/// version: its rows are served, the first and last line it held among them,
/// none of what was written after, and the row says the file grew and how
/// many bytes were searched.
#[cfg(unix)]
#[test]
fn a_log_appended_after_every_read_is_searched_as_of_its_read() {
    assert_each_shape(|fields| {
        let root = tempfile::tempdir().expect("root");
        let file = root.path().join("live.log");
        std::fs::write(&file, log()).expect("fixture");
        let target = file.clone();
        let inode = after_each_read(&file, move || {
            append(&target, "needle appended 99\n");
            true
        });
        let out = served(&file, root.path(), fields);
        unregister_read(inode);
        let data = &out["results"][0]["data"];
        let mut broken = Vec::new();
        if !data["errorCode"].is_null() {
            broken.push("the log is refused".to_owned());
        }
        let text = shown(&out);
        if !text.contains("needle request 1") || !text.contains("needle request 12") {
            broken.push("the rows are not the lines of the log as read".to_owned());
        }
        if text.contains("appended") {
            broken.push("a row shows text written after the read".to_owned());
        }
        let warnings = warnings(&out);
        if !warnings.contains("live.log")
            || !warnings.contains(&format!("as of {} bytes", log().len()))
        {
            broken.push("the row does not say the file grew and how much was searched".to_owned());
        }
        (out, broken)
    });
}

/// A save in place after the search read the file, longer and starting with
/// other bytes, is no version the read is the start of: the file is
/// searched again, every row is that settled version's, its token shown as
/// its placeholder, and nothing says the file grew.
#[cfg(unix)]
#[test]
fn a_save_in_place_after_the_search_is_not_taken_for_its_start() {
    assert_each_shape(|fields| {
        let root = tempfile::tempdir().expect("root");
        let file = root.path().join("live.txt");
        std::fs::write(&file, "needle first 1\n").expect("fixture");
        let target = file.clone();
        let inode = after_each_read(&file, move || {
            std::fs::write(
                &target,
                format!("needle second 1 ghp_{BODY}\nneedle second 2\n"),
            )
            .expect("rewrite in place");
            false
        });
        let out = served(&file, root.path(), fields);
        unregister_read(inode);
        let mut broken = Vec::new();
        let text = shown(&out);
        if text.contains("needle first") || !text.contains("needle second 2") {
            broken.push("the rows are not the settled version's".to_owned());
        }
        if out.to_string().contains(BODY) {
            broken.push("the token is served".to_owned());
        }
        if warnings(&out).contains("grewWhileRead") {
            broken.push("a rewrite is said to be growth".to_owned());
        }
        (out, broken)
    });
}

/// A file rewritten after every read the search makes of it never settles:
/// the call is refused, the error says the file kept changing while it was
/// read, and the recovery is to wait or search a still copy, not to check
/// read access.
#[test]
fn a_file_that_keeps_changing_is_refused_saying_so() {
    let root = tempfile::tempdir().expect("root");
    let file = root.path().join("live.txt");
    let versions = ["needle one\n", "needle two, a longer version\n"];
    std::fs::write(&file, versions[0]).expect("fixture");
    let source = std::fs::canonicalize(&file).expect("canonical source");
    let (target, mut writes) = (file.clone(), 0usize);
    DURING_SEARCH_READ
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push((
            source.clone(),
            Box::new(move |_| {
                writes += 1;
                std::fs::write(&target, versions[writes % 2]).expect("rewrite in place");
                true
            }),
        ));
    let out = served(
        &file,
        root.path(),
        &json_fixture(r#"{"matchString": ["needle"]}"#),
    );
    DURING_SEARCH_READ
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .retain(|(path, _)| *path != source);
    let data = &out["results"][0]["data"];
    assert!(
        data["error"]
            .as_str()
            .is_some_and(|error| error.contains("kept changing while it was read")),
        "the error names why: {out}"
    );
    let hints = data["hints"].to_string();
    assert!(
        hints.contains("stops changing") && !hints.contains("read access"),
        "the recovery fits a file that keeps changing: {out}"
    );
}
