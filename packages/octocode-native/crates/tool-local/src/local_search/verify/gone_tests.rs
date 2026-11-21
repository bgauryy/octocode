//! What localSearch serves when a matched file is gone, or holds fewer lines,
//! by the time the page that shows its rows reads it, as an editor's atomic
//! save (a temporary file renamed over the target) or a save in place leaves
//! it: the rest of the page is served, no row shows text no read gave it,
//! and the row says what it left out. Read from the call's served result.
use octocode_runtime_base::execution::cancel::NeverCancel;
use octocode_runtime_base::id::ToolId;
use octocode_test_support::json_fixture;
use serde_json::{Value, json};
use std::path::Path;

/// Eight lines, each a `needle` line.
fn eight_lines() -> String {
    (1..=8).map(|n| format!("needle line {n}\n")).collect()
}

/// One localSearch call over `path`, as an agent receives its result: the
/// row staged as the runtime stages it (an empty one says so).
fn served(path: &Path, root: &Path, fields: &Value) -> Value {
    let (paths, security) = octocode_runtime_base::test_support::simple_policy(root);
    let mut row = fields.clone();
    row["path"] = json!(path);
    let query = serde_json::from_value(row).expect("query");
    let result = super::super::execute_local_search(
        octocode_runtime_base::test_support::BUILD_IDENTITY,
        &query,
        &paths,
        &security,
        &NeverCancel,
        None,
    )
    .unwrap_or_else(|error| panic!("{fields}: the page fails: {error:?}"));
    let mut data = result.to_row().expect("row");
    if result.status == super::super::SearchStatus::Empty {
        data["status"] = json!("empty");
    }
    octocode_runtime_base::test_support::staged_response(ToolId::LocalSearch, vec![data])
}

/// Registers `write` for the canonical `file` in `races`.
fn race_at(
    races: &std::sync::Mutex<Vec<super::CheckRace>>,
    file: &Path,
    write: impl FnOnce() + Send + 'static,
) {
    let source = std::fs::canonicalize(file).expect("canonical source");
    races
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push((source, Box::new(write)));
}

/// Whether a race registered for `file` (canonical `source`) in `races` is
/// still waiting; drops it, so a race no call made never fires later.
fn waiting(races: &std::sync::Mutex<Vec<super::CheckRace>>, source: &Path) -> bool {
    let mut races = races
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let before = races.len();
    races.retain(|(path, _)| path != source);
    races.len() != before
}

/// Each served row of `file` (by its file name): its value.
fn values_of(out: &Value, file: &str) -> Vec<String> {
    out["results"][0]["data"]["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| {
            entry["path"]
                .as_str()
                .is_some_and(|path| path.ends_with(file))
        })
        .flat_map(|entry| entry["matches"].as_array().cloned().unwrap_or_default())
        .map(|row| row["value"].as_str().unwrap_or_default().to_owned())
        .collect()
}

/// Every served row's text, each line of it, is text a read gave it.
fn assert_no_empty_text(out: &Value) {
    let rows = out["results"][0]["data"]["files"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|entry| entry["matches"].as_array().cloned().unwrap_or_default());
    for row in rows {
        let value = row["value"].as_str().unwrap_or_default();
        for line in value.split('\n') {
            let text = line.split_once('\t').map_or(line, |(_, text)| text);
            assert!(!text.is_empty(), "a row shows no text: {out}");
        }
    }
}

/// The row says a file's rows were left out, naming that file.
fn assert_left_out(out: &Value, file: &str) {
    let data = &out["results"][0]["data"];
    let reasons = data["partialReasons"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        reasons.contains(&json!("filesSkipped")),
        "the row says it left a file's rows out: {out}"
    );
    assert!(
        data.to_string().contains(file),
        "the row names the file it left out: {out}"
    );
}

/// The query shapes a page shows rows in: whole lines (rebuilt from the
/// file), context windows (kept by the scan) and matchOnly spans.
fn shapes() -> [Value; 3] {
    let terms = json_fixture(r#"["needle line 7", "needle line 3"]"#);
    [
        json!({"matchString": terms}),
        json!({"matchString": terms, "contextLines": {"before": 1, "after": 1}}),
        json_fixture(
            r#"{
                "matchString": ["needle line [37]"],
                "regexEngine": "rust",
                "view": "matchOnly"
            }"#,
        ),
    ]
}

/// A matched file removed between the scan and the page (an atomic save's
/// temporary file, renamed away): the folder query is served, with the other
/// file's rows, none of the removed file's, and the removal said.
#[test]
fn a_matched_file_removed_before_the_page_leaves_only_its_own_rows_out() {
    for fields in shapes() {
        let dir = tempfile::tempdir().expect("fixture");
        let root = dir.path().canonicalize().expect("real root");
        std::fs::write(root.join("kept.txt"), eight_lines()).expect("fixture");
        let gone = root.join("live.txt.new");
        std::fs::write(&gone, eight_lines()).expect("fixture");
        let target = gone.clone();
        race_at(&super::BEFORE_VALIDATE, &gone, move || {
            std::fs::remove_file(&target).expect("remove");
        });
        let out = served(&root, &root, &fields);
        assert!(
            !waiting(&super::BEFORE_VALIDATE, &gone),
            "{fields}: the file was removed before the page read it"
        );
        assert!(!gone.exists());
        assert_eq!(values_of(&out, "kept.txt").len(), 2, "{fields}: {out}");
        assert!(
            values_of(&out, "live.txt.new").is_empty(),
            "{fields}: no row of the removed file is served: {out}"
        );
        assert_no_empty_text(&out);
        assert_left_out(&out, "live.txt.new");
    }
}

/// A matched file removed after the page checked it and before it rebuilt the
/// values it shows from the file: none of its rows is served without text.
#[test]
fn a_matched_file_removed_while_the_page_rebuilds_its_rows_shows_none_of_them() {
    let dir = tempfile::tempdir().expect("fixture");
    let root = dir.path().canonicalize().expect("real root");
    std::fs::write(root.join("kept.txt"), eight_lines()).expect("fixture");
    let gone = root.join("live.txt.new");
    std::fs::write(&gone, eight_lines()).expect("fixture");
    let source = gone.clone();
    let target = gone.clone();
    race_at(&super::lines::BEFORE_REREAD, &gone, move || {
        std::fs::remove_file(&target).expect("remove");
    });
    let fields = json_fixture(r#"{"matchString": ["needle line 7", "needle line 3"]}"#);
    let out = served(&root, &root, &fields);
    assert!(
        !waiting(&super::lines::BEFORE_REREAD, &source),
        "the file was removed before the page rebuilt its rows"
    );
    assert_eq!(values_of(&out, "kept.txt").len(), 2, "{out}");
    assert!(values_of(&out, "live.txt.new").is_empty(), "{out}");
    assert_no_empty_text(&out);
    assert_left_out(&out, "live.txt.new");
}

/// A file rewritten shorter between the scan and the page's rebuild of its
/// rows, searched as the file and in its folder: the row whose line it still
/// holds shows that line as the file now reads; the row whose line it no
/// longer holds is left out, never served without text, and the row says so.
#[test]
fn a_row_whose_line_the_rewritten_file_no_longer_holds_is_left_out() {
    for in_folder in [false, true] {
        let dir = tempfile::tempdir().expect("fixture");
        let root = dir.path().canonicalize().expect("real root");
        let file = root.join("live.txt");
        std::fs::write(&file, eight_lines()).expect("fixture");
        let target = file.clone();
        race_at(&super::lines::BEFORE_REREAD, &file, move || {
            let short = (1..=4)
                .map(|n| format!("needle line {n} short\n"))
                .collect::<String>();
            std::fs::write(&target, short).expect("rewrite");
        });
        let path = if in_folder {
            root.clone()
        } else {
            file.clone()
        };
        let fields = json_fixture(r#"{"matchString": ["needle line 7", "needle line 3"]}"#);
        let out = served(&path, &root, &fields);
        assert!(
            !waiting(&super::lines::BEFORE_REREAD, &file),
            "the file was rewritten before the page rebuilt its rows"
        );
        assert_no_empty_text(&out);
        let shown = values_of(&out, "live.txt");
        assert_eq!(shown, ["needle line 3 short"], "{out}");
        assert_left_out(&out, "live.txt");
    }
}

/// A file rewritten shorter before the page rebuilds its rows and whole again
/// before the page's secret check reads it: the row the rebuild did not reach
/// shows its line as the check's read holds it, and nothing is left out.
#[test]
fn a_row_the_rebuild_did_not_reach_shows_its_line_from_the_check_read() {
    for in_folder in [false, true] {
        let dir = tempfile::tempdir().expect("fixture");
        let root = dir.path().canonicalize().expect("real root");
        let file = root.join("live.txt");
        std::fs::write(&file, eight_lines()).expect("fixture");
        let target = file.clone();
        race_at(&super::lines::BEFORE_REREAD, &file, move || {
            let short = (1..=4)
                .map(|n| format!("needle line {n} short\n"))
                .collect::<String>();
            std::fs::write(&target, short).expect("rewrite");
        });
        let target = file.clone();
        race_at(&super::BEFORE_CHECK, &file, move || {
            let whole = (1..=8)
                .map(|n| format!("needle line {n} again\n"))
                .collect::<String>();
            std::fs::write(&target, whole).expect("rewrite");
        });
        let path = if in_folder {
            root.clone()
        } else {
            file.clone()
        };
        let fields = json_fixture(r#"{"matchString": ["needle line 7", "needle line 3"]}"#);
        let out = served(&path, &root, &fields);
        let source = std::fs::canonicalize(&file).expect("canonical");
        assert!(!waiting(&super::lines::BEFORE_REREAD, &source));
        assert!(!waiting(&super::BEFORE_CHECK, &source));
        assert_no_empty_text(&out);
        assert_eq!(
            values_of(&out, "live.txt"),
            ["needle line 3 short", "needle line 7 again"],
            "{out}"
        );
        assert!(
            out["results"][0]["data"].get("fileErrors").is_none(),
            "nothing is left out: {out}"
        );
    }
}

/// A matched file the policy no longer admits when the page checks it (here a
/// link out of the allowed root put in its place) fails the call on every
/// view, the listing views that read no file included.
#[cfg(unix)]
#[test]
fn a_matched_file_the_policy_no_longer_admits_fails_the_page_on_every_view() {
    let mut views = shapes().to_vec();
    views.push(json!({"matchString": ["needle line 7"], "view": "files"}));
    views.push(json!({"matchString": ["needle line 7"], "view": "countMatches"}));
    for fields in views {
        let dir = tempfile::tempdir().expect("fixture");
        let root = dir.path().join("root");
        std::fs::create_dir(&root).expect("root");
        let root = root.canonicalize().expect("real root");
        let outside = dir.path().join("outside.txt");
        std::fs::write(&outside, eight_lines()).expect("fixture");
        let target = root.join("swapped.txt");
        std::fs::write(&target, eight_lines()).expect("fixture");
        let (link, to) = (target.clone(), outside.clone());
        let key = std::fs::canonicalize(&target).expect("canonical");
        race_at(&super::BEFORE_VALIDATE, &target, move || {
            std::fs::remove_file(&link).expect("remove");
            std::os::unix::fs::symlink(&to, &link).expect("link");
        });
        let (paths, security) = octocode_runtime_base::test_support::simple_policy(&root);
        let mut row = fields.clone();
        row["path"] = json!(&root);
        let query = serde_json::from_value(row).expect("query");
        let result = super::super::execute_local_search(
            octocode_runtime_base::test_support::BUILD_IDENTITY,
            &query,
            &paths,
            &security,
            &NeverCancel,
            None,
        );
        assert!(
            result.is_err(),
            "{fields}: a file swapped for a link out of the root is no page"
        );
        assert!(
            !waiting(&super::BEFORE_VALIDATE, &key),
            "{fields}: the page checked the file after the swap"
        );
    }
}

/// A matched file removed before the page leaves no entry behind, and the
/// row's totals count only the files it can still show: the kept file's
/// two hits, as a search of the kept file alone counts them.
#[test]
fn a_removed_file_is_neither_listed_nor_counted() {
    for fields in shapes() {
        let dir = tempfile::tempdir().expect("fixture");
        let root = dir.path().canonicalize().expect("real root");
        std::fs::write(root.join("kept.txt"), eight_lines()).expect("fixture");
        let alone = served(&root.join("kept.txt"), &root, &fields);
        let gone = root.join("live.txt.new");
        std::fs::write(&gone, eight_lines()).expect("fixture");
        let target = gone.clone();
        race_at(&super::BEFORE_VALIDATE, &gone, move || {
            std::fs::remove_file(&target).expect("remove");
        });
        let out = served(&root, &root, &fields);
        assert!(
            !waiting(&super::BEFORE_VALIDATE, &gone),
            "{fields}: the file was removed before the page read it"
        );
        let data = &out["results"][0]["data"];
        let listed = data["files"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|file| file["path"].as_str().unwrap_or_default().to_owned())
            .collect::<Vec<_>>();
        assert!(
            listed.iter().all(|path| !path.ends_with("live.txt.new")),
            "{fields}: the removed file is no entry: {out}"
        );
        assert_left_out(&out, "live.txt.new");
        let alone = &alone["results"][0]["data"];
        for count in ["matchCount", "matchedLineCount"] {
            assert_eq!(data.get(count), alone.get(count), "{fields} {count}: {out}");
        }
    }
}
