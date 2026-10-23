//! Tool flags, built from the embedded CLI presentation's `cli` spec
//! (`contracts::cli_tool_flags`, that part of it alone): every
//! query field is one long flag named as the field, a few short letters
//! follow rg, grep, fd, ast-grep and gh, and required fields read as
//! positional arguments. Flags only build the query object the JSON form
//! would; validation, defaults and errors stay with the runtime.
mod clap_args;
mod values;
use clap::ArgMatches;
pub(super) use clap_args::augment;
use clap_args::{preset_id, split_id, strings};
use serde_json::{Map, Value, json};
use values::{flag_value, line_suffix, positional_value, set_by_flag, split_two};

fn presentation_tool(tool: &str) -> Option<&'static Value> {
    octocode_native::contracts::cli_tool_flags()
        .ok()?
        .as_array()?
        .iter()
        .find(|candidate| candidate["name"] == tool)
}

/// A tool's `cli` spec from the embedded presentation: the selection form's
/// when it has one (a superset that adds `clasifyContext`), so every field
/// parses; the runtime still gates a field the configuration does not enable.
pub(super) fn spec(tool: &str) -> Option<&'static Value> {
    let entry = presentation_tool(tool)?;
    [&entry["selection"]["cli"], &entry["cli"]]
        .into_iter()
        .find(|cli| cli.is_object())
}

/// The flags only the selection form adds: help hides them unless a
/// classification key enables them, as MCP lists them only then.
pub(super) fn selection_only(tool: &str) -> Vec<&'static str> {
    let Some(entry) = presentation_tool(tool) else {
        return Vec::new();
    };
    let fields = |cli: &'static Value| -> Vec<&'static str> {
        cli["flags"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|flag| flag["field"].as_str())
            .collect()
    };
    let plain = fields(&entry["cli"]);
    fields(&entry["selection"]["cli"])
        .into_iter()
        .filter(|field| !plain.contains(field))
        .collect()
}

/// The arg id of a field's flag.
pub(super) fn field_id(field: &str) -> String {
    format!("field:{field}")
}

/// The accepted no-op `-n` (line numbers always print).
const LINE_NUMBER: &str = "habit:line-number";

/// Fields an agent never types: a follow-up call carries them. Help hides
/// them; the flags still parse.
const COPIED_FIELDS: [&str; 5] = ["page", "matchPage", "snapshot", "selection", "importerPage"];

/// A field only a follow-up call sets (`page`, `snapshot`, `selection`).
pub(super) fn is_copied(field: &str) -> bool {
    COPIED_FIELDS.contains(&field)
}

/// The query the flags and positionals build, or `None` when the call used
/// neither (the JSON form). `args` are the raw positionals.
pub(super) fn query(
    tool: &str,
    matches: &ArgMatches,
    args: &[String],
) -> Result<Option<Value>, String> {
    let Some(spec) = spec(tool) else {
        return Ok(None);
    };
    // A JSON query is the only argument: anything beside it is unexpected.
    if let [first, extra, ..] = args
        && first.trim_start().starts_with('{')
    {
        return Err(format!(
            "unexpected argument '{extra}' found; a JSON query is the only argument (a pattern that starts with '{{' goes in -e)"
        ));
    }
    // `--flag false` reads `false` as a positional; say how to write it.
    let positions: Vec<usize> = matches
        .indices_of("args")
        .map(Iterator::collect)
        .unwrap_or_default();
    for flag in spec["flags"].as_array().into_iter().flatten() {
        let Some(field) = flag["field"].as_str().filter(|_| flag["kind"] == "bool") else {
            continue;
        };
        if set_by_flag(matches, field)
            && let Some(at) = matches.index_of(&field_id(field))
            && let Some(slot) = positions.iter().position(|position| *position == at + 1)
            && matches!(args.get(slot).map(String::as_str), Some("true" | "false"))
        {
            return Err(format!(
                "write --{field}={} (with '='); a separate '{}' reads as an argument",
                args[slot], args[slot]
            ));
        }
    }
    let mut query = Map::new();
    let mut args: std::collections::VecDeque<&str> = args.iter().map(String::as_str).collect();
    let operations = strings(&spec["operation"]["values"]);
    if !set_by_flag(matches, "operation")
        && args
            .front()
            .is_some_and(|first| operations.iter().any(|op| op == first))
    {
        query.insert("operation".into(), json!(args.pop_front()));
    }
    // Short letters (the last one per field remains set, see `add_shorts`),
    // then explicit long flags win.
    for preset in spec["presets"].as_array().into_iter().flatten() {
        let Some(short) = preset["short"].as_str() else {
            continue;
        };
        if matches.get_flag(&preset_id(short))
            && let Some(field) = preset["field"].as_str()
        {
            query.insert(field.into(), preset["value"].clone());
        }
    }
    for split in spec["splits"].as_array().into_iter().flatten() {
        let Some(short) = split["short"].as_str() else {
            continue;
        };
        if let Some(text) = matches.get_one::<String>(&split_id(short)) {
            let separator = split["separator"].as_str().unwrap_or("/");
            let (first, second) = split_two(text, separator, &format!("-{short}"))?;
            let fields = strings(&split["fields"]);
            if let [a, b] = fields.as_slice() {
                query.insert(a.clone(), json!(first));
                query.insert(b.clone(), json!(second));
            }
        }
    }
    for flag in spec["flags"].as_array().into_iter().flatten() {
        let (Some(field), Some(kind)) = (flag["field"].as_str(), flag["kind"].as_str()) else {
            continue;
        };
        if let Some(value) = flag_value(matches, &field_id(field), kind)? {
            query.insert(field.into(), value);
        }
    }
    let used_flags = !query.is_empty();
    let used_args = !args.is_empty();
    let positionals = spec["positionals"]
        .as_array()
        .map_or(&[][..], Vec::as_slice);
    for (index, positional) in positionals.iter().enumerate() {
        // An operation that may omit this positional once a flag scopes it
        // (workspaceSymbol with --workspaceRoot) leaves a lone argument to
        // the positionals after it.
        let omit = &positional["omitWhen"];
        if query.get("operation").and_then(Value::as_str) == omit["operation"].as_str()
            && omit["field"]
                .as_str()
                .is_some_and(|field| query.contains_key(field))
            && args.len() < positionals.len() - index
        {
            continue;
        }
        let target = match &positional["field"] {
            Value::Object(by_operation) => {
                let operation = query
                    .get("operation")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                by_operation.get(operation).cloned().unwrap_or(Value::Null)
            }
            other => other.clone(),
        };
        let name = positional["name"].as_str().unwrap_or("ARG");
        match &target {
            Value::String(field) => {
                if query.contains_key(field) {
                    continue;
                }
                if positional["variadic"] == true {
                    if !args.is_empty() {
                        query.insert(field.clone(), json!(args.drain(..).collect::<Vec<_>>()));
                    }
                    continue;
                }
                let Some(text) = args.pop_front() else {
                    if let Some(default) = positional
                        .get("default")
                        .filter(|_| used_flags || used_args)
                    {
                        query.insert(field.clone(), default.clone());
                    }
                    continue;
                };
                let lines = &positional["lines"];
                match (
                    lines["field"].as_str(),
                    line_suffix(text, !tool.starts_with("gh")),
                ) {
                    (Some(lines_field), Some((path, spans))) => {
                        if query.contains_key(lines_field) {
                            return Err(format!(
                                "{text} names lines and --{lines_field} is set too; use one"
                            ));
                        }
                        let value = if lines["kind"] == "line" {
                            let [(start, None)] = spans.as_slice() else {
                                return Err(format!(
                                    "{name} takes one line (PATH:N), got '{text}'"
                                ));
                            };
                            json!(start)
                        } else {
                            json!(
                                spans
                                    .iter()
                                    .map(|(start, end)| format!(
                                        "{start}-{}",
                                        end.unwrap_or(*start)
                                    ))
                                    .collect::<Vec<_>>()
                            )
                        };
                        query.insert(field.clone(), json!(path));
                        query.insert(lines_field.into(), value);
                    }
                    _ => {
                        query.insert(field.clone(), positional_value(spec, field, text));
                    }
                }
            }
            Value::Array(fields) => {
                if fields
                    .iter()
                    .any(|field| field.as_str().is_some_and(|f| query.contains_key(f)))
                {
                    continue;
                }
                let Some(text) = args.pop_front() else {
                    continue;
                };
                let separator = positional["separator"].as_str().unwrap_or("/");
                let (first, second) = split_two(text, separator, name)?;
                if let [Value::String(a), Value::String(b)] = fields.as_slice() {
                    query.insert(a.clone(), positional_value(spec, a, first));
                    query.insert(b.clone(), positional_value(spec, b, second));
                }
            }
            _ => {
                if !args.is_empty() {
                    let names = operations.join(", ");
                    return Err(format!(
                        "{name} needs an operation first: octocode {tool} <{names}> {name}"
                    ));
                }
            }
        }
    }
    if let Some(extra) = args.front() {
        // `-l rust` from ast-grep habit: here `-l` lists files, as rg's does.
        let language = spec["flags"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|flag| flag["field"] == "language")
            .and_then(|flag| flag["short"].as_str());
        // `rg x a.rs b.rs` habit: one PATH per query.
        let second_path = std::path::Path::new(extra).exists();
        let tip = match language {
            Some(short)
                if matches.try_get_one::<bool>(&preset_id("l")).ok().flatten() == Some(&true) =>
            {
                format!(
                    "; -l lists matching files, as rg -l does; for a language use -{short} {extra}"
                )
            }
            _ if second_path => "; one PATH per call: search a directory that holds both (narrow with -g), or batch {\"queries\":[…]} with one row per path".to_owned(),
            _ => String::new(),
        };
        return Err(format!(
            "unexpected argument '{extra}' found{tip}; usage: octocode {tool} --help"
        ));
    }
    Ok((!query.is_empty()).then_some(Value::Object(query)))
}

#[cfg(test)]
mod tests;
