//! What localSearch serves when a file is rewritten in place while the
//! search reads it. The search's first read returns the old, shorter
//! version whole; the writer then truncates the file and writes a longer
//! version; the search's next read, at the old length, returns the new
//! version's rest, which starts inside a token. Bytes so joined are no
//! version of the file, and the token's tail they hold matches no secret
//! pattern alone: no row may show them. Read from the call's served result.
//! The secret is fake.
use octocode_engine::search::text_search::DURING_SEARCH_READ;
use octocode_runtime_base::execution::cancel::NeverCancel;
use octocode_runtime_base::id::ToolId;
use octocode_test_support::json_fixture;
use serde_json::{Value, json};
use std::path::Path;

/// A fake GitHub token's body (after `ghp_`).
const BODY: &str = "FAKEtornTok3nQ7wX9mZ2pL4vR8sN1cY6bU3";

fn token() -> String {
    format!("ghp_{BODY}")
}

/// The version the file holds when the search opens it: no secret.
fn before() -> String {
    "needle one\nneedle xxxxxx\n".to_owned()
}

/// The version written while the search reads it: longer, its token
/// spanning the byte the first version ends at.
fn after() -> String {
    format!(
        "needle key {} needle tail\nneedle two\nneedle three\n",
        token()
    )
}

/// The query shapes a page shows rows in: whole lines (rebuilt from the
/// file), context windows and matchOnly spans (kept by the scan).
fn shapes() -> [Value; 3] {
    [
        json_fixture(r#"{"matchString": ["needle"]}"#),
        json_fixture(r#"{"matchString": ["needle"], "contextLines": {"before": 1, "after": 1}}"#),
        json_fixture(
            r#"{"matchString": ["\\S*\\s?needle \\w+"], "regexEngine": "rust", "view": "matchOnly"}"#,
        ),
    ]
}

/// One localSearch call over `path` (a file or a directory), as an agent
/// receives its result.
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
        None,
    )
    .unwrap_or_else(|error| panic!("{fields}: the page fails: {error:?}"));
    let mut data = result.to_row().expect("row");
    if result.status == super::super::SearchStatus::Empty {
        data["status"] = json!("empty");
    }
    octocode_runtime_base::test_support::staged_response(ToolId::LocalSearch, vec![data])
}

/// Registers `write` for `file`, made after each read of it the search
/// makes once the read has taken the first version whole, while it returns
/// `true`.
fn while_read(file: &Path, mut write: impl FnMut() -> bool + Send + 'static) -> std::path::PathBuf {
    let whole = before().len() as u64;
    let source = std::fs::canonicalize(file).expect("canonical source");
    DURING_SEARCH_READ
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push((
            source.clone(),
            Box::new(move |offset: u64| offset < whole || write()),
        ));
    source
}

/// Drops what is still registered for `source`; whether something was.
fn unregister(source: &Path) -> bool {
    let mut races = DURING_SEARCH_READ
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let before = races.len();
    races.retain(|(path, _)| path != source);
    races.len() != before
}

/// The first piece of the token's body, eight characters or longer, that
/// `out` serves, if any.
fn token_piece(out: &Value) -> Option<String> {
    let text = out.to_string();
    (0..=BODY.len() - 8)
        .map(|start| &BODY[start..start + 8])
        .find(|piece| text.contains(piece))
        .map(|piece| format!("a piece of the token, {piece}, is served"))
}

/// Each served row's text, joined.
fn shown(out: &Value) -> String {
    shown_of(out, "")
}

/// Each served row's text of the files whose path ends with `name`, joined.
fn shown_of(out: &Value, name: &str) -> String {
    out["results"][0]["data"]["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| {
            entry["path"]
                .as_str()
                .is_some_and(|path| path.ends_with(name))
        })
        .flat_map(|entry| entry["matches"].as_array().cloned().unwrap_or_default())
        .map(|row| row["value"].as_str().unwrap_or_default().to_owned())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Whether the row says it left a file's rows out.
fn left_out(out: &Value) -> bool {
    out["results"][0]["data"]["partialReasons"]
        .as_array()
        .is_some_and(|reasons| reasons.contains(&json!("filesSkipped")))
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

/// A search whose read a rewrite tore, once: the file is searched again,
/// and every row is served from that read, which no write moved; no row
/// shows the joined bytes, and nothing is left out.
#[test]
fn a_search_read_a_rewrite_tore_is_served_from_the_settled_read() {
    assert!(
        (11 + 4..11 + 40).contains(&before().len()),
        "the first version ends inside the second's token body"
    );
    assert_each_shape(|fields| {
        let root = tempfile::tempdir().expect("root");
        let file = root.path().join("live.txt");
        std::fs::write(&file, before()).expect("fixture");
        let (target, mut writes) = (file.clone(), 0);
        let source = while_read(&file, move || {
            writes += 1;
            let version = if writes == 1 { after() } else { before() };
            std::fs::write(&target, version).expect("rewrite in place");
            writes < 2
        });
        let out = served(&file, root.path(), fields);
        let mut broken = Vec::new();
        if unregister(&source) {
            broken.push("the search did not read the file while it was rewritten".to_owned());
        }
        broken.extend(token_piece(&out));
        if left_out(&out) {
            broken.push("a row is left out".to_owned());
        }
        if !shown(&out).contains("needle one") {
            broken.push("the rows are not the settled version's".to_owned());
        }
        (out, broken)
    });
}

/// A file rewritten after every read the search makes of it never settles:
/// its rows are left out, the row says so and names it, no row shows bytes
/// of any read of it, and the other files' rows are served.
#[test]
fn a_file_rewritten_through_every_search_read_is_left_out() {
    assert_each_shape(|fields| {
        let root = tempfile::tempdir().expect("root");
        let file = root.path().join("live.txt");
        std::fs::write(&file, before()).expect("fixture");
        std::fs::write(root.path().join("still.txt"), "needle steady\n").expect("fixture");
        let (target, mut writes) = (file.clone(), 0u64);
        let source = while_read(&file, move || {
            writes += 1;
            let version = if writes % 2 == 1 { after() } else { before() };
            std::fs::write(&target, version).expect("rewrite in place");
            true
        });
        let out = served(root.path(), root.path(), fields);
        let mut broken = Vec::new();
        if !unregister(&source) {
            broken.push("the writer was not registered".to_owned());
        }
        broken.extend(token_piece(&out));
        if !shown_of(&out, "live.txt").is_empty() {
            broken.push("a row of the file is served".to_owned());
        }
        if !shown_of(&out, "still.txt").contains("needle steady") {
            broken.push("the other file's rows are not served".to_owned());
        }
        if !left_out(&out) || !out["results"][0]["data"].to_string().contains("live.txt") {
            broken.push("the row does not say it left the file out, naming it".to_owned());
        }
        (out, broken)
    });
}

/// A search read no write moved is served as before: the token's line
/// shows its placeholder in place of the token alone, and nothing is left
/// out.
#[test]
fn a_search_read_no_write_moved_is_served_as_before() {
    assert_each_shape(|fields| {
        let root = tempfile::tempdir().expect("root");
        let file = root.path().join("still.txt");
        std::fs::write(&file, after()).expect("fixture");
        let out = served(&file, root.path(), fields);
        let mut broken = Vec::new();
        if out.to_string().contains(&token()) {
            broken.push("the token is served".to_owned());
        }
        broken.extend(token_piece(&out));
        if left_out(&out) {
            broken.push("a row is left out".to_owned());
        }
        let text = shown(&out);
        if !text.contains("needle tail")
            || octocode_runtime_base::security::placeholders(&text) == 0
        {
            broken
                .push("the token's line does not show its placeholder beside its text".to_owned());
        }
        (out, broken)
    });
}
