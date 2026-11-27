//! What is never read: a call that bills, an undeclared field, a field in
//! another object.
use super::*;

/// Nothing turns an invalid request into a billed one: a clasify request
/// runs only as sent, in every lenient form a row of another tool is read in.
#[test]
fn nothing_is_read_into_a_call_that_bills() {
    let lenient = |row: &Value| {
        let one_row = json!({ "queries": row });
        let text = json!({ "queries": json!([row]).to_string() });
        [row.clone(), json!([row]), one_row, text]
    };
    let mut billed = 0;
    for (tool, row) in examples()
        .into_iter()
        .filter(|(tool, _)| *tool == ToolId::Clasify)
    {
        for sent in lenient(&row) {
            assert_eq!(
                normalized_readings(tool.as_str(), sent.clone()),
                (sent, Vec::new()),
                "{tool:?}"
            );
            billed += 1;
        }
    }
    assert!(billed > 0, "clasify was tried");
}

/// A row that still fails is judged as read, so its refusal names only what
/// is still wrong with it: a discriminator in another case no longer hides
/// the field at fault behind "not allowed in this form". It states no note
/// (the refusal is its answer), while the other rows of the batch are read.
#[test]
fn a_row_that_still_fails_is_refused_for_what_is_still_wrong() {
    let lenient = json_fixture(r#"{"path": ".", "matchString": ["x"], "exclude": "target"}"#);
    let failing = with(&lenient, "undeclaredField", json!(1));
    let sent = json!({"queries": [failing, lenient]});
    let (input, notes) = normalized_readings("localSearch", sent);
    assert_eq!(input["queries"][0]["undeclaredField"], 1);
    assert_eq!(input["queries"][1]["exclude"], json!(["target"]));
    assert!(
        matches!(notes.as_slice(), [note] if note.starts_with("read queries[1]: ")),
        "{notes:?}"
    );
    let shown = refusal(ToolId::LocalSearch, &input).expect("row 0 is refused");
    assert!(shown.contains("'undeclaredField'"), "{shown}");

    // R6: a miscased form selector beside a value outside its enum.
    let sent = json!({"queries": [{"operation": "MATCH", "view": "Filez", "path": "src",
        "rule": {"pattern": "export const $A = $B"}}]});
    let (input, notes) = normalized_readings("astSearch", sent);
    assert_eq!(notes, Vec::<String>::new());
    let shown = refusal(ToolId::AstSearch, &input).expect("the row is refused");
    assert!(
        shown.contains("view") && shown.contains("\"Filez\"") && shown.contains("files"),
        "{shown}"
    );
    assert!(
        !shown.contains("MATCH") && !shown.contains("'rule'"),
        "{shown}"
    );
}

/// A value the schema gives no single typed reading stays as sent: a scalar
/// for an object (which member?), a number for a text, a list for a text.
/// Its refusal still says the form to send.
#[test]
fn a_value_without_one_typed_reading_is_refused() {
    for (field, value) in [
        ("contextLines", json!(2)),
        ("path", json!(5)),
        ("path", json!(["src"])),
        ("limit", json!([5])),
    ] {
        let row = with(
            &json_fixture(r#"{"path": ".", "matchString": ["x"]}"#),
            field,
            value,
        );
        let sent = request(&row);
        assert_eq!(
            normalized_readings("localSearch", sent.clone()),
            (sent.clone(), Vec::new()),
            "{field}"
        );
        let shown = refusal(ToolId::LocalSearch, &sent).expect("refused");
        assert!(shown.contains(field), "{shown}");
    }
}

/// A row field beside `queries` with a value the rows do not hold is never
/// given to them: the request is refused, and the refusal says where the
/// field goes. Held for the first field of every example. (A copy every row
/// already holds is dropped instead:
/// `a_row_field_repeated_beside_queries_with_the_rows_value_is_dropped`.)
#[test]
fn a_row_field_beside_queries_is_refused_saying_where_it_goes() {
    for (tool, row) in examples() {
        let fields = row.as_object().expect("a row");
        let Some((field, value)) = fields
            .iter()
            .find(|(field, _)| !envelope_fields(tool).contains(field))
        else {
            continue;
        };
        let mut sent = request(&row);
        sent[field] = differs(value);
        assert_eq!(
            normalized_readings(tool.as_str(), sent.clone()),
            (sent.clone(), Vec::new()),
            "{tool:?}"
        );
        let shown = refusal(tool, &sent).expect("refused");
        assert!(
            shown.contains(&format!("'{field}' goes inside each query")),
            "{tool:?}: {shown}"
        );
    }
}

/// A request field sent in a row is never moved beside `queries`: the row
/// is refused, and the refusal says where the field goes.
#[test]
fn a_request_field_in_a_row_is_refused_saying_where_it_goes() {
    for (tool, row) in examples() {
        for field in envelope_fields(tool) {
            if crate::contracts::query_declares(tool, &field) || field == "cursor" {
                continue;
            }
            let sent = request(&with(&row, &field, json!(2000)));
            assert_eq!(
                normalized_readings(tool.as_str(), sent.clone()),
                (sent.clone(), Vec::new()),
                "{tool:?}"
            );
            let shown = refusal(tool, &sent).expect("refused");
            assert!(
                shown.contains(&format!("'{field}' is a request field")),
                "{tool:?}: {shown}"
            );
        }
    }
}

/// An undeclared field is never read as another: the row stays as sent and
/// is refused in one line that names the field sent and a declared field of
/// that tool to use, or lists the declared fields. Another tool's word for a
/// field ([`FOREIGN_WORDS`](crate::contracts::validate::FOREIGN_WORDS)),
/// sent in place of that field, names it on a tool that declares it, and
/// every pair of the table is used by some tool.
#[test]
fn an_undeclared_field_is_refused_naming_a_declared_one() {
    use crate::contracts::validate::FOREIGN_WORDS;
    let mut used: Vec<&str> = Vec::new();
    for (tool, row) in examples() {
        let foreign = FOREIGN_WORDS.iter().map(|(word, meant)| (*word, *meant));
        for (word, meant) in foreign.chain([("zzNothingNear", &[][..])]) {
            if crate::contracts::query_declares(tool, word) {
                continue;
            }
            // The word beside the row's fields, and in place of each field
            // it may mean.
            let mut sent = vec![with(&row, word, json!("x"))];
            for field in meant.iter().filter(|field| row.get(**field).is_some()) {
                let mut replaced = row.as_object().expect("a row").clone();
                let value = replaced.remove(*field).expect("held");
                replaced.insert(word.to_owned(), value);
                sent.push(Value::Object(replaced));
            }
            for sent in sent.iter().map(request) {
                assert_eq!(
                    normalized_readings(tool.as_str(), sent.clone()),
                    (sent.clone(), Vec::new()),
                    "{tool:?} {word}"
                );
                let shown = refusal(tool, &sent).expect("refused");
                let Some(line) = shown
                    .lines()
                    .find(|line| line.contains(&format!("'{word}'")))
                else {
                    panic!("{tool:?} {word} is not named: {shown}");
                };
                let Some((_, named)) = line.split_once("; use ") else {
                    assert!(
                        line.contains("Allowed fields: ") || line.contains(" is a field of "),
                        "{tool:?} {word} names no declared field: {shown}"
                    );
                    continue;
                };
                let named: Vec<&str> = named.split('\'').skip(1).step_by(2).collect();
                assert!(!named.is_empty(), "{shown}");
                for name in &named {
                    assert!(
                        crate::contracts::query_declares(tool, name),
                        "{tool:?} {word}: `{name}` is not declared: {shown}"
                    );
                }
                if meant.contains(&named[0]) && !used.contains(&word) {
                    used.push(word);
                }
            }
        }
    }
    for (word, _) in FOREIGN_WORDS {
        assert!(used.contains(word), "no tool is told what `{word}` means");
    }
}

/// A value of the same JSON kind that differs from `value`.
fn differs(value: &Value) -> Value {
    match value {
        Value::String(text) => Value::String(format!("{text}-other")),
        Value::Number(number) => serde_json::json!(number.as_f64().unwrap_or_default() + 1.0),
        Value::Bool(flag) => Value::Bool(!flag),
        Value::Array(items) if items.is_empty() => serde_json::json!([null]),
        Value::Array(_) => serde_json::json!([]),
        Value::Object(_) => serde_json::json!({"other": true}),
        Value::Null => serde_json::json!(true),
    }
}

/// A field sent in the row that belongs inside an object field is refused
/// with a hint naming that object in a form it accepts: with every member
/// the object requires, whatever its form (`sort:{"by":…,"order":…}`, not
/// `sort:{"order":…}`, which lacks the `by` a sort requires). Held for every
/// row field whose object requires a member, on each example.
#[test]
fn a_misplaced_members_hint_names_the_members_its_object_requires() {
    use crate::contracts::schema_facts::{always_required, property_names, property_schemas};
    let mut hinted: Vec<(ToolId, String)> = Vec::new();
    for (tool, row) in examples() {
        let contract = crate::contracts::tool_contract(tool).expect("contract");
        let query = &contract["querySchema"];
        let defs = &query["$defs"];
        let row_fields = property_names(query, defs);
        for owner in &row_fields {
            let schemas = property_schemas(query, defs, owner);
            let Some((first, rest)) = schemas.split_first() else {
                continue;
            };
            let required: Vec<&str> = always_required(first, defs)
                .into_iter()
                .filter(|name| {
                    rest.iter()
                        .all(|other| always_required(other, defs).contains(name))
                })
                .collect();
            if required.is_empty() {
                continue;
            }
            let members: Vec<&str> = schemas
                .iter()
                .flat_map(|schema| property_names(schema, defs))
                .filter(|member| !row_fields.contains(member) && !required.contains(member))
                .collect();
            for member in members {
                let sent = request(&with(&row, member, json!("x")));
                let shown = refusal(tool, &sent).expect("refused");
                let Some(form) = shown
                    .lines()
                    .filter(|line| line.contains(&format!("'{member}' is a field of")))
                    .find_map(|line| line.split_once(&format!("{owner}:{{")))
                    .and_then(|(_, form)| form.split_once('}'))
                    .map(|(form, _)| form)
                else {
                    continue;
                };
                for name in required.iter().chain([&member]) {
                    assert!(
                        form.contains(&format!("\"{name}\":…")),
                        "{tool:?} {member}: the hint lacks `{owner}.{name}`: {shown}"
                    );
                }
                let at = (tool, format!("{owner}.{member}"));
                if !hinted.contains(&at) {
                    hinted.push(at);
                }
            }
        }
    }
    assert!(
        hinted
            .iter()
            .any(|(tool, at)| tool.as_str() == "structureSearch" && at == "sort.order"),
        "structureSearch's misplaced `order` is hinted: {hinted:?}"
    );
}
