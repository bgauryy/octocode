//! Row fields agents send for a declared one. A guess with exactly one reading
//! is read as that field, and the call says so; one with several readings is
//! left for validation, whose error names the field to use. The schema stays
//! the full input: these are repairs of what was sent, never aliases it lists.
use crate::id::ToolId;
use serde_json::{Map, Value, json};

/// What one sent field means.
enum Reading {
    /// The declared field and the value it takes.
    Field(&'static str, Value),
    /// More than one value or field fits: validation rejects it, naming this
    /// field.
    Ambiguous,
}

/// A field sent for `field` of the tools in `tools`, read by `read` from its
/// value.
struct Guess {
    tools: &'static [ToolId],
    sent: &'static str,
    field: &'static str,
    read: fn(&Value) -> Reading,
}

const SEARCH: &[ToolId] = &[ToolId::LocalSearch];
const READERS: &[ToolId] = &[ToolId::LocalFetch, ToolId::GhGetFileContent];

const GUESSES: &[Guess] = &[
    Guess {
        tools: SEARCH,
        sent: "pattern",
        field: "matchString",
        read: |value| Reading::Field("matchString", value.clone()),
    },
    Guess {
        tools: SEARCH,
        sent: "mode",
        field: "regexEngine",
        read: |value| match value.as_str() {
            Some(engine @ ("literal" | "rust" | "pcre2")) => {
                Reading::Field("regexEngine", json!(engine))
            }
            // A regex runs in `rust` or `pcre2`: two readings.
            _ => Reading::Ambiguous,
        },
    },
    Guess {
        tools: SEARCH,
        sent: "literal",
        field: "regexEngine",
        read: |value| match value {
            Value::Bool(true) => Reading::Field("regexEngine", json!("literal")),
            _ => Reading::Ambiguous,
        },
    },
    Guess {
        tools: SEARCH,
        sent: "filesOnly",
        field: "view",
        read: |value| when_true(value, "view", "files"),
    },
    Guess {
        tools: SEARCH,
        sent: "countOnly",
        field: "view",
        read: |value| when_true(value, "view", "countMatches"),
    },
    Guess {
        tools: SEARCH,
        sent: "perFileCount",
        field: "view",
        read: |value| when_true(value, "view", "countMatches"),
    },
    Guess {
        tools: SEARCH,
        sent: "excludeDir",
        field: "exclude",
        read: |value| match value {
            Value::String(_) => Reading::Field("exclude", json!([value])),
            Value::Array(items) if items.iter().all(Value::is_string) => {
                Reading::Field("exclude", value.clone())
            }
            _ => Reading::Ambiguous,
        },
    },
    Guess {
        tools: &[ToolId::GhSearchHistory],
        sent: "type",
        field: "operation",
        read: |value| Reading::Field("operation", value.clone()),
    },
    Guess {
        tools: &[ToolId::GhSearchCode],
        sent: "match",
        field: "matchIn",
        read: |value| match value {
            Value::String(_) => Reading::Field("matchIn", json!([value])),
            Value::Array(_) => Reading::Field("matchIn", value.clone()),
            _ => Reading::Ambiguous,
        },
    },
    // `startLine` and `endLine` beside `path` read as one range below.
    Guess {
        tools: READERS,
        sent: "startLine",
        field: "ranges",
        read: |_| Reading::Ambiguous,
    },
    Guess {
        tools: READERS,
        sent: "endLine",
        field: "ranges",
        read: |_| Reading::Ambiguous,
    },
];

fn when_true(value: &Value, field: &'static str, meant: &str) -> Reading {
    if value == &Value::Bool(true) {
        Reading::Field(field, json!(meant))
    } else {
        Reading::Ambiguous
    }
}

/// The declared field a sent `field` of `tool` stands for, when it is a known
/// guess: what an unknown-field error tells the caller to use.
pub(crate) fn meant_field(tool: &str, field: &str) -> Option<&'static str> {
    let id = ToolId::from_name(tool)?;
    GUESSES
        .iter()
        .find(|guess| guess.sent == field && guess.tools.contains(&id))
        .map(|guess| guess.field)
}

/// Read each row's guesses that have one reading as their declared fields;
/// what was read, one note per field.
pub(super) fn read(id: ToolId, input: &mut Value) -> Vec<String> {
    let Some(rows) = input.get_mut("queries").and_then(Value::as_array_mut) else {
        return Vec::new();
    };
    let mut notes = Vec::new();
    for (index, row) in rows.iter_mut().enumerate() {
        let Some(row) = row.as_object_mut() else {
            continue;
        };
        if READERS.contains(&id) {
            notes.extend(read_range(row).map(|note| format!("queries[{index}]: {note}")));
            notes.extend(read_pairs(row).map(|note| format!("queries[{index}]: {note}")));
        }
        for guess in GUESSES.iter().filter(|guess| guess.tools.contains(&id)) {
            let Some(value) = row.get(guess.sent) else {
                continue;
            };
            let Reading::Field(field, meant) = (guess.read)(value) else {
                continue;
            };
            let merged = match (row.get(field), &meant) {
                (None, _) => meant,
                // Paths to skip add up: both lists are one reading.
                (Some(Value::Array(have)), Value::Array(more)) if field == "exclude" => {
                    Value::Array(have.iter().chain(more).cloned().collect())
                }
                // The row sets the field too: two values, no one reading.
                (Some(_), _) => continue,
            };
            row.remove(guess.sent);
            row.insert(field.to_owned(), merged);
            notes.push(format!(
                "queries[{index}]: read `{}` as `{field}`; send `{field}` next time.",
                guess.sent
            ));
        }
    }
    notes
}

/// `startLine` with `endLine` beside a reader's path: the one range they
/// bound. Either alone, or beside `ranges`, has no one reading.
fn read_range(row: &mut Map<String, Value>) -> Option<String> {
    if row.contains_key("ranges") {
        return None;
    }
    let (Some(start), Some(end)) = (
        row.get("startLine").filter(|line| line.is_u64()).cloned(),
        row.get("endLine").filter(|line| line.is_u64()).cloned(),
    ) else {
        return None;
    };
    row.remove("startLine");
    row.remove("endLine");
    row.insert(
        "ranges".to_owned(),
        json!([{"startLine": start, "endLine": end}]),
    );
    Some("read `startLine`/`endLine` as `ranges:[{startLine,endLine}]`.".to_owned())
}

/// `ranges` as `[[start, end], …]`: each pair one range. A flat list of
/// numbers (`[2, 4]`: lines 2 and 4, or 2 through 4) has no one reading.
fn read_pairs(row: &mut Map<String, Value>) -> Option<String> {
    let items = row.get("ranges")?.as_array()?;
    let pairs = items
        .iter()
        .map(|item| match item.as_array()?.as_slice() {
            [start, end] if start.is_u64() && end.is_u64() => {
                Some(json!({"startLine": start, "endLine": end}))
            }
            _ => None,
        })
        .collect::<Option<Vec<_>>>()
        .filter(|pairs| !pairs.is_empty())?;
    row.insert("ranges".to_owned(), Value::Array(pairs));
    Some("read each `[start, end]` pair of `ranges` as `{startLine, endLine}`.".to_owned())
}
