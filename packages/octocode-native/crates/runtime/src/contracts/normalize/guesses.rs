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
/// value and the row it sits in.
struct Guess {
    tools: &'static [ToolId],
    /// The row field whose object holds `sent`: empty for the row itself.
    within: &'static str,
    sent: &'static str,
    field: &'static str,
    /// What `field` takes, where the sent name promises something else: said
    /// with the field when the guess is refused.
    takes: Option<&'static str>,
    read: fn(&Value, &Map<String, Value>) -> Reading,
}

const SEARCH: &[ToolId] = &[ToolId::LocalSearch];
const READERS: &[ToolId] = &[ToolId::LocalFetch, ToolId::GhGetFileContent];

const GUESSES: &[Guess] = &[
    Guess {
        within: "",
        tools: SEARCH,
        sent: "mode",
        field: "regexEngine",
        takes: Some("literal, rust or pcre2 (a files-only listing is view:\"files\")"),
        read: |value, _| match value.as_str() {
            Some(engine @ ("literal" | "rust" | "pcre2")) => {
                Reading::Field("regexEngine", json!(engine))
            }
            // The former search mode that listed matching files only.
            Some("discovery") => Reading::Field("view", json!("files")),
            // A regex runs in `rust` or `pcre2`: two readings.
            _ => Reading::Ambiguous,
        },
    },
    Guess {
        within: "",
        tools: SEARCH,
        sent: "literal",
        field: "regexEngine",
        takes: None,
        read: |value, _| match value {
            Value::Bool(true) => Reading::Field("regexEngine", json!("literal")),
            _ => Reading::Ambiguous,
        },
    },
    // After the guesses that set `regexEngine`: an engine in the row says
    // how the value is read.
    Guess {
        within: "",
        tools: SEARCH,
        sent: "pattern",
        field: "matchString",
        takes: Some("literal text, or a regex with regexEngine:\"rust\""),
        read: |value, row| {
            let terms: Vec<&str> = match value {
                Value::String(term) => vec![term],
                Value::Array(terms) => terms.iter().filter_map(Value::as_str).collect(),
                _ => Vec::new(),
            };
            // `matchString` is literal text unless the row names an engine;
            // `pattern` names a regex in most tools. Terms that also read as
            // a regex have both readings.
            if !row.contains_key("regexEngine") && reads_as_regex(&terms) {
                Reading::Ambiguous
            } else {
                Reading::Field("matchString", value.clone())
            }
        },
    },
    Guess {
        within: "",
        tools: SEARCH,
        sent: "filesOnly",
        field: "view",
        takes: None,
        read: |value, _| when_true(value, "view", "files"),
    },
    Guess {
        within: "",
        tools: SEARCH,
        sent: "countOnly",
        field: "view",
        takes: None,
        read: |value, _| when_true(value, "view", "countMatches"),
    },
    Guess {
        within: "",
        tools: SEARCH,
        sent: "perFileCount",
        field: "view",
        takes: None,
        read: |value, _| when_true(value, "view", "countMatches"),
    },
    Guess {
        within: "",
        tools: SEARCH,
        sent: "excludeDir",
        field: "exclude",
        takes: None,
        read: |value, _| match value {
            Value::String(_) => Reading::Field("exclude", json!([value])),
            Value::Array(items) if items.iter().all(Value::is_string) => {
                Reading::Field("exclude", value.clone())
            }
            _ => Reading::Ambiguous,
        },
    },
    Guess {
        within: "",
        tools: &[ToolId::GhSearchHistory],
        sent: "type",
        field: "operation",
        takes: None,
        read: |value, _| Reading::Field("operation", value.clone()),
    },
    Guess {
        within: "",
        tools: &[ToolId::GhSearchCode],
        sent: "match",
        field: "matchIn",
        takes: None,
        read: |value, _| match value {
            Value::String(_) => Reading::Field("matchIn", json!([value])),
            Value::Array(_) => Reading::Field("matchIn", value.clone()),
            _ => Reading::Ambiguous,
        },
    },
    // `startLine` and `endLine` beside `path` read as one range below.
    Guess {
        within: "",
        tools: READERS,
        sent: "startLine",
        field: "ranges",
        takes: None,
        read: |_, _| Reading::Ambiguous,
    },
    Guess {
        within: "",
        tools: READERS,
        sent: "endLine",
        field: "ranges",
        takes: None,
        read: |_, _| Reading::Ambiguous,
    },
    // Former names of renamed fields: refused, the error names the field.
    Guess {
        within: "",
        tools: &[ToolId::StructureSearch],
        sent: "sizeBytes",
        field: "bytes",
        takes: None,
        read: |_, _| Reading::Ambiguous,
    },
    Guess {
        within: "",
        tools: &[ToolId::LspSearch],
        sent: "symbolQuery",
        field: "symbolFuzzy",
        takes: None,
        read: |_, _| Reading::Ambiguous,
    },
    Guess {
        within: "",
        tools: &[ToolId::GraphQuery],
        sent: "name",
        field: "symbolName",
        takes: None,
        read: |_, _| Reading::Ambiguous,
    },
    // Filter names follow the output fields they filter.
    Guess {
        within: "filters",
        tools: &[ToolId::GhSearchRepo],
        sent: "forkCount",
        field: "forks",
        takes: None,
        read: |_, _| Reading::Ambiguous,
    },
    Guess {
        within: "filters",
        tools: &[ToolId::GhSearchHistory],
        sent: "commentCount",
        field: "commentsCount",
        takes: None,
        read: |_, _| Reading::Ambiguous,
    },
];

fn when_true(value: &Value, field: &'static str, meant: &str) -> Reading {
    if value == &Value::Bool(true) {
        Reading::Field(field, json!(meant))
    } else {
        Reading::Ambiguous
    }
}

/// Whether literal `terms` also read as a regex: some term holds regex
/// syntax and every term compiles. Text without regex syntax, or with syntax
/// that does not compile (`call(`), has the literal reading alone.
pub(crate) fn reads_as_regex(terms: &[&str]) -> bool {
    terms.iter().any(|term| regex::escape(term) != *term)
        && terms.iter().all(|term| regex::Regex::new(term).is_ok())
}

/// The declared field a sent `field` of `tool` stands for, when it is a known
/// guess, and what that field takes when the sent name promises something
/// else: what an unknown-field error tells the caller to use. `within` names
/// the row field whose object holds `field`, empty for the row itself.
pub(crate) fn meant_field(
    tool: &str,
    within: &str,
    field: &str,
) -> Option<(&'static str, Option<&'static str>)> {
    let id = ToolId::from_name(tool)?;
    GUESSES
        .iter()
        .find(|guess| guess.within == within && guess.sent == field && guess.tools.contains(&id))
        .map(|guess| (guess.field, guess.takes))
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
        notes.extend(
            read_tool_queries(id, row)
                .into_iter()
                .map(|note| format!("queries[{index}]: {note}")),
        );
        for guess in GUESSES
            .iter()
            .filter(|guess| guess.within.is_empty() && guess.tools.contains(&id))
        {
            let Some(value) = row.get(guess.sent) else {
                continue;
            };
            let Reading::Field(field, meant) = (guess.read)(value, row) else {
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
            // The repair names its value when the field takes another than
            // was sent (`filesOnly: true` is `view: "files"`).
            let repair = if &merged == value {
                format!("`{field}`")
            } else {
                format!("`{field}: {merged}`")
            };
            row.remove(guess.sent);
            row.insert(field.to_owned(), merged);
            notes.push(format!(
                "queries[{index}]: read `{}` as {repair}; send {repair} next time.",
                guess.sent
            ));
        }
    }
    notes
}

/// Each `{tool, query}` field of the row sent as its query alone
/// (`source:{path}`), read as `{tool, query}` of the one tool whose closed
/// query form takes it: every sent field declared, none it requires missing.
/// A form naming several tools (a search query, checked against the named
/// tool's own schema) names no one tool, and a value sending a field of the
/// union itself (`tool`, `query`, `url`) has chosen its form: both are left
/// for validation. One note per field read.
fn read_tool_queries(id: ToolId, row: &mut Map<String, Value>) -> Vec<String> {
    let Ok(tool) = super::super::tool_contract(id) else {
        return Vec::new();
    };
    let schema = &tool["querySchema"];
    let mut notes = Vec::new();
    wrap_tool_queries(row, &[schema], &schema["$defs"], "", &mut notes);
    notes
}

/// [`read_tool_queries`] over the fields of `object`, whose schemas are
/// `schemas`, and the objects and lists nested in them.
fn wrap_tool_queries(
    object: &mut Map<String, Value>,
    schemas: &[&Value],
    defs: &Value,
    at: &str,
    notes: &mut Vec<String>,
) {
    use super::super::schema_facts::{child_schemas, item_schemas};
    for (key, value) in object.iter_mut() {
        let children = schemas
            .iter()
            .flat_map(|schema| child_schemas(schema, defs, key))
            .collect::<Vec<_>>();
        if children.is_empty() {
            continue;
        }
        let label = if at.is_empty() {
            key.clone()
        } else {
            format!("{at}.{key}")
        };
        if let Some(note) = wrap_tool_query(value, &children, defs, &label) {
            notes.push(note);
            continue;
        }
        match value {
            Value::Object(inner) => wrap_tool_queries(inner, &children, defs, &label, notes),
            Value::Array(items) => {
                let item_schemas = children
                    .iter()
                    .flat_map(|schema| item_schemas(schema, defs))
                    .collect::<Vec<_>>();
                if item_schemas.is_empty() {
                    continue;
                }
                for (index, item) in items.iter_mut().enumerate() {
                    let label = format!("{label}[{index}]");
                    if let Some(note) = wrap_tool_query(item, &item_schemas, defs, &label) {
                        notes.push(note);
                    } else if let Value::Object(inner) = item {
                        wrap_tool_queries(inner, &item_schemas, defs, &label, notes);
                    }
                }
            }
            _ => {}
        }
    }
}

/// `value` as `{tool, query}` when its fields are the query of exactly one
/// tool among the forms of `schemas`; the note that says so.
fn wrap_tool_query(
    value: &mut Value,
    schemas: &[&Value],
    defs: &Value,
    label: &str,
) -> Option<String> {
    let sent = value.as_object().filter(|object| !object.is_empty())?;
    let mut forms = Vec::new();
    for schema in schemas {
        object_forms(schema, defs, &mut forms);
    }
    let mut tools: Vec<&str> = Vec::new();
    for form in forms {
        let Some(properties) = form.get("properties").and_then(Value::as_object) else {
            continue;
        };
        if sent.keys().any(|name| properties.contains_key(name)) {
            return None;
        }
        let (Some(tool), Some(query)) = (properties.get("tool"), properties.get("query")) else {
            continue;
        };
        let Some(tool) = single_name(super::super::schema_facts::resolve_ref(tool, defs)) else {
            continue;
        };
        let query = super::super::schema_facts::resolve_ref(query, defs);
        let declared = query.get("properties").and_then(Value::as_object);
        let takes = query.get("additionalProperties") == Some(&Value::Bool(false))
            && sent
                .keys()
                .all(|name| declared.is_some_and(|declared| declared.contains_key(name)))
            && query
                .get("required")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .all(|name| sent.contains_key(name));
        if takes && !tools.contains(&tool) {
            tools.push(tool);
        }
    }
    let [tool] = tools.as_slice() else {
        return None;
    };
    let fields = sent.keys().cloned().collect::<Vec<_>>().join(",");
    let query = value.take();
    *value = json!({"tool": tool, "query": query});
    Some(format!(
        "read `{label}:{{{fields}}}` as `{label}:{{tool:\"{tool}\",query:{{{fields}}}}}`; send that next time."
    ))
}

/// The object forms `schema` stands for, through references and unions.
fn object_forms<'a>(schema: &'a Value, defs: &'a Value, forms: &mut Vec<&'a Value>) {
    let schema = super::super::schema_facts::resolve_ref(schema, defs);
    let nested = ["anyOf", "oneOf"]
        .iter()
        .find_map(|keyword| schema.get(*keyword).and_then(Value::as_array));
    match nested {
        Some(branches) => {
            for branch in branches {
                object_forms(branch, defs, forms);
            }
        }
        None => forms.push(schema),
    }
}

/// The one string a `const`, or a one-value `enum`, pins a field to.
fn single_name(schema: &Value) -> Option<&str> {
    schema.get("const").and_then(Value::as_str).or_else(|| {
        match schema.get("enum").and_then(Value::as_array)?.as_slice() {
            [only] => only.as_str(),
            _ => None,
        }
    })
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
    Some(
        "read `startLine`/`endLine` as `ranges:[{startLine,endLine}]`; send `ranges` next time."
            .to_owned(),
    )
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
    Some(
        "read each `[start, end]` pair of `ranges` as `{startLine, endLine}`; send `ranges:[{startLine,endLine}]` next time."
            .to_owned(),
    )
}
