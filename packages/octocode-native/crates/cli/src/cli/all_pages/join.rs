//! The rows of a walk's pages joined once: a whole row is appended, and a
//! row sent in `rowPart` pieces becomes one row. The join is the reader's
//! of `packages/octocode-graph/src/runtime/tools.ts` (`fragmentJoiner`), rule
//! for rule: a later piece repeats its row's identity and carries only new
//! evidence. Its marker's coordinates are in the joined row; an array on the
//! way is narrowed to the element it continues. A collection the output
//! contract declares (`collectionPaths`, through `collectionRefs`) appends
//! the new elements and merges the continued one with the piece's first;
//! any other array is repeated context and must match; the text the marker
//! points at (`text`) is appended; any other value must repeat exactly.
//! Every disagreement is an error: the pieces cannot be one row.
use octocode_runtime_base::id::ToolId;
use serde_json::{Map, Value};
use std::collections::HashMap;

/// A `rowPart` marker: which piece of how many, and the string (`text`) or
/// the collection element (`continues`) the piece continues.
struct Marker {
    part: u64,
    of: u64,
    text: Option<String>,
    continues: Option<(String, Value)>,
}

impl Marker {
    /// The marker of a row, or why it is not one.
    fn of(marker: &Value) -> Result<Self, String> {
        let text = match marker.get("text") {
            None => None,
            Some(Value::String(text)) => Some(text.clone()),
            Some(_) => return Err("Response fragment requires a JSON pointer".to_owned()),
        };
        let continues = match marker.get("continues") {
            None => None,
            Some(continues) => Some((
                continues
                    .get("array")
                    .and_then(Value::as_str)
                    .ok_or("Response fragment requires a JSON pointer")?
                    .to_owned(),
                continues.get("index").cloned().unwrap_or(Value::Null),
            )),
        };
        Ok(Self {
            // Not a count: the order check refuses it.
            part: marker.get("part").and_then(Value::as_u64).unwrap_or(0),
            of: marker.get("of").and_then(Value::as_u64).unwrap_or(0),
            text,
            continues,
        })
    }

    /// The pointer to the element the piece continues: its text pointer,
    /// else its `continues` coordinates.
    fn route(&self) -> Option<String> {
        self.text.clone().or_else(|| {
            self.continues
                .as_ref()
                .map(|(array, index)| format!("{array}/{index}"))
        })
    }
}

/// The tool's collections: the array pointers whose pieces append, and the
/// references that name one collection's items by another's path.
struct Collections {
    paths: Vec<Vec<String>>,
    refs: Vec<(Vec<String>, Vec<String>)>,
}

impl Collections {
    /// `tool`'s collections, from its output contract: the routes the
    /// response pager cuts by, as `schema <tool> --view output` lists them.
    fn of(tool: ToolId) -> Result<Self, String> {
        let contract = octocode_runtime_base::contracts::tool_contract(tool).map_err(|error| {
            format!(
                "The embedded {} contract is unreadable: {error}",
                tool.as_str()
            )
        })?;
        let pointers = |field: &str| -> Vec<Vec<String>> {
            contract[field]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(steps)
                .collect()
        };
        let refs = contract["collectionRefs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|reference| {
                Some((
                    steps(reference["path"].as_str()?),
                    steps(reference["target"].as_str()?),
                ))
            })
            .collect();
        Ok(Self {
            paths: pointers("collectionPaths"),
            refs,
        })
    }

    /// Whether the array at `path` (in a row's coordinates) is a collection:
    /// a declared path, or one a reference rewrites to a declared path.
    fn holds(&self, path: &str) -> Result<bool, String> {
        let mut parts = steps(path);
        let mut seen = Vec::new();
        loop {
            if self
                .paths
                .iter()
                .any(|pattern| pattern.len() == parts.len() && matches(pattern, &parts))
            {
                return Ok(true);
            }
            if seen.contains(&parts) {
                return Err("Cyclic collection schema reference".to_owned());
            }
            seen.push(parts.clone());
            let Some((from, target)) = self.refs.iter().find(|(from, _)| matches(from, &parts))
            else {
                return Ok(false);
            };
            if target.len() >= from.len() {
                return Err("Collection schema reference does not reduce its path".to_owned());
            }
            parts = target.iter().chain(&parts[from.len()..]).cloned().collect();
        }
    }
}

/// Whether `pattern` (`*` any one step) matches the start of `parts`.
fn matches(pattern: &[String], parts: &[String]) -> bool {
    pattern.len() <= parts.len()
        && pattern
            .iter()
            .zip(parts)
            .all(|(key, part)| key == "*" || key == part)
}

/// A JSON pointer's unescaped steps (`""` has none).
fn steps(pointer: &str) -> Vec<String> {
    pointer
        .split('/')
        .skip(1)
        .map(|key| key.replace("~1", "/").replace("~0", "~"))
        .collect()
}

fn escape(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

/// A row being joined: where it is in the joined rows and the last part it
/// holds.
#[derive(Clone, Copy)]
struct Saved {
    at: usize,
    part: u64,
    of: u64,
}

/// The joined rows of a walk, in arrival order: each whole row, and each
/// row its `rowPart` pieces make.
pub(super) struct Joiner {
    tool: ToolId,
    /// Read from the contract only once a row continues.
    collections: Option<Collections>,
    rows: Vec<Value>,
    /// Per result `index`, the row its pieces join into.
    parts: HashMap<Option<String>, Saved>,
}

impl Joiner {
    pub(super) fn new(tool: ToolId) -> Self {
        Self {
            tool,
            collections: None,
            rows: Vec::new(),
            parts: HashMap::new(),
        }
    }

    /// Add one row of a page: appended whole, or joined into the row its
    /// earlier pieces started.
    pub(super) fn add(&mut self, row: Value) -> Result<(), String> {
        let Some(marker) = row.get("rowPart") else {
            self.rows.push(row);
            return Ok(());
        };
        let marker = Marker::of(marker)?;
        let key = row.get("index").map(Value::to_string);
        // A first part after a finished row starts a new row.
        let prior = self
            .parts
            .get(&key)
            .copied()
            .filter(|saved| !(saved.part == saved.of && marker.part == 1));
        check_part_order(&marker, prior)?;
        let at = match prior {
            Some(prior) => {
                let joined = std::mem::take(&mut self.rows[prior.at]);
                self.rows[prior.at] = self.join_part(joined, row, &marker)?;
                prior.at
            }
            None => {
                let mut row = row;
                if let Some(fields) = row.as_object_mut() {
                    fields.shift_remove("rowPart");
                }
                self.rows.push(row);
                self.rows.len() - 1
            }
        };
        self.parts.insert(
            key,
            Saved {
                at,
                part: marker.part,
                of: marker.of,
            },
        );
        Ok(())
    }

    /// The joined rows; an error when a row's pieces ended before its last.
    pub(super) fn finish(self) -> Result<Vec<Value>, String> {
        if self.parts.values().any(|saved| saved.part != saved.of) {
            return Err("Response fragment ends before its final part".to_owned());
        }
        Ok(self.rows)
    }

    /// The rows joined so far, a row whose pieces are still arriving
    /// included.
    pub(super) fn rows(self) -> Vec<Value> {
        self.rows
    }

    fn join_part(&mut self, prior: Value, row: Value, marker: &Marker) -> Result<Value, String> {
        if self.collections.is_none() {
            self.collections = Some(Collections::of(self.tool)?);
        }
        let collections = self.collections.as_ref().ok_or("no collections")?;
        if let Some(text) = &marker.text {
            let (before, after) = at_pointer(&prior, &row, text)?;
            if !(before.is_some_and(Value::is_string) && after.is_some_and(Value::is_string)) {
                return Err(format!(
                    "Response fragment text {text} names no string to continue"
                ));
            }
        }
        if let Some((array, index)) = &marker.continues {
            let (before, after) = at_pointer(&prior, &row, array)?;
            let length = before.and_then(Value::as_array).map(Vec::len);
            let continues = after
                .and_then(Value::as_array)
                .is_some_and(|after| !after.is_empty());
            let at = index.as_u64().and_then(|at| usize::try_from(at).ok());
            if !(continues
                && collections.holds(array)?
                && length.zip(at).is_some_and(|(length, at)| at < length))
            {
                return Err(format!(
                    "Response fragment continues a missing collection element at {array}"
                ));
            }
        }
        // A row is an object: its parts merge field by field.
        let Value::Object(fields) = row else {
            return Err("A response fragment row is not an object".to_owned());
        };
        merge_object(
            Some(prior),
            fields,
            &Merge {
                marker,
                collections,
            },
            "",
        )
    }
}

/// The marker and the collections one piece merges by.
struct Merge<'a> {
    marker: &'a Marker,
    collections: &'a Collections,
}

fn check_part_order(marker: &Marker, prior: Option<Saved>) -> Result<(), String> {
    let Marker { part, of, .. } = *marker;
    let counted = part >= 1 && part <= of;
    let follows = prior.map_or(part == 1, |prior| part == prior.part + 1 && of == prior.of);
    if counted && follows {
        Ok(())
    } else {
        Err("Response fragment parts are missing or out of order".to_owned())
    }
}

/// The values at `pointer` in the joined row and in the piece: in the piece,
/// each array on the way is narrowed to its first element, the one it
/// continues.
fn at_pointer<'a>(
    previous: &'a Value,
    fragment: &'a Value,
    pointer: &str,
) -> Result<(Option<&'a Value>, Option<&'a Value>), String> {
    if !pointer.starts_with('/') {
        return Err("Response fragment requires a JSON pointer".to_owned());
    }
    let (mut previous, mut fragment) = (Some(previous), Some(fragment));
    for key in steps(pointer) {
        previous = previous.and_then(|value| own_field(value, &key));
        fragment = fragment.and_then(|value| match value {
            Value::Array(items) => items.first(),
            value => own_field(value, &key),
        });
    }
    Ok((previous, fragment))
}

/// A JSON value's own field: an object's member, an array's element.
fn own_field<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Object(fields) => fields.get(key),
        Value::Array(items) => items.get(key.parse::<usize>().ok()?),
        _ => None,
    }
}

fn merge_part(
    previous: Option<Value>,
    fragment: Value,
    merge: &Merge<'_>,
    path: &str,
) -> Result<Value, String> {
    if merge.marker.text.as_deref() == Some(path) {
        return match (previous, fragment) {
            (Some(Value::String(mut previous)), Value::String(fragment)) => {
                previous.push_str(&fragment);
                Ok(Value::String(previous))
            }
            _ => Err(format!(
                "Response fragment text {path} has no preceding string"
            )),
        };
    }
    match fragment {
        Value::Array(fragment) => merge_array(previous, fragment, merge, path),
        Value::Object(fragment) => merge_object(previous, fragment, merge, path),
        fragment => match previous {
            Some(previous) if previous != fragment => Err(format!(
                "Repeated response fragment context disagrees at {path}"
            )),
            _ => Ok(fragment),
        },
    }
}

/// A collection appends every new element and merges the continued one
/// with the piece's first; any other array is repeated context, which must
/// match what the first part carried.
fn merge_array(
    previous: Option<Value>,
    fragment: Vec<Value>,
    merge: &Merge<'_>,
    path: &str,
) -> Result<Value, String> {
    let Some(Value::Array(mut previous)) = previous else {
        return Ok(Value::Array(fragment));
    };
    if !merge.collections.holds(path)? {
        return if fragment.is_empty() || previous == fragment {
            Ok(Value::Array(previous))
        } else {
            Err(format!("Repeated response context disagrees at {path}"))
        };
    }
    let within = format!("{path}/");
    match merge.marker.route() {
        Some(route) if route.starts_with(&within) => {
            let at = steps(route.strip_prefix(path).unwrap_or_default())
                .first()
                .and_then(|step| step.parse::<usize>().ok())
                .filter(|at| *at < previous.len() && !fragment.is_empty())
                .ok_or_else(|| {
                    format!("Response fragment continues a missing array element at {path}")
                })?;
            let mut fragment = fragment.into_iter();
            if let Some(first) = fragment.next() {
                let continued = std::mem::take(&mut previous[at]);
                previous[at] = merge_part(Some(continued), first, merge, &format!("{path}/{at}"))?;
            }
            previous.extend(fragment);
        }
        _ => previous.extend(fragment),
    }
    Ok(Value::Array(previous))
}

fn merge_object(
    previous: Option<Value>,
    fragment: Map<String, Value>,
    merge: &Merge<'_>,
    path: &str,
) -> Result<Value, String> {
    let mut out = match previous {
        Some(Value::Object(fields)) => fields,
        _ => Map::new(),
    };
    for (key, value) in fragment {
        if key == "rowPart" && path.is_empty() {
            continue;
        }
        let child = format!("{path}/{}", escape(&key));
        let prior = out.get_mut(&key).map(std::mem::take);
        let merged = merge_part(prior, value, merge, &child)?;
        out.insert(key, merged);
    }
    Ok(Value::Object(out))
}

/// `own` with every field of `shared` it lacks, nested objects filled the
/// same way; a value `own` states is kept (the reader's `withShared`).
pub(super) fn with_shared(shared: &Value, own: Option<Value>) -> Value {
    match (shared, own) {
        (shared, None) => shared.clone(),
        (Value::Object(shared), Some(Value::Object(mut own))) => {
            for (key, value) in shared {
                let mine = own.get_mut(key).map(std::mem::take);
                own.insert(key.clone(), with_shared(value, mine));
            }
            Value::Object(own)
        }
        (_, Some(own)) => own,
    }
}
