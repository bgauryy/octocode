//! Each lenient reading holds for every field of the schema shape it is
//! derived from, on every tool that does not bill.
use super::*;

/// One lenient form of an example row: what it is, and the request sent.
struct Form {
    kind: &'static str,
    /// The row field re-formed; `None`: the envelope.
    field: Option<String>,
    sent: Value,
}

/// Every lenient form of `row`'s request: its rows without their envelope,
/// and each field's value as one item for its list, as JSON text, and as a
/// literal in another case and separator style.
fn forms(row: &Value) -> Vec<Form> {
    let whole = request(row);
    let mut forms = vec![
        ("bare row", row.clone()),
        ("bare list", json!([row])),
        ("one row for queries", json!({ "queries": row })),
        (
            "queries as JSON text",
            json!({ "queries": whole["queries"].to_string() }),
        ),
    ]
    .into_iter()
    .map(|(kind, sent)| Form {
        kind,
        field: None,
        sent,
    })
    .collect::<Vec<_>>();
    for (field, value) in row.as_object().expect("a row") {
        let mut reform = |kind, value: Value| {
            forms.push(Form {
                kind,
                field: Some(field.clone()),
                sent: request(&with(row, field, value)),
            });
        };
        match value {
            Value::Array(items) if items.len() == 1 => {
                reform("one value for a list", items[0].clone());
                reform("JSON text", json!(value.to_string()));
            }
            Value::String(literal) => {
                let mut cased = String::new();
                for character in literal.chars() {
                    if character.is_uppercase() {
                        cased.push('_');
                    }
                    cased.extend(character.to_uppercase());
                }
                reform("literal in another case", json!(cased));
            }
            Value::Null => {}
            other => reform("JSON text", json!(other.to_string())),
        }
    }
    forms
}

/// Whether the schema gives `form` its one reading: the form is refused as
/// sent, and by nothing but its own value (a type, or a literal outside its
/// enum with one literal spelled so).
fn has_one_reading(tool: ToolId, form: &Form) -> bool {
    let issues = issues(tool, &form.sent);
    let Some(field) = &form.field else {
        return !issues.is_empty();
    };
    let at = ["queries", "0", field.as_str()];
    !issues.is_empty()
        && issues.iter().all(|issue| {
            issue.path == at && matches!(issue.rule_id.as_str(), "schema.type" | "schema.enum")
        })
}

/// Every lenient form of every example that the schema gives one reading is
/// read as the example itself, with one warning that names the field and
/// the form to send. A form that is valid as sent is untouched. Each kind of
/// reading is exercised on more than one tool.
#[test]
fn a_form_with_one_typed_reading_is_read_as_the_declared_form() {
    let mut read: std::collections::BTreeMap<&str, Vec<ToolId>> = Default::default();
    for (tool, row) in examples() {
        if bills(tool) {
            continue;
        }
        for form in forms(&row) {
            let (input, notes) = normalized_readings(tool.as_str(), form.sent.clone());
            if !has_one_reading(tool, &form) {
                assert_eq!(
                    (&input, notes.len()),
                    (&form.sent, 0),
                    "{tool:?} {}: {}",
                    form.kind,
                    form.sent
                );
                continue;
            }
            assert_eq!(
                input,
                request(&row),
                "{tool:?} {}: {}",
                form.kind,
                form.sent
            );
            let [note] = notes.as_slice() else {
                panic!("{tool:?} {}: one warning, not {notes:?}", form.kind);
            };
            // The reading is stated, never an instruction: the call ran as read.
            assert!(
                note.starts_with("read ") && !note.contains("next time"),
                "{note}"
            );
            if let Some(field) = &form.field {
                assert!(
                    note.starts_with(&format!("read queries[0]: {field}")),
                    "{note}"
                );
            }
            let tools = read.entry(form.kind).or_default();
            if !tools.contains(&tool) {
                tools.push(tool);
            }
        }
    }
    for kind in [
        "bare row",
        "bare list",
        "one row for queries",
        "queries as JSON text",
        "one value for a list",
        "JSON text",
        "literal in another case",
    ] {
        assert!(
            read.get(kind).is_some_and(|tools| tools.len() > 1),
            "{kind} is read on several tools: {read:?}"
        );
    }
}

/// D4: readings of one row are independent. The fields of an example that
/// are each read alone in another letter case are read together when all
/// are sent so, in one warning that names each, on every tool that has two
/// such fields: a discriminator's case is read before its form judges the
/// rest.
#[test]
fn every_independent_reading_of_one_row_is_made_together() {
    let mut tools = Vec::new();
    for (tool, row) in examples() {
        if bills(tool) {
            continue;
        }
        let cased: Vec<Form> = forms(&row)
            .into_iter()
            .filter(|form| form.kind == "literal in another case")
            .filter(|form| {
                form.sent != request(&row)
                    && normalized_readings(tool.as_str(), form.sent.clone()).0 == request(&row)
            })
            .collect();
        if cased.len() < 2 {
            continue;
        }
        let mut all = row.clone();
        for form in &cased {
            let field = form.field.as_deref().expect("a field form");
            all[field] = form.sent["queries"][0][field].clone();
        }
        let (input, notes) = normalized_readings(tool.as_str(), request(&all));
        assert_eq!(input, request(&row), "{tool:?}: {all}");
        let [note] = notes.as_slice() else {
            panic!("{tool:?}: one warning, not {notes:?}");
        };
        for form in &cased {
            let field = form.field.as_deref().expect("a field form");
            assert!(note.contains(field), "{tool:?} names {field}: {note}");
        }
        if !tools.contains(&tool) {
            tools.push(tool);
        }
    }
    assert!(tools.len() > 1, "several tools read two fields: {tools:?}");
}

/// An object sent without a member its schema states a default for runs
/// with that default, and the call says the form: held for every row field
/// whose schema states an object default, on each example whose form takes
/// the field.
#[test]
fn a_missing_member_is_filled_from_its_declared_default() {
    use crate::contracts::schema_facts::{child_schemas, property_names, schema_keyword};
    let mut filled: Vec<(ToolId, &str)> = Vec::new();
    for (tool, row) in examples() {
        if bills(tool) {
            continue;
        }
        let contract = crate::contracts::tool_contract(tool).expect("contract");
        let query = &contract["querySchema"];
        let defs = &query["$defs"];
        for field in property_names(query, defs) {
            let defaults: Vec<&Value> = child_schemas(query, defs, field)
                .into_iter()
                .filter_map(|schema| schema_keyword(schema, defs, "default"))
                .collect();
            let [default, rest @ ..] = defaults.as_slice() else {
                continue;
            };
            let Some(members) = default.as_object().filter(|members| members.len() > 1) else {
                continue;
            };
            let whole = request(&with(&row, field, (*default).clone()));
            if rest.iter().any(|other| other != default) || !issues(tool, &whole).is_empty() {
                continue;
            }
            for member in members.keys() {
                let mut partial = members.clone();
                partial.remove(member);
                let sent = request(&with(&row, field, Value::Object(partial)));
                if issues(tool, &sent).is_empty() {
                    continue;
                }
                let (input, notes) = normalized_readings(tool.as_str(), sent.clone());
                assert_eq!(input, whole, "{tool:?} {sent}");
                assert!(
                    matches!(notes.as_slice(), [note] if note.contains(field) && note.contains(member.as_str())),
                    "{notes:?}"
                );
                if !filled.contains(&(tool, field)) {
                    filled.push((tool, field));
                }
            }
        }
    }
    assert!(
        filled.len() > 1,
        "several fields state an object default: {filled:?}"
    );
}

/// One warning per call: a reading is said once for the rows that share it,
/// and a value read from its JSON text is named, not quoted.
#[test]
fn a_reading_is_said_once_for_the_rows_that_share_it() {
    let terms: Vec<String> = (0..40).map(|at| format!("term{at}")).collect();
    let row = json!({"path": ".", "matchString": ["x"], "exclude": json!(terms).to_string()});
    let sent = json!({"queries": [row, {"path": ".", "matchString": ["y"]}, row]});
    let (input, notes) = normalized_readings("localSearch", sent);
    assert_eq!(input["queries"][0]["exclude"], json!(terms));
    let [note] = notes.as_slice() else {
        panic!("one warning: {notes:?}");
    };
    assert_eq!(note, "read queries[0,2]: exclude from JSON text");
    assert!(
        note.chars().count() < json!(terms).to_string().chars().count(),
        "the value is quoted by its ends: {note}"
    );
}

/// Fields of one row read the same way are named together, once per way:
/// the warning lists each field, never repeats the reading.
#[test]
fn fields_read_the_same_way_share_one_reading() {
    let row = json!({
        "path": ".",
        "matchString": "x",
        "include": json!(["*.rs"]).to_string(),
        "exclude": json!(["target"]).to_string()
    });
    let (input, notes) = normalized_readings("localSearch", json!({"queries": [row]}));
    assert_eq!(input["queries"][0]["include"], json!(["*.rs"]));
    assert_eq!(input["queries"][0]["matchString"], json!(["x"]));
    let [note] = notes.as_slice() else {
        panic!("one warning: {notes:?}");
    };
    assert_eq!(note.matches("from JSON text").count(), 1, "{note}");
    assert!(
        note.contains("include") && note.contains("exclude"),
        "{note}"
    );
    assert!(note.contains("matchString as a list"), "{note}");
}

/// A `[start, end]` pair where a range `{startLine, endLine}` is declared
/// reads as that range, its first item the start (the contract's `x-order`
/// names the lower); an object without a declared order (`contextLines`)
/// has no such reading.
#[test]
fn a_pair_reads_as_the_ordered_object() {
    let sent = json!({"queries": [{"path": "a.rs", "ranges": [[3, 9], {"startLine": 20, "endLine": 22}]}]});
    let (input, notes) = normalized_readings("localFetch", sent);
    assert_eq!(
        input["queries"][0]["ranges"],
        json!([{"startLine": 3, "endLine": 9}, {"startLine": 20, "endLine": 22}])
    );
    let [note] = notes.as_slice() else {
        panic!("one warning: {notes:?}");
    };
    assert!(note.contains("ranges[0]"), "{note}");
    let context =
        json!({"queries": [{"path": "a.rs", "matchString": ["x"], "contextLines": [2, 4]}]});
    let (input, _) = normalized_readings("localFetch", context.clone());
    assert_eq!(input, context, "contextLines declares no order");
}

/// A regex item that compiles only with an unpaired bracket read as text is
/// read so, and the call says how; one that fails for another reason, and
/// a literal row, stay as sent.
#[test]
fn an_unpaired_bracket_in_a_regex_reads_as_text() {
    let sent = json!({"queries": [{"path": "a.py", "regexEngine": "rust", "matchString": ["^class ", "def route(", "def map[|def imap"]}]});
    let (input, notes) = normalized_readings("localFetch", sent);
    assert_eq!(
        input["queries"][0]["matchString"],
        json!(["^class ", "def route\\(", "def map\\[|def imap"])
    );
    let [note] = notes.as_slice() else {
        panic!("one warning: {notes:?}");
    };
    assert!(note.contains("matchString[1] as `def route\\(`"), "{note}");
    for kept in [
        json!({"path": "a.py", "regexEngine": "rust", "matchString": ["(?:x"]}),
        json!({"path": "a.py", "matchString": ["def route("]}),
    ] {
        let sent = json!({"queries": [kept]});
        let (input, notes) = normalized_readings("localFetch", sent.clone());
        assert_eq!(input, sent);
        assert!(notes.is_empty(), "{notes:?}");
    }
}

/// A cursor sent in the one row of `queries` reads as the cursor call, and
/// the call says so; the row's other fields stand beside the cursor, for the
/// saved query to read. A cursor among several rows stays for validation to
/// refuse.
#[test]
fn a_cursor_in_the_one_row_reads_as_the_cursor_call() {
    let (input, notes) =
        normalized_readings("securityCheck", json!({"queries": [{"cursor": "issued"}]}));
    assert_eq!(input, json!({"cursor": "issued"}));
    let [note] = notes.as_slice() else {
        panic!("one warning: {notes:?}");
    };
    assert!(note.starts_with("read queries[0].cursor"), "{note}");
    let (input, _) = normalized_readings(
        "securityCheck",
        json!({"queries": [{"operation": "package", "view": "details", "cursor": "issued"}]}),
    );
    assert_eq!(
        input,
        json!({"operation": "package", "view": "details", "cursor": "issued"})
    );
    let kept = json!({"queries": [{"cursor": "issued"}, {"cursor": "other"}]});
    let (input, _) = normalized_readings("localSearch", kept.clone());
    assert_eq!(input, kept);
}

/// What a call states: a reading that changes no value or meaning (the
/// envelope, JSON text, a one-item list, an integral float, an ordered pair,
/// a cursor in its row) is made silently; a regex bracket read as text
/// changes the pattern, so the call says so.
#[test]
fn only_a_reading_that_changes_meaning_is_stated() {
    for (tool, sent) in [
        (
            "localSearch",
            json!({"path": ".", "matchString": "x", "exclude": "[\"target\"]"}),
        ),
        ("localFetch", json!({"path": "a.rs", "ranges": [[3, 9]]})),
        (
            "localFetch",
            json!({"queries": [{"path": "a.rs", "limit": 3.0}]}),
        ),
        ("securityCheck", json!({"queries": [{"cursor": "issued"}]})),
    ] {
        let (read, said) = normalized_input(tool, sent.clone());
        assert_ne!(read, sent, "{tool}: read");
        assert!(said.is_empty(), "{tool}: {said:?}");
    }
    let (_, said) = normalized_input(
        "localFetch",
        json!({"path": "a.py", "regexEngine": "rust", "matchString": ["def route("]}),
    );
    let [note] = said.as_slice() else {
        panic!("one warning: {said:?}");
    };
    assert!(note.contains("matchString[0] as `def route\\(`"), "{note}");
}

/// The brief fields a tool other than clasify does not declare are dropped,
/// beside the request or in a row, and the call says so; clasify keeps them.
#[test]
fn a_brief_field_is_dropped_and_said_outside_clasify() {
    let sent = json!({"mainGoal": "g", "queries": [{"path": ".", "matchString": ["x"], "mainGoal": "g", "reasoning": "r"}]});
    let (read, said) = normalized_input("localSearch", sent);
    assert!(read.get("mainGoal").is_none(), "{read}");
    assert!(
        read["queries"][0].get("mainGoal").is_none()
            && read["queries"][0].get("reasoning").is_none(),
        "{read}"
    );
    let [note] = said.as_slice() else {
        panic!("one warning: {said:?}");
    };
    assert!(note.contains("mainGoal, reasoning dropped"), "{note}");
}
