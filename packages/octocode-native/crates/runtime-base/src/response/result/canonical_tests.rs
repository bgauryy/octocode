//! [`super::canonical_row`] over every tool's contract: a row stating each
//! defaulted field at its default parses to the same query as its canonical
//! form, so every field it drops re-applies as the same value.
#![expect(
    clippy::expect_used,
    reason = "fixtures for tests, compiled only for them (`test`, `test-support`): a broken fixture fails its test at once"
)]
#![cfg_attr(
    not(test),
    expect(
        clippy::cast_possible_truncation,
        reason = "schema bounds of a sampled row: small integers"
    )
)]
use crate::contracts::{resolve_ref, schema_keyword};
use crate::id::ToolId;
use serde_json::{Map, Value, json};

/// Strings tried, in order, for a string schema: the first one its length
/// bounds, patterns and format admit is sampled. Each covers a family of
/// contract patterns (names, numbers, Rust paths, Maven coordinates, file
/// modes, hex digests, dates, URLs).
const STRINGS: &[&str] = &[
    "a",
    "1",
    "a.rs",
    "a:a",
    "755",
    "0123456789abcdef",
    "0123456789abcdef0123456789abcdef01234567",
    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    "2024-01-01",
    "2024-01-01T00:00:00Z",
    "https://example.com/a",
];

/// `schema` and every `allOf` member below it, each resolved: their
/// constraints all hold at once.
fn parts<'a>(schema: &'a Value, defs: &'a Value) -> Vec<&'a Value> {
    let mut found = vec![schema];
    let mut pending = vec![resolve_ref(schema, defs)];
    while let Some(part) = pending.pop() {
        found.push(part);
        for member in part["allOf"].as_array().into_iter().flatten() {
            pending.push(resolve_ref(member, defs));
        }
    }
    found
}

fn keyword<'a>(parts: &[&'a Value], defs: &'a Value, name: &str) -> Option<&'a Value> {
    parts
        .iter()
        .find_map(|part| schema_keyword(part, defs, name))
}

/// A property schema that no value satisfies (`not: {}`): the field is forbidden.
fn forbidden(schema: &Value, defs: &Value) -> bool {
    parts(schema, defs)
        .iter()
        .any(|part| part["not"].as_object().is_some_and(Map::is_empty))
}

/// A value `schema` admits, or `None` when the sampler finds none: its
/// const, first enum member, default, first satisfiable `anyOf`/`oneOf`
/// branch, or the smallest value of its type within its bounds.
fn sample(schema: &Value, defs: &Value) -> Option<Value> {
    if forbidden(schema, defs) {
        return None;
    }
    let parts = parts(schema, defs);
    let get = |name| keyword(&parts, defs, name);
    if let Some(value) = get("const").or_else(|| get("enum").and_then(|values| values.get(0))) {
        return Some(value.clone());
    }
    if let Some(value) = get("default") {
        return Some(value.clone());
    }
    if let Some(branches) = ["anyOf", "oneOf"]
        .iter()
        .find_map(|name| get(name).and_then(Value::as_array))
    {
        return branches.iter().find_map(|branch| sample(branch, defs));
    }
    let kind = match get("type") {
        Some(Value::Array(kinds)) => kinds
            .iter()
            .filter_map(Value::as_str)
            .find(|kind| *kind != "null"),
        Some(kind) => kind.as_str(),
        None if get("properties").is_some() => Some("object"),
        None => None,
    };
    let bound = |name| get(name).and_then(Value::as_u64);
    match kind {
        Some("string") => {
            let patterns = parts
                .iter()
                .filter_map(|part| part["pattern"].as_str())
                .map(|pattern| {
                    regress::Regex::with_flags(pattern, "u").expect("contract pattern compiles")
                })
                .collect::<Vec<_>>();
            STRINGS
                .iter()
                .find(|text| {
                    let length = text.chars().count() as u64;
                    bound("minLength").is_none_or(|min| length >= min)
                        && bound("maxLength").is_none_or(|max| length <= max)
                        && patterns.iter().all(|pattern| pattern.find(text).is_some())
                        && (get("format").and_then(Value::as_str) != Some("uri")
                            || text.starts_with("https:"))
                })
                .map(|text| json!(text))
        }
        Some("integer" | "number") => {
            let low = get("minimum")
                .and_then(Value::as_i64)
                .or_else(|| {
                    get("exclusiveMinimum")
                        .and_then(Value::as_i64)
                        .map(|bound| bound + 1)
                })
                .unwrap_or(1);
            let high = get("maximum").and_then(Value::as_i64).unwrap_or(i64::MAX);
            (low <= high).then(|| json!(low))
        }
        Some("boolean") => Some(json!(false)),
        Some("array") => {
            let item = sample(get("items").unwrap_or(&Value::Null), defs)?;
            Some(Value::Array(vec![
                item;
                bound("minItems").unwrap_or(1).max(1)
                    as usize
            ]))
        }
        Some("object") => object(&parts, defs, false),
        _ => Some(json!("a")),
    }
}

/// The required properties of an object schema (its `parts`), plus what
/// `minProperties` and its unconditional `x-checks` ask for; with
/// `defaults`, every optional property that declares a default, stated at
/// that default. `None` when a required property has no sample.
fn object(parts: &[&Value], defs: &Value, defaults: bool) -> Option<Value> {
    let mut properties = Map::new();
    let mut required = Vec::new();
    for part in parts {
        for (field, schema) in part["properties"].as_object().into_iter().flatten() {
            properties
                .entry(field.clone())
                .or_insert_with(|| schema.clone());
        }
        required.extend(
            part["required"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str),
        );
    }
    let mut row = Map::new();
    for (field, schema) in &properties {
        if required.contains(&field.as_str()) {
            row.insert(field.clone(), sample(schema, defs)?);
        } else if defaults
            && !forbidden(schema, defs)
            && let Some(default) = schema_keyword(schema, defs, "default")
        {
            row.insert(field.clone(), default.clone());
        }
    }
    // A check that always applies and asks for one of several fields.
    let checks = parts
        .iter()
        .filter_map(|part| part["x-checks"].as_array())
        .flatten();
    for check in checks.filter(|check| check.get("when").is_none() && check.get("each").is_none()) {
        let Some(branches) = check["assert"]["anyOf"].as_array() else {
            continue;
        };
        let asked = |branch: &Value| -> Vec<String> {
            branch["required"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        };
        if branches
            .iter()
            .any(|branch| asked(branch).iter().all(|field| row.contains_key(field)))
        {
            continue;
        }
        let added = branches.iter().find_map(|branch| {
            asked(branch)
                .into_iter()
                .map(|field| Some((field.clone(), sample(properties.get(&field)?, defs)?)))
                .collect::<Option<Vec<_>>>()
        })?;
        row.extend(added);
    }
    let least = keyword(parts, defs, "minProperties")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    for (field, schema) in &properties {
        if row.len() >= least {
            break;
        }
        if !row.contains_key(field)
            && let Some(value) = sample(schema, defs)
        {
            row.insert(field.clone(), value);
        }
    }
    if row.len() < least {
        let names = keyword(parts, defs, "propertyNames");
        let entry =
            keyword(parts, defs, "additionalProperties").filter(|schema| schema.is_object())?;
        let name = names.map_or(Some(json!("a")), |names| sample(names, defs))?;
        row.insert(name.as_str()?.to_owned(), sample(entry, defs)?);
    }
    Some(Value::Object(row))
}

/// The leaf forms of a query schema: each `oneOf`/`anyOf` branch, with a
/// branch that is itself a union replaced by its own branches.
fn forms<'a>(schema: &'a Value, defs: &'a Value, out: &mut Vec<&'a Value>) {
    let resolved = resolve_ref(schema, defs);
    match ["oneOf", "anyOf"]
        .iter()
        .find_map(|key| resolved[*key].as_array())
    {
        Some(branches) => branches.iter().for_each(|branch| forms(branch, defs, out)),
        None => out.push(schema),
    }
}

/// One sampled row of a tool's query variant, stated two ways: with every
/// defaulted field at its default, and without them.
pub struct Sampled {
    pub stated: Value,
    pub omitted: Value,
}

/// What the contract asks of `tool`'s variants and what the sampler built.
pub struct Variants {
    /// Leaf forms the contract's query schema declares.
    pub declared: usize,
    /// Leaf forms the sampler found no row for.
    pub skipped: Vec<String>,
    pub rows: Vec<Sampled>,
}

/// Every leaf form of `tool`'s query schema, sampled.
pub fn variants(tool: ToolId) -> Variants {
    let contract = crate::contracts::tool_contract(tool).expect("every tool has a contract");
    let defs = &contract["querySchema"]["$defs"];
    let mut forms_found = Vec::new();
    forms(&contract["querySchema"], defs, &mut forms_found);
    let mut found = Variants {
        declared: forms_found.len(),
        skipped: Vec::new(),
        rows: Vec::new(),
    };
    for variant in forms_found {
        let parts = parts(variant, defs);
        match (object(&parts, defs, true), object(&parts, defs, false)) {
            (Some(stated), Some(omitted)) => found.rows.push(Sampled { stated, omitted }),
            _ => found.skipped.push(variant.to_string()),
        }
    }
    found
}

/// `row` as `tool`'s generated query type, back as a value: the typed
/// query, with the Rust runtime's own defaults applied.
pub fn typed(tool: ToolId, row: &Value) -> Value {
    use octocode_contract_types as t;
    macro_rules! through {
        ($($id:ident => $query:ty),* $(,)?) => {
            match tool {
                $(ToolId::$id => serde_json::to_value(
                    serde_json::from_value::<$query>(row.clone()).unwrap_or_else(|error| {
                        panic!("{}: {row} is not a typed query: {error}", tool.as_str())
                    }),
                ),)*
            }
            .expect("a typed query serializes")
        };
    }
    through! {
        ArtifactSearch => t::ArtifactSearchQuery,
        AstSearch => t::AstSearchQuery,
        Clasify => t::ClasifyQuery,
        GhCloneRepo => t::GhCloneRepoQuery,
        GhGetFileContent => t::GhGetFileContentQuery,
        GhGetHistoryItem => t::GhGetHistoryItemQuery,
        GhSearchCode => t::GhSearchCodeQuery,
        GhSearchHistory => t::GhSearchHistoryQuery,
        GhSearchRepo => t::GhSearchRepoQuery,
        GhStructure => t::GhStructureQuery,
        GraphBuild => t::GraphBuildQuery,
        GraphQuery => t::GraphQueryQuery,
        LocalFetch => t::LocalFetchQuery,
        LocalSearch => t::LocalSearchQuery,
        LspSearch => t::LspSearchQuery,
        SecurityCheck => t::SecurityCheckQuery,
        StructureSearch => t::StructureSearchQuery,
    }
}

#[cfg(test)]
mod tests;
