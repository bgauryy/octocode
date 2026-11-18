//! Guessed fields, checked against the guess table and the embedded contract,
//! then one case per reader that reads by value shape.
use super::*;
use crate::contracts::normalize::guesses::{GUESSES, Guess, Read, SPELLINGS};
use crate::contracts::schema_facts::{child_schemas, property_names, resolve_ref};
use crate::id::ToolId;
use octocode_test_support::json_fixture;

/// What a call with `sent` as its one row runs as: the repaired row, the
/// notes, and the error and its details when refused, of the request as the
/// runtime validates it.
fn run(tool: &str, sent: Value) -> (Value, Vec<String>, Option<String>) {
    let (input, notes) = normalized_input(tool, json!({"queries":[sent]}));
    let refused = match prepare_rows(tool, input.clone()) {
        Err(error) => Some(error),
        Ok(mut rows) => rows.remove(0).err(),
    };
    let shown = refused.map(|error| {
        let shown = crate::contracts::format_input_error(tool, &error, &[], &input);
        std::iter::once(&shown["error"])
            .chain(shown["details"].as_array().into_iter().flatten())
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join("\n")
    });
    (input["queries"][0].clone(), notes, shown)
}

/// Where a guess sits: the row field whose object holds it (empty for the
/// row), the declared field it reads as and the name sent.
type At = (&'static str, &'static str, &'static str);

fn at(guess: &Guess) -> At {
    (guess.within, guess.field, guess.sent)
}

/// `field` as a note names it: inside `within`'s object, or the row's.
fn label(within: &str, field: &str) -> String {
    match within {
        "" => field.to_owned(),
        within => format!("{within}.{field}"),
    }
}

/// The object of `row` holding fields in `within` (the row when empty).
fn held<'a>(row: &'a Value, within: &str) -> &'a Value {
    match within {
        "" => row,
        within => &row[within],
    }
}

/// What [`sending`] builds a guess's row from.
struct Sending {
    /// The schemas holding the guess's fields, with their definitions.
    holders: Vec<&'static Value>,
    /// The contract example of the first form taking the field, without it,
    /// as a row the value is sent in.
    row: Value,
    /// A text the field takes.
    text: String,
    /// A scalar the field takes: the example's own, else one of its type.
    scalar: Value,
}

/// The guess at `(within, field, sent)` of `id`'s rows, sent as `value`.
fn sending(id: ToolId, (within, field, sent): At, value: Value) -> Sending {
    let tool = crate::contracts::tool_contract(id).expect("contract");
    let (query, defs) = (&tool["querySchema"], &tool["querySchema"]["$defs"]);
    let holders = match within {
        "" => vec![query],
        within => child_schemas(query, defs, within),
    };
    let taking = if within.is_empty() { field } else { within };
    let mut row = tool["variants"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|form| {
            form["fields"]
                .as_array()
                .is_some_and(|fields| fields.iter().any(|f| f == taking))
        })
        .map(|form| form["example"].clone())
        .expect("a form takes the field");
    let holder = match within {
        "" => &mut row,
        within => row
            .as_object_mut()
            .expect("row")
            .entry(within)
            .or_insert(json!({})),
    };
    // The example's own value of the field, else the first its schema names.
    let example = holder.as_object_mut().expect("object").remove(field);
    holder[sent] = value;
    let field = resolve_ref(
        holders
            .iter()
            .flat_map(|h| child_schemas(h, defs, field))
            .next()
            .expect("declared"),
        defs,
    );
    let text = [
        example
            .as_ref()
            .and_then(|value| value.as_array().map_or(Some(value), |items| items.first())),
        Some(&field["enum"][0]),
        Some(&resolve_ref(&field["items"], defs)["enum"][0]),
    ]
    .into_iter()
    .flatten()
    .find_map(Value::as_str)
    .unwrap_or("x")
    .to_owned();
    let scalar = example
        .filter(|value| !value.is_object() && !value.is_array())
        .unwrap_or_else(|| match field["type"].as_str() {
            Some("boolean") => json!(true),
            Some("integer" | "number") => field.get("minimum").cloned().unwrap_or_else(|| json!(1)),
            _ => json!(text),
        });
    Sending {
        holders,
        row,
        text,
        scalar,
    }
}

/// Every guess sends a name its tools do not declare, for one they do, and
/// no other guess shadows it. Each reading the table states runs and its
/// note names the field (and the value, where it differs from the one sent);
/// a value no reading takes is refused naming the field.
#[test]
fn every_guess_reads_as_the_table_says_and_names_its_field() {
    for guess in GUESSES {
        for id in guess.tools {
            let tool = id.as_str();
            let Sending {
                holders,
                row: refused,
                text,
                scalar,
            } = sending(*id, at(guess), json!({}));
            let defs =
                &crate::contracts::tool_contract(*id).expect("contract")["querySchema"]["$defs"];
            let declared = |name: &str| {
                holders
                    .iter()
                    .any(|h| property_names(h, defs).contains(name))
            };
            assert!(
                !declared(guess.sent) && declared(guess.field),
                "{tool} {}",
                guess.sent
            );
            assert_eq!(
                crate::contracts::meant_field(tool, guess.within, guess.sent).map(|meant| meant.0),
                Some(guess.field),
                "{tool}: `{}` is shadowed",
                guess.sent
            );
            let (_, notes, shown) = run(tool, refused);
            let shown = shown.expect("refused");
            assert!(notes.is_empty(), "{tool} {}: {notes:?}", guess.sent);
            assert!(
                shown.contains(&format!("Unknown field '{}'", guess.sent))
                    && shown.contains(&format!("use '{}'", guess.field)),
                "{tool}: {shown}"
            );
            let stated = match guess.read {
                Read::Switch(meant) => vec![(json!(true), json!(meant))],
                Read::Text => vec![(json!(text), json!(text))],
                Read::Listed => vec![(json!(text), json!([text])), (json!([text]), json!([text]))],
                Read::Same => vec![(scalar.clone(), scalar)],
                Read::Refused | Read::Shaped(_) => Vec::new(),
            };
            for (sent, meant) in stated {
                let row = sending(*id, at(guess), sent.clone()).row;
                let (row, notes, shown) = run(tool, row);
                assert_eq!(
                    (&held(&row, guess.within)[guess.field], shown),
                    (&meant, None),
                    "{tool} {}",
                    guess.sent
                );
                let field = label(guess.within, guess.field);
                let named = if meant == sent {
                    format!("send `{field}` next time")
                } else {
                    format!("send `{field}: {meant}` next time")
                };
                assert!(
                    notes.len() == 1 && notes[0].contains(&named),
                    "{tool}: {notes:?}"
                );
            }
        }
    }
}

/// The readers that read by value shape: the row runs with these fields,
/// and a note says what to send; a shape with several readings is refused,
/// the error naming the field.
#[test]
fn a_value_read_by_its_shape_runs_and_is_named_or_refused() {
    let rust = r#"send `matchString` with `regexEngine: "rust"` next time"#;
    for (tool, sent, ran, note) in [
        (
            "localSearch",
            json_fixture(r#"{"path": ".", "matchString": "x", "mode": "discovery"}"#),
            json_fixture(r#"{"view": "files"}"#),
            "send `view: \"files\"`",
        ),
        (
            "localSearch",
            json_fixture(r#"{"path": ".", "matchString": "x", "mode": "pcre2"}"#),
            json_fixture(r#"{"regexEngine": "pcre2"}"#),
            "send `regexEngine`",
        ),
        // A regex runs in rust; text whose regex reading spells it, or that
        // does not compile, is literal; an engine in the row says how it reads.
        (
            "localSearch",
            json_fixture(r#"{"path": ".", "pattern": ["x", "^(class |def )"]}"#),
            json_fixture(
                r#"{
                    "matchString": ["x", "^(class |def )"],
                    "regexEngine": "rust"
                }"#,
            ),
            rust,
        ),
        // A text for the list names the list it runs as.
        (
            "localSearch",
            json_fixture(r#"{"path": ".", "pattern": "defects-queue"}"#),
            json_fixture(r#"{"matchString": ["defects-queue"], "regexEngine": null}"#),
            "send `matchString: [\"defects-queue\"]` next",
        ),
        // One GitHub keyword is one term to match.
        (
            "localSearch",
            json_fixture(r#"{"path": ".", "keywords": ["registerHandler"]}"#),
            json_fixture(r#"{"matchString": ["registerHandler"]}"#),
            "read `keywords` as `matchString`; send `matchString` next time.",
        ),
        (
            "localSearch",
            json_fixture(r#"{"path": ".", "pattern": "call("}"#),
            json_fixture(r#"{"matchString": ["call("], "regexEngine": null}"#),
            "send `matchString: [\"call(\"]` next",
        ),
        (
            "localSearch",
            json_fixture(r#"{"path": ".", "pattern": ["a|b"], "regexEngine": "pcre2"}"#),
            json_fixture(r#"{"regexEngine": "pcre2"}"#),
            "send `matchString` next",
        ),
        (
            "structureSearch",
            json_fixture(r#"{"path": ".", "name": "*.ts"}"#),
            json_fixture(r#"{"include": ["*.ts"]}"#),
            "send `include: [\"*.ts\"]`",
        ),
        (
            "ghSearchCode",
            json_fixture(r#"{"owner": "o", "query": "fix  login"}"#),
            json_fixture(r#"{"keywords": ["fix", "login"]}"#),
            "send `keywords: [\"fix\",\"login\"]`",
        ),
        // Paths to skip add up.
        (
            "localSearch",
            json_fixture(
                r#"{
                    "path": ".",
                    "matchString": "x",
                    "exclude": ["a"],
                    "excludeDir": "b"
                }"#,
            ),
            json_fixture(r#"{"exclude": ["a", "b"]}"#),
            "send `exclude: [\"a\",\"b\"]`",
        ),
        (
            "localFetch",
            json_fixture(r#"{"path": "a", "startLine": 3, "endLine": 9}"#),
            json_fixture(r#"{"ranges": [{"startLine": 3, "endLine": 9}]}"#),
            r#"read `startLine`, `endLine` as `ranges: [{"startLine":3,"endLine":9}]`; send `ranges: [{"startLine":3,"endLine":9}]` next time."#,
        ),
        (
            "ghGetFileContent",
            json_fixture(
                r#"{
                    "owner": "o",
                    "repo": "r",
                    "path": "a",
                    "ranges": [
                        [1, 2],
                        {"startLine": 3, "endLine": 4},
                        {"start": 5, "end": 9}
                    ]
                }"#,
            ),
            json_fixture(
                r#"{
                    "ranges": [
                        {"startLine": 1, "endLine": 2},
                        {"startLine": 3, "endLine": 4},
                        {"startLine": 5, "endLine": 9}
                    ]
                }"#,
            ),
            r#"send `ranges: [{"startLine":1,"endLine":2},{"startLine":3,"endLine":4},{"startLine":5,"endLine":9}]` next time."#,
        ),
        (
            "localSearch",
            json_fixture(r#"{"path": ".", "matchString": "x", "multiline": true}"#),
            json_fixture(r#"{"multiline": "on"}"#),
            r#"read `multiline: true` as `multiline: "on"`; send `multiline: "on"` next time."#,
        ),
        // A name regex lists files by name.
        (
            "structureSearch",
            json_fixture(r#"{"path": ".", "pattern": "^CONF.*\\.md$"}"#),
            json_fixture(r#"{"nameRegex": "^CONF.*\\.md$", "operation": "files"}"#),
            "send `nameRegex` with `operation: \"files\"`",
        ),
        // A field one row field's object declares is read inside it, and an
        // operation the row's fields fit alone is that operation.
        (
            "ghSearchHistory",
            json_fixture(
                r#"{
                    "operation": "pullRequest",
                    "owner": "o",
                    "repo": "r",
                    "state": "open"
                }"#,
            ),
            json_fixture(r#"{"filters": {"state": "open"}}"#),
            "send `filters: {\"state\":\"open\"}`",
        ),
        (
            "ghSearchHistory",
            json_fixture(
                r#"{
                    "operation": "pullRequest",
                    "owner": "o",
                    "repo": "r",
                    "state": "merged"
                }"#,
            ),
            json_fixture(r#"{"filters": {"isMerged": true}}"#),
            "send `filters: {\"isMerged\":true}`",
        ),
        (
            "astSearch",
            json_fixture(r#"{"path": ".", "pattern": "eval($X)"}"#),
            json_fixture(r#"{"rule": {"pattern": "eval($X)"}, "operation": "match"}"#),
            "send `rule: {\"pattern\":\"eval($X)\"}`",
        ),
        (
            "astSearch",
            json_fixture(r#"{"path": ".", "rule": {"pattern": "eval($X)"}}"#),
            json_fixture(r#"{"operation": "match"}"#),
            "send `operation: \"match\"`",
        ),
        // A value the field takes in another case or separator style.
        (
            "securityCheck",
            json_fixture(
                r#"{
                    "operation": "package",
                    "ecosystem": "pypi",
                    "packageName": "requests"
                }"#,
            ),
            json_fixture(r#"{"ecosystem": "PyPI"}"#),
            "as `ecosystem: \"PyPI\"`; send `ecosystem: \"PyPI\"` next time",
        ),
        (
            "artifactSearch",
            json_fixture(
                r#"{
                    "operation": "resolve",
                    "ecosystem": "PyPI",
                    "packageName": "requests"
                }"#,
            ),
            json_fixture(r#"{"ecosystem": "pypi"}"#),
            "as `ecosystem: \"pypi\"`; send `ecosystem: \"pypi\"` next time",
        ),
    ] {
        let (row, notes, shown) = run(tool, sent.clone());
        for (field, value) in ran.as_object().expect("fields") {
            assert_eq!(
                row.get(field).unwrap_or(&Value::Null),
                value,
                "{tool} {sent}: {row}"
            );
        }
        assert!(
            shown.is_none() && notes.iter().any(|said| said.contains(note)),
            "{tool} {sent}: {notes:?} {shown:?}"
        );
    }
    for (tool, sent, named) in [
        (
            "localSearch",
            json_fixture(r#"{"path": ".", "matchString": ["x"], "pattern": "y"}"#),
            "use 'matchString': literal text, or a regex with regexEngine:\"rust\".",
        ),
        (
            "structureSearch",
            json_fixture(r#"{"path": ".", "name": "x.ts"}"#),
            "use 'include'",
        ),
        // GitHub ANDs its keywords; localSearch ORs its terms.
        (
            "localSearch",
            json_fixture(r#"{"path": ".", "keywords": ["fix", "login"]}"#),
            "use 'matchString': ORed terms; fileHas lists terms a file must all hold.",
        ),
        (
            "structureSearch",
            json_fixture(r#"{"path": ".", "pattern": "README"}"#),
            "use 'nameRegex'",
        ),
        (
            "ghSearchCode",
            json_fixture(r#"{"owner": "o", "query": "repo:a/b parse"}"#),
            "use 'keywords'",
        ),
        (
            "localFetch",
            json_fixture(r#"{"path": "a", "startLine": 4}"#),
            "use 'ranges'",
        ),
        (
            "localFetch",
            json_fixture(r#"{"path": "a", "ranges": [{"start": 1}]}"#),
            "Unknown field 'start'",
        ),
    ] {
        let (row, notes, shown) = run(tool, sent.clone());
        assert_eq!(row, sent, "left as sent");
        assert!(notes.is_empty(), "{tool} {sent}: {notes:?}");
        assert!(
            shown.as_deref().is_some_and(|shown| shown.contains(named)),
            "{tool} {sent}: {shown:?}"
        );
    }
}

/// Names an agent reads in the GitHub tools' own output or in
/// GitHub's API run as the field the tool takes, values mapped: every row
/// names `commitSha`, and `git clone --branch` and the commit listing's
/// `sha` pin what `ref` pins; a search `kind` (`pr`) is its `operation`;
/// GitHub's `in:file` is `matchIn: ["content"]`.
#[test]
fn github_names_from_output_and_the_api_run_as_the_declared_field() {
    let sha = "0123456789abcdef0123456789abcdef01234567";
    for (tool, sent, ran, note) in [
        (
            "ghGetFileContent",
            json!({"owner":"o","repo":"r","path":"a.rs","commitSha":sha}),
            json!({"ref":sha}),
            "send `ref` next time",
        ),
        (
            "ghStructure",
            json!({"operation":"tree","owner":"o","repo":"r","commitSha":sha}),
            json!({"ref":sha}),
            "send `ref` next time",
        ),
        (
            "ghSearchCode",
            json!({"owner":"o","repo":"r","keywords":["x"],"commitSha":sha}),
            json!({"ref":sha}),
            "send `ref` next time",
        ),
        (
            "ghCloneRepo",
            json!({"owner":"o","repo":"r","commitSha":sha}),
            json!({"ref":sha}),
            "send `ref` next time",
        ),
        (
            "ghCloneRepo",
            json_fixture(r#"{"owner": "o", "repo": "r", "branch": "trunk"}"#),
            json_fixture(r#"{"ref": "trunk"}"#),
            "read `branch` as `ref`; send `ref` next time.",
        ),
        (
            "ghGetFileContent",
            json_fixture(r#"{"owner": "o", "repo": "r", "path": "a.rs", "branch": "trunk"}"#),
            json_fixture(r#"{"ref": "trunk"}"#),
            "read `branch` as `ref`; send `ref` next time.",
        ),
        (
            "ghSearchHistory",
            json!({"operation":"commit","owner":"o","repo":"r","sha":"trunk"}),
            json_fixture(r#"{"ref": "trunk"}"#),
            "read `sha` as `ref`; send `ref` next time.",
        ),
        (
            "ghGetHistoryItem",
            json!({"operation":"commit","owner":"o","repo":"r","sha":sha}),
            json!({"ref":sha}),
            "read `sha` as `ref`; send `ref` next time.",
        ),
        (
            "ghSearchHistory",
            json_fixture(r#"{"kind": "pr", "owner": "o", "repo": "r", "keywords": ["x"]}"#),
            json_fixture(r#"{"operation": "pullRequest"}"#),
            "send `operation: \"pullRequest\"` next time",
        ),
        (
            "ghSearchHistory",
            json_fixture(
                r#"{
                    "kind": "issues",
                    "owner": "o",
                    "repo": "r",
                    "keywords": ["x"]
                }"#,
            ),
            json_fixture(r#"{"operation": "issue"}"#),
            "send `operation: \"issue\"` next time",
        ),
        (
            "ghSearchHistory",
            json_fixture(
                r#"{
                    "type": "pull_request",
                    "owner": "o",
                    "repo": "r",
                    "keywords": ["x"]
                }"#,
            ),
            json_fixture(r#"{"operation": "pullRequest"}"#),
            "send `operation: \"pullRequest\"` next time",
        ),
        // GitHub's compare API names its sides `base` and `head`.
        (
            "ghGetHistoryItem",
            json_fixture(
                r#"{
                    "owner": "o",
                    "repo": "r",
                    "operation": "compare",
                    "base": "v1",
                    "head": "v2"
                }"#,
            ),
            json_fixture(r#"{"baseRef": "v1", "headRef": "v2"}"#),
            "send `baseRef` next time",
        ),
        // ast-grep's `language`.
        (
            "astSearch",
            json_fixture(
                r#"{
                    "operation": "match",
                    "path": ".",
                    "rule": {"pattern": "f($X)"},
                    "language": "rust"
                }"#,
            ),
            json_fixture(r#"{"parser": "rust"}"#),
            "send `parser` next time",
        ),
        // ast-grep's `--lang`.
        (
            "astSearch",
            json_fixture(
                r#"{
                    "operation": "match",
                    "path": ".",
                    "rule": {"pattern": "f($X)"},
                    "lang": "rust"
                }"#,
            ),
            json_fixture(r#"{"parser": "rust"}"#),
            "read `lang` as `parser`; send `parser` next time.",
        ),
        (
            "ghSearchCode",
            json_fixture(
                r#"{
                    "owner": "o",
                    "repo": "r",
                    "keywords": ["x"],
                    "match": "file"
                }"#,
            ),
            json_fixture(r#"{"matchIn": ["content"]}"#),
            "send `matchIn: [\"content\"]` next time",
        ),
        (
            "ghSearchCode",
            json_fixture(
                r#"{
                    "owner": "o",
                    "repo": "r",
                    "keywords": ["x"],
                    "match": ["file", "path"]
                }"#,
            ),
            json_fixture(r#"{"matchIn": ["content", "path"]}"#),
            "send `matchIn: [\"content\",\"path\"]` next time",
        ),
    ] {
        let (row, notes, shown) = run(tool, sent.clone());
        for (field, value) in ran.as_object().expect("fields") {
            assert_eq!(
                row.get(field).unwrap_or(&Value::Null),
                value,
                "{tool} {sent}: {row}"
            );
        }
        assert!(
            shown.is_none() && notes.iter().any(|said| said.contains(note)),
            "{tool} {sent}: {notes:?} {shown:?}"
        );
    }
}

/// A `{tool, query}` field sent as the query alone (`source:{path}`) runs as
/// the one tool whose query form takes exactly those fields, wherever the
/// field sits, and the call names the form to send. A value fitting no tool,
/// or sending a field of the union itself, is left for validation.
#[test]
fn a_tool_query_sent_alone_runs_as_its_one_tool_and_is_named() {
    let questions = json_fixture(r#"{"q": {"type": "noul", "instructions": "Is it ok?"}}"#);
    for (sent, at, read) in [
        (
            json!({"operation":"evaluate","source":{"owner":"o","repo":"r","path":"x.ts"},"questions":questions}),
            "/source",
            json_fixture(
                r#"{
                    "tool": "ghGetFileContent",
                    "query": {"owner": "o", "repo": "r", "path": "x.ts"}
                }"#,
            ),
        ),
        (
            json!({"operation":"sections","resources":{"a":{"path":"x.ts"}},"questions":questions}),
            "/resources/a",
            json_fixture(r#"{"tool": "localFetch", "query": {"path": "x.ts"}}"#),
        ),
        (
            json!({"operation":"evaluate","content":"Screenshot a.","questions":questions,"assets":{"a":{"source":{"path":"a.png"}}}}),
            "/assets/a/source",
            json_fixture(r#"{"tool": "localFetch", "query": {"path": "a.png"}}"#),
        ),
    ] {
        let (row, notes, shown) = run("clasify", sent.clone());
        assert_eq!((row.pointer(at), shown), (Some(&read), None), "{sent}");
        let form = format!("`{}: {}`", at[1..].replace('/', "."), sketch(&read));
        assert!(
            notes.len() == 1 && notes[0].ends_with(&format!(" as {form}; send {form} next time.")),
            "{notes:?}"
        );
    }
    for source in [
        json_fixture(r#"{"path": "x.ts", "bogus": 1}"#),
        json_fixture(r#"{"ranges": [{"startLine": 1, "endLine": 2}]}"#),
    ] {
        let sent = json!({"operation":"evaluate","source":source,"questions":questions});
        let (row, notes, _) = run("clasify", sent.clone());
        assert_eq!((&row["source"], notes.len()), (&source, 0), "{sent}");
    }
}

/// A repair is a fact of the call: the same guess sent on several rows is
/// said once, naming every row it was read on, and rows that sent nothing
/// to repair are not named. Each distinct repair keeps its own note.
#[test]
fn a_repair_sent_on_several_rows_is_said_once_naming_them() {
    for guess in GUESSES {
        let Read::Switch(_) = guess.read else {
            continue;
        };
        for id in guess.tools {
            let tool = id.as_str();
            let guessed = sending(*id, at(guess), json!(true)).row;
            let mut clean = sending(*id, at(guess), json!({})).row;
            if let Some(row) = clean.as_object_mut() {
                row.remove(guess.sent);
            }
            let batch = json!({"queries": [guessed, clean, guessed]});
            let (input, notes) = normalized_input(tool, batch);
            assert_eq!(notes.len(), 1, "{tool} {}: {notes:?}", guess.sent);
            assert!(
                notes[0].starts_with("queries[0,2]: "),
                "{tool} {}: {notes:?}",
                guess.sent
            );
            let single = run(tool, guessed.clone()).1;
            assert_eq!(
                notes[0].strip_prefix("queries[0,2]: "),
                single[0].strip_prefix("queries[0]: "),
                "{tool}: the call-level note says what the row note says"
            );
            assert!(input["queries"][0].get(guess.sent).is_none(), "{input}");
        }
    }
    let (_, notes) = normalized_input(
        "localSearch",
        json_fixture(
            r#"{
                "queries": [
                    {"path": ".", "matchString": ["x"], "literal": true},
                    {"path": ".", "matchString": ["x"], "filesOnly": true},
                    {"path": ".", "matchString": ["x"], "literal": true}
                ]
            }"#,
        ),
    );
    assert_eq!(notes.len(), 2, "{notes:?}");
    assert!(notes[0].starts_with("queries[0,2]: ") && notes[1].starts_with("queries[1]: "));
}

/// Every spelling sends a value its field refuses, in the table's case or
/// another, and runs as the field and value the table reads it as, its note
/// naming them.
#[test]
fn every_spelling_reads_as_the_table_says_and_names_its_field() {
    for spelling in SPELLINGS {
        let (field, meant) = spelling.reads;
        let meant: Value = serde_json::from_str(meant).expect("the reading is JSON");
        for id in spelling.tools {
            // A reading of a value this contract does not declare is not made.
            if !spelling.declared(*id) {
                continue;
            }
            let tool = id.as_str();
            for sent in [spelling.sent.to_owned(), spelling.sent.to_uppercase()] {
                let at = (spelling.within, spelling.field, spelling.field);
                let row = sending(*id, at, json!(sent)).row;
                let refused = prepare_rows(tool, json!({"queries": [row.clone()]}))
                    .map_or(true, |rows| rows.iter().any(Result::is_err));
                assert!(refused, "{tool}: {sent} is a value the field refuses");
                let (ran, notes, shown) = run(tool, row);
                let holder = held(&ran, spelling.within);
                // The example row of a call that anchors on a symbol still
                // sends `symbolName` and `line`, which the repaired
                // `documentSymbols` does not take: validation names them.
                let anchored = meant == json!("documentSymbols");
                if anchored {
                    assert_eq!(&holder[field], &meant, "{tool} {sent}: {ran}");
                    assert!(
                        shown.is_some_and(|shown| shown.contains("'symbolName' is not allowed")),
                        "{tool} {sent}"
                    );
                } else {
                    assert_eq!(
                        (&holder[field], shown),
                        (&meant, None),
                        "{tool} {sent}: {ran}"
                    );
                }
                if field != spelling.field {
                    assert!(holder.get(spelling.field).is_none(), "{tool}: {ran}");
                }
                // The note gives the form to send: the field with the value it
                // ran with, or, where the row's object was also repaired, the
                // whole object as it ran. A row that still fails (an anchored
                // call) says only what was read: its form is not one to send.
                let said = |form: String| {
                    if anchored {
                        format!("as {form}.")
                    } else {
                        format!("as {form}; send {form} next time")
                    }
                };
                let at = label(spelling.within, field);
                let mut named = vec![said(format!("`{at}: {meant}`"))];
                if !spelling.within.is_empty() {
                    named.push(said(format!("`{}: {}`", spelling.within, sketch(holder))));
                }
                assert!(
                    notes
                        .iter()
                        .any(|note| named.iter().any(|form| note.contains(form))),
                    "{tool} {sent}: {notes:?}"
                );
            }
        }
    }
}

/// Every value that names what each read of its operation returns is one
/// the contract refuses there, and its stated field is one the tool's rows
/// return. Sent alone it leaves the list out, beside a value the list takes
/// it leaves that value; the call runs either way, its note naming the field.
#[test]
fn every_returned_value_is_left_out_and_names_its_field() {
    use crate::contracts::normalize::guesses::RETURNED;
    use crate::contracts::schema_facts::item_schemas;
    for returned in RETURNED {
        for id in returned.tools {
            let (tool, field) = (id.as_str(), returned.field);
            let contract = crate::contracts::tool_contract(*id).expect("contract");
            let output = &contract["outputSchema"];
            let returns = child_schemas(output, &output["$defs"], "results")
                .into_iter()
                .flat_map(|results| item_schemas(results, &output["$defs"]))
                .flat_map(|row| child_schemas(row, &output["$defs"], "data"))
                .any(|data| property_names(data, &output["$defs"]).contains(returned.returned_as));
            assert!(returns, "{tool}: rows return `{}`", returned.returned_as);
            let row = contract["variants"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|form| {
                    form["operations"]
                        .as_array()
                        .is_some_and(|ops| ops.iter().any(|op| op == returned.operation))
                })
                .map(|form| form["example"].clone())
                .expect("a form runs the operation");
            let (query, defs) = (&contract["querySchema"], &contract["querySchema"]["$defs"]);
            let taken = query["anyOf"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|form| resolve_ref(form, defs))
                .filter(|form| form["properties"]["operation"]["const"] == returned.operation)
                .flat_map(|form| child_schemas(form, defs, field))
                .flat_map(|list| item_schemas(list, defs))
                .find_map(|item| resolve_ref(item, defs)["enum"][0].as_str())
                .expect("the list takes a value");
            for (sent, ran) in [
                (json!([returned.sent]), None),
                (
                    json!([returned.sent.to_uppercase(), taken]),
                    Some(json!([taken])),
                ),
            ] {
                let mut row = row.clone();
                row[field] = sent.clone();
                let refused = prepare_rows(tool, json!({"queries": [row.clone()]}))
                    .map_or(true, |rows| rows.iter().any(Result::is_err));
                assert!(refused, "{tool}: `{field}: {sent}` is refused as sent");
                let (row, notes, shown) = run(tool, row);
                assert_eq!(
                    (row.get(field).cloned(), shown),
                    (ran, None),
                    "{tool} {sent}"
                );
                let named = format!("`{}`", returned.returned_as);
                assert!(
                    notes.len() == 1
                        && notes[0].contains(&named)
                        && notes[0].contains(returned.sent),
                    "{tool} {sent}: {notes:?}"
                );
            }
        }
    }
}

/// An old `matchPage` runs as no field, and the note names the cursor that
/// pages the hits now.
#[test]
fn a_retired_match_page_is_left_out_and_names_the_cursor() {
    let (input, notes) = normalized_input(
        "localSearch",
        json_fixture(
            r#"{
                "queries": [{"path": ".", "matchString": ["x"], "matchPage": 2}]
            }"#,
        ),
    );
    assert!(input["queries"][0].get("matchPage").is_none(), "{input}");
    assert!(
        notes
            .iter()
            .any(|note| note.contains("matchPage") && note.contains("next.nextPage")),
        "{notes:?}"
    );
}

/// Names agents carry over from other tools' output and flags, each with one
/// reading, run as the declared field or value, and the call says the field
/// to send: a commit pinned by `commitSha`, plural operations, a symbol, an
/// `outline`, a `find`, `cargo`, a counting flag and a clasify source whose
/// query fields sit beside its tool.
#[test]
fn near_miss_names_run_as_the_declared_field_and_say_it() {
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let questions = json_fixture(r#"{"q": {"type": "noul", "instructions": "Is it ok?"}}"#);
    for (tool, sent, ran, note) in [
        (
            "ghGetHistoryItem",
            json!({"operation":"commit","owner":"o","repo":"r","commitSha":sha}),
            json!({"ref":sha}),
            "send `ref` next time",
        ),
        (
            "ghSearchHistory",
            json_fixture(r#"{"operation": "commits", "owner": "o", "repo": "r"}"#),
            json_fixture(r#"{"operation": "commit"}"#),
            "send `operation: \"commit\"` next time",
        ),
        (
            "ghSearchHistory",
            json_fixture(
                r#"{
                    "operation": "pullRequests",
                    "owner": "o",
                    "repo": "r",
                    "keywords": ["x"]
                }"#,
            ),
            json_fixture(r#"{"operation": "pullRequest"}"#),
            "send `operation: \"pullRequest\"` next time",
        ),
        (
            "lspSearch",
            json_fixture(
                r#"{
                    "operation": "callers",
                    "path": "a.ts",
                    "symbol": "run",
                    "line": 1
                }"#,
            ),
            json_fixture(r#"{"symbolName": "run"}"#),
            "send `symbolName` next time",
        ),
        (
            "lspSearch",
            json_fixture(r#"{"operation": "outline", "path": "a.ts"}"#),
            json_fixture(r#"{"operation": "documentSymbols"}"#),
            "send `operation: \"documentSymbols\"` next time",
        ),
        (
            "graphQuery",
            json_fixture(r#"{"operation": "deps", "path": ".", "symbol": "a.ts"}"#),
            json_fixture(r#"{"target": "a.ts"}"#),
            "send `target` next time",
        ),
        (
            "graphQuery",
            json_fixture(r#"{"operation": "witnesses", "path": ".", "symbol": "run"}"#),
            json_fixture(r#"{"symbolName": "run"}"#),
            "send `symbolName` next time",
        ),
        (
            "structureSearch",
            json_fixture(r#"{"operation": "find", "path": ".", "nameRegex": "x"}"#),
            json_fixture(r#"{"operation": "files"}"#),
            "send `operation: \"files\"` next time",
        ),
        (
            "structureSearch",
            json_fixture(
                r#"{
                    "operation": "files",
                    "path": ".",
                    "sortBy": {"by": "bytes", "order": "desc"}
                }"#,
            ),
            json_fixture(r#"{"sort": {"by": "bytes", "order": "desc"}}"#),
            "send `sort` next time",
        ),
        (
            "artifactSearch",
            json_fixture(
                r#"{
                    "operation": "resolve",
                    "ecosystem": "cargo",
                    "packageName": "serde"
                }"#,
            ),
            json_fixture(r#"{"ecosystem": "crates"}"#),
            "send `ecosystem: \"crates\"` next time",
        ),
        (
            "artifactSearch",
            json_fixture(
                r#"{
                    "operation": "resolve",
                    "ecosystem": "crates",
                    "name": "serde"
                }"#,
            ),
            json_fixture(r#"{"packageName": "serde"}"#),
            "send `packageName` next time",
        ),
        (
            "localSearch",
            json_fixture(r#"{"path": ".", "matchString": ["x"], "count": true}"#),
            json_fixture(r#"{"view": "countMatches"}"#),
            "send `view",
        ),
        (
            "localSearch",
            json_fixture(r#"{"path": ".", "matchString": ["x"], "excludePaths": ["target"]}"#),
            json_fixture(r#"{"exclude": ["target"]}"#),
            "read `excludePaths` as `exclude`; send `exclude` next time.",
        ),
        (
            "clasify",
            json!({"operation":"evaluate","source":{"tool":"localFetch","path":"x.ts"},"questions":questions}),
            json_fixture(
                r#"{
                    "source": {"tool": "localFetch", "query": {"path": "x.ts"}}
                }"#,
            ),
            r#"read `source.path` as `source.query: {"path":"x.ts"}`; send `source.query: {"path":"x.ts"}` next time."#,
        ),
    ] {
        let (row, notes, shown) = run(tool, sent.clone());
        for (field, value) in ran.as_object().expect("fields") {
            assert_eq!(
                row.get(field).unwrap_or(&Value::Null),
                value,
                "{tool} {sent}: {row}"
            );
        }
        assert!(
            shown.is_none() && notes.len() == 1 && notes[0].contains(note),
            "{tool} {sent}: {notes:?} {shown:?}"
        );
    }
    // A bare sort key names no order: refused, naming `sort`.
    let (_, _, shown) = run(
        "structureSearch",
        json_fixture(r#"{"operation": "files", "path": ".", "sortBy": "sizeBytes"}"#),
    );
    assert!(shown.is_some_and(|shown| shown.contains("sort")));
}

/// A size sort has one key, the file's size in bytes: `size` and the other
/// name of it run as the one the contract declares (`bytes`, or `sizeBytes`
/// in a contract that still names it so), and the call says it once.
#[test]
fn a_size_sort_key_runs_as_the_one_the_contract_declares() {
    for (tool, row) in [
        (
            "structureSearch",
            json_fixture(r#"{"operation": "files", "path": "."}"#),
        ),
        (
            "ghStructure",
            json_fixture(
                r#"{
                    "operation": "tree",
                    "owner": "o",
                    "repo": "r",
                    "ref": "main",
                    "materialize": false
                }"#,
            ),
        ),
    ] {
        let declared = ["bytes", "sizeBytes"]
            .into_iter()
            .filter(|value| {
                let mut sent = row.clone();
                sent["sort"] = json!({"by": value, "order": "desc"});
                let (_, notes, shown) = run(tool, sent);
                notes.is_empty() && shown.is_none()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            declared.len(),
            1,
            "{tool}: exactly one spelling is declared"
        );
        for sent_by in ["size", "bytes", "sizeBytes"] {
            if sent_by == declared[0] {
                continue;
            }
            let mut sent = row.clone();
            sent["sort"] = json!({"by": sent_by, "order": "desc"});
            let (ran, notes, shown) = run(tool, sent);
            assert_eq!(
                ran["sort"]["by"],
                json!(declared[0]),
                "{tool} {sent_by}: {ran}"
            );
            assert!(shown.is_none(), "{tool} {sent_by}: {shown:?}");
            let form = format!("`sort.by: \"{}\"`", declared[0]);
            assert!(
                matches!(notes.as_slice(), [one]
                    if one.ends_with(&format!(" as {form}; send {form} next time."))),
                "{tool} {sent_by}: {notes:?}"
            );
        }
    }
}

/// securityCheck's package row names its package `packageName`, so an
/// agent's `name` there has one reading, and the call names the field.
#[test]
fn security_check_reads_name_as_package_name() {
    let (row, notes, shown) = run(
        "securityCheck",
        json_fixture(r#"{"operation": "package", "ecosystem": "npm", "name": "lodash"}"#),
    );
    assert_eq!(
        (&row["packageName"], row.get("name"), shown),
        (&json!("lodash"), None, None),
        "{row}"
    );
    assert!(
        matches!(notes.as_slice(), [one]
            if one.ends_with("read `name` as `packageName`; send `packageName` next time.")),
        "{notes:?}"
    );
}

/// securityCheck's commit row names its commit `commitSha`, as every row
/// that reads a commit does: a `commit` there runs as `commitSha`, and the
/// call names the field once.
#[test]
fn security_check_reads_commit_as_commit_sha() {
    let sha = "6879efc2c1596d11a6a6ad296f80063b558d5e0f";
    let (row, notes, shown) = run(
        "securityCheck",
        json!({"operation": "commit", "commit": sha}),
    );
    assert_eq!(
        (&row["commitSha"], row.get("commit"), shown),
        (&json!(sha), None, None),
        "{row}"
    );
    assert!(
        matches!(notes.as_slice(), [one]
            if one.ends_with("read `commit` as `commitSha`; send `commitSha` next time.")),
        "{notes:?}"
    );
}

/// No other securityCheck operation takes a package: a `name` beside a
/// repository stays as sent, and its refusal offers no field that row
/// cannot take.
#[test]
fn security_check_reads_name_as_package_name_not_on_a_repository_row() {
    let sent =
        json_fixture(r#"{"operation": "repository", "repo": "tmcw/leftpad", "name": "leftpad"}"#);
    let (row, notes, shown) = run("securityCheck", sent.clone());
    assert_eq!(row, sent);
    assert!(notes.is_empty(), "{notes:?}");
    let shown = shown.expect("refused");
    assert!(
        shown.contains("Unknown field 'name'") && !shown.contains("packageName"),
        "{shown}"
    );
}

/// A line sent under another name (`lineHint`, `lineNumber`, `startLine`)
/// on an lspSearch row that anchors on one position runs as `line`, and the
/// call names the field once. A row whose operation takes no line keeps it
/// as sent, and its refusal names no `line`.
#[test]
fn a_position_line_under_another_name_runs_as_line() {
    for sent in ["lineHint", "lineNumber", "startLine"] {
        for operation in ["references", "definition", "hover"] {
            let mut row = json!({"operation":operation,"path":"a.ts","symbolName":"run"});
            row[sent] = json!(12);
            let (ran, notes, shown) = run("lspSearch", row.clone());
            assert_eq!(
                (&ran["line"], ran.get(sent), shown),
                (&json!(12), None, None),
                "{row}"
            );
            assert!(
                matches!(notes.as_slice(), [one]
                    if one.ends_with(&format!("read `{sent}` as `line`; send `line` next time."))),
                "{row}: {notes:?}"
            );
        }
        let row = json!({"operation":"documentSymbols","path":"a.ts",sent:12});
        let (ran, notes, shown) = run("lspSearch", row.clone());
        assert_eq!(ran, row);
        assert!(notes.is_empty(), "{notes:?}");
        let shown = shown.expect("refused");
        assert!(
            shown.contains(&format!("Unknown field '{sent}'")) && !shown.contains("'line'"),
            "{shown}"
        );
    }
}
