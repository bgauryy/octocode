//! What localSearch serves an agent, read from the call's result: no value
//! leaves raw, and a result is flagged `redacted: true` exactly when the
//! text it shows holds a placeholder. A source rewritten between a page's
//! rebuild of its values and the secret check's read of it is the hard case:
//! the check's judgment of the new bytes says nothing of values made from the
//! old ones. All secrets are fake.
use crate::runtime::{Channel, ToolRuntime};
use octocode_runtime_base::config::{ConfigInput, FileInput, RuntimeSurface};
use octocode_runtime_base::execution::RuntimeLimits;
use octocode_test_support::json_fixture;
use serde_json::{Value, json};
use std::path::Path;

const TOKEN: &str = "ghp_FAKEchurnTok3n0000000000000000000abc";

/// Eight lines, the seventh holding the token.
fn version_a() -> String {
    (1..=8)
        .map(|n| match n {
            7 => format!("needle line {n} {TOKEN}\n"),
            _ => format!("needle line {n}\n"),
        })
        .collect()
}

/// Eleven lines, the third holding the token.
fn version_b() -> String {
    (1..=11)
        .map(|n| match n {
            3 => format!("needle line {n} pad {TOKEN}\n"),
            _ => format!("needle line {n} pad\n"),
        })
        .collect()
}

fn runtime(home: &Path, cwd: &Path) -> ToolRuntime {
    let env = std::collections::BTreeMap::from([(
        "OCTOCODE_HOME".to_owned(),
        home.to_string_lossy().into_owned(),
    )]);
    ToolRuntime::with_limits(
        ConfigInput {
            env,
            cwd: cwd.to_path_buf(),
            os_home: home.to_path_buf(),
            global_env: FileInput::Missing {
                path: home.join(".env"),
            },
            project_env: FileInput::Missing {
                path: cwd.join(".octocode/.env"),
            },
            config_file: FileInput::Missing {
                path: home.join(".octocoderc"),
            },
            project_config_file: FileInput::Missing {
                path: cwd.join(".octocode/.octocoderc"),
            },
            runtime_surface: RuntimeSurface::Mcp,
        },
        RuntimeLimits::DEFAULT,
    )
    .expect("runtime")
}

/// One localSearch call with `input`, as an agent receives its result.
async fn call(runtime: &ToolRuntime, input: Value) -> Value {
    let admission = runtime.admit("served".into()).expect("admitted");
    runtime
        .execute(admission, "localSearch".into(), input, Channel::Mcp)
        .await
        .expect("call")
        .structured_content
        .into_value()
}

/// One localSearch call over `scanned` (the bytes the scan reads and the
/// page rebuilds values from), with `checked` written in their place before
/// the secret check reads the file.
async fn raced(scanned: &str, checked: &str, fields: Value) -> Value {
    let home = tempfile::tempdir().expect("home");
    let root = tempfile::tempdir().expect("root");
    let file = root.path().join("live.txt");
    std::fs::write(&file, scanned).expect("fixture");
    let source = std::fs::canonicalize(&file).expect("canonical source");
    let (target, checked) = (file.clone(), checked.to_owned());
    octocode_tool_local::local_search::verify::BEFORE_CHECK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push((
            source.clone(),
            Box::new(move || std::fs::write(&target, checked).expect("rewrite")),
        ));
    let runtime = runtime(home.path(), root.path());
    let mut row = fields;
    row["path"] = json!(file);
    let out = call(&runtime, json!({ "queries": [row] })).await;
    runtime.close().await;
    let raced = !octocode_tool_local::local_search::verify::BEFORE_CHECK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .any(|(path, _)| *path == source);
    assert!(raced, "the file was rewritten before the check read it");
    out
}

/// The result shows no raw token, and is flagged exactly when its rows show
/// a placeholder; returns whether they do.
fn assert_served_safely(out: &Value) -> bool {
    let text = out.to_string();
    assert!(!text.contains(TOKEN), "a raw secret is served: {text}");
    let data = &out["results"][0]["data"];
    let shows = octocode_runtime_base::security::placeholders_in(&data["files"]) > 0;
    assert_eq!(
        data["redacted"] == json!(true),
        shows,
        "the flag follows the shown text: {text}"
    );
    shows
}

/// Single-line rows whose values were rebuilt from the bytes the scan read:
/// a check of other bytes judges none of them, in either direction of the
/// rewrite.
#[tokio::test]
async fn a_value_rebuilt_from_other_bytes_than_the_check_read_is_never_served_raw() {
    let query = json_fixture(r#"{"matchString": ["needle line 7", "needle line 3"]}"#);
    for (scanned, checked) in [(version_a(), version_b()), (version_b(), version_a())] {
        let out = raced(&scanned, &checked, query.clone()).await;
        assert!(assert_served_safely(&out), "{out}");
    }
}

/// Context rows cut from the text the scan searched, which already shows the
/// token's placeholder: a check of other bytes whose lines hold no secret
/// there proves nothing of them, and the placeholder they show is flagged.
#[tokio::test]
async fn a_context_window_that_shows_a_placeholder_over_other_bytes_is_flagged() {
    let query = json_fixture(
        r#"{
            "matchString": ["needle line 7"],
            "contextLines": {"before": 1, "after": 1}
        }"#,
    );
    for (scanned, checked) in [(version_a(), version_b()), (version_b(), version_a())] {
        let out = raced(&scanned, &checked, query.clone()).await;
        assert_served_safely(&out);
    }
}

/// Pages of merged context windows, each cut to the lines its own hits hold:
/// a page is flagged by the text it shows, not by a window it trimmed.
#[tokio::test]
async fn each_page_of_context_windows_is_flagged_by_the_text_it_shows() {
    let home = tempfile::tempdir().expect("home");
    let root = tempfile::tempdir().expect("root");
    let file = root.path().join("f.txt");
    let body = (1..=20)
        .map(|n| match n {
            7 => format!("needle line {n} {TOKEN}\n"),
            _ => format!("needle line {n}\n"),
        })
        .collect::<String>();
    std::fs::write(&file, body).expect("fixture");
    let runtime = runtime(home.path(), root.path());
    let mut input = json!({"queries": [{
        "path": file,
        "matchString": ["needle"],
        "contextLines": {"before": 2, "after": 2},
        "limit": 2
    }]});
    let (mut pages, mut shown) = (0, 0);
    loop {
        let out = call(&runtime, input).await;
        shown += usize::from(assert_served_safely(&out));
        pages += 1;
        assert!(pages <= 20, "the walk ends");
        match &out["next"]["input"] {
            next @ Value::Object(_) => input = next.clone(),
            _ => break,
        }
    }
    runtime.close().await;
    assert!(shown > 0 && shown < pages, "{shown} of {pages} pages");
}

/// Registers `write` for the canonical `file` in `races`: made once, at the
/// stage those races are made.
fn race_at(
    races: &std::sync::Mutex<Vec<octocode_tool_local::local_search::verify::CheckRace>>,
    file: &Path,
    write: impl FnOnce() + Send + 'static,
) {
    let source = std::fs::canonicalize(file).expect("canonical source");
    races
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push((source, Box::new(write)));
}

/// Whether a race registered for the canonical `file` in `races` is still
/// waiting to be made.
fn waiting(
    races: &std::sync::Mutex<Vec<octocode_tool_local::local_search::verify::CheckRace>>,
    file: &Path,
) -> bool {
    let source = std::fs::canonicalize(file).expect("canonical source");
    races
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .any(|(path, _)| *path == source)
}

/// Six lines that each match `needle`, the second a private key's armor
/// line: its rows are rebuilt by the key-block redaction, which reads the
/// file again.
fn armored(word: &str) -> String {
    (1..=6)
        .map(|n| match n {
            2 => "needle -----BEGIN RSA PRIVATE KEY-----\n".to_owned(),
            _ => format!("needle {word} {n}\n"),
        })
        .collect()
}

/// A file rewritten in place after the scan matched it and before a page
/// rebuilt its values (single-line rows, whose values a page rebuilds from
/// the file), searched as the file and in its directory: each row the
/// rebuild withheld still shows text (the line as the file now reads, judged
/// alone), never an empty value.
#[tokio::test]
async fn a_row_withheld_while_its_values_are_rebuilt_keeps_its_text() {
    for in_directory in [false, true] {
        let home = tempfile::tempdir().expect("home");
        let root = tempfile::tempdir().expect("root");
        let file = root.path().join("live.txt");
        std::fs::write(&file, armored("line")).expect("fixture");
        let target = file.clone();
        race_at(
            &octocode_tool_local::local_search::verify::lines::BEFORE_REREAD,
            &file,
            move || {
                std::fs::write(&target, armored("row")).expect("rewrite");
            },
        );
        let runtime = runtime(home.path(), root.path());
        let query = json_fixture(r#"{"matchString": ["needle"]}"#);
        let mut row = query.clone();
        row["path"] = if in_directory {
            json!(root.path())
        } else {
            json!(file)
        };
        let out = call(&runtime, json!({ "queries": [row] })).await;
        runtime.close().await;
        assert!(
            !waiting(
                &octocode_tool_local::local_search::verify::lines::BEFORE_REREAD,
                &file
            ),
            "the file was rewritten before its values were rebuilt"
        );
        assert_served_safely(&out);
        let rows = out["results"][0]["data"]["files"][0]["matches"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        assert_eq!(rows.len(), 6, "{query}: {out}");
        for row in &rows {
            let value = row["value"].as_str().unwrap_or_default();
            assert!(!value.is_empty(), "{query}: a row lost its text: {out}");
        }
    }
}

/// A file rewritten between the scan and a page's rebuild of its values,
/// then written back before the page's secret check: the check judges the
/// scanned bytes whole, but the rows the rebuild withheld show other text,
/// which that judgment never cut. The output gate takes none of the file's
/// rows as cuts of it.
#[test]
fn rows_no_judgment_cut_are_never_marked_as_its_cuts() {
    let dir = tempfile::tempdir().expect("fixture");
    let (paths, security) = octocode_runtime_base::test_support::simple_policy(dir.path());
    let file = dir.path().join("live.txt");
    let scanned = (1..=4)
        .map(|n| format!("needle line {n}\n"))
        .collect::<String>();
    std::fs::write(&file, &scanned).expect("fixture");
    let target = file.clone();
    race_at(
        &octocode_tool_local::local_search::verify::lines::BEFORE_REREAD,
        &file,
        move || {
            std::fs::write(
                &target,
                "needle other 1\nneedle other 2\nneedle other 3\nneedle other 4\n",
            )
            .expect("rewrite");
        },
    );
    let target = file.clone();
    race_at(
        &octocode_tool_local::local_search::verify::BEFORE_CHECK,
        &file,
        move || {
            std::fs::write(&target, scanned).expect("write back");
        },
    );
    let query: octocode_contract_types::LocalSearchQuery = serde_json::from_value(json!({
        "path": dir.path(), "matchString": ["needle"]
    }))
    .expect("query");
    let (security, judged) = security.for_request();
    let result = octocode_tool_local::local_search::execute_local_search(
        octocode_runtime_base::test_support::BUILD_IDENTITY,
        &query,
        &paths,
        &security,
        &octocode_runtime_base::execution::cancel::NeverCancel,
        None,
    )
    .expect("search");
    assert!(!waiting(
        &octocode_tool_local::local_search::verify::lines::BEFORE_REREAD,
        &file
    ));
    assert!(!waiting(
        &octocode_tool_local::local_search::verify::BEFORE_CHECK,
        &file
    ));
    let canonical = std::fs::canonicalize(&file).expect("canonical");
    assert!(!security.judged_whole(&canonical));
    let shown = result
        .files
        .iter()
        .flat_map(|file| file.matches.iter().flatten())
        .map(|row| row.value.clone())
        .collect::<Vec<_>>();
    assert!(
        shown.iter().any(|value| value.contains("other")),
        "rows withheld from the rewrite are shown: {shown:?}"
    );
    assert!(
        judged.leaves().is_empty(),
        "no row is marked as a cut of the judgment: {shown:?}"
    );
}

/// Context windows, a page of one window, and matchOnly spans over a file
/// rewritten in place between a page's rebuild of its values and its secret
/// check, in both directions: no value leaves raw, the result is flagged
/// exactly when it shows a placeholder, no row loses its text, and a span
/// shows its secret's placeholder in place of the secret alone, never in
/// place of the whole span.
#[tokio::test]
async fn windows_and_spans_over_an_in_place_rewrite_are_served_safely() {
    let terms = json_fixture(r#"["needle line 7", "needle line 3"]"#);
    for fields in [
        json!({"matchString": terms, "contextLines": {"before": 2, "after": 2}}),
        json!({"matchString": terms, "contextLines": {"before": 1, "after": 1}, "limit": 1}),
        json_fixture(
            r#"{
                "matchString": ["needle line [37].*"],
                "regexEngine": "rust",
                "view": "matchOnly"
            }"#,
        ),
    ] {
        for (scanned, checked) in [(version_a(), version_b()), (version_b(), version_a())] {
            let out = raced(&scanned, &checked, fields.clone()).await;
            assert_served_safely(&out);
            let rows = out["results"][0]["data"]["files"][0]["matches"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            assert!(!rows.is_empty(), "{fields}: {out}");
            for row in &rows {
                let value = row["value"].as_str().unwrap_or_default();
                let lines = value
                    .split('\n')
                    .map(|line| line.split_once('\t').map_or(line, |(_, text)| text));
                for line in lines {
                    assert!(!line.is_empty(), "{fields}: a row lost its text: {out}");
                    assert!(
                        !line.starts_with("[REDACTED"),
                        "{fields}: a placeholder stands for a whole line or span: {out}"
                    );
                }
            }
        }
    }
}
