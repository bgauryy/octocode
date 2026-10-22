//! Tool flags, built from the embedded CLI presentation's `cli` spec: every
//! query field is one long flag named as the field, a few short letters
//! follow rg, grep, fd, ast-grep and gh, and required fields read as
//! positional arguments. Flags only build the query object the JSON form
//! would; validation, defaults and errors stay with the runtime.
use clap::builder::PossibleValuesParser;
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde_json::{Map, Value, json};

/// Help heading for the fields MCP publishes.
const QUERY_OPTIONS: &str = "Query options";
/// Help heading for secondary, paging and continuation fields.
const MORE_OPTIONS: &str = "More query options (also in `schema <tool> --view full`)";

fn presentation_tool(tool: &str) -> Option<&'static Value> {
    let presentation = octocode_native::contracts::cli_presentation().ok()?;
    presentation["tools"]
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

fn preset_id(short: &str) -> String {
    format!("preset:{short}")
}

fn split_id(short: &str) -> String {
    format!("split:{short}")
}

fn first_char(text: &str) -> Option<char> {
    let mut chars = text.chars();
    let first = chars.next()?;
    chars.next().is_none().then_some(first)
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// One sentence of a schema description, so a flag's help stays one line.
fn short_help(description: &str) -> String {
    description
        .split("\n")
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned()
}

/// Add the tool's flags and usage to its sub-command.
pub(super) fn augment(mut command: Command, tool: &str) -> Command {
    let Some(spec) = spec(tool) else {
        return command;
    };
    let positionals = spec["positionals"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    let name = item["name"].as_str().unwrap_or("ARG");
                    let name = if item["variadic"] == true {
                        format!("{name}...")
                    } else {
                        name.to_owned()
                    };
                    if item.get("default").is_some() {
                        format!("[{name}]")
                    } else {
                        name
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    let operation = if spec.get("operation").is_some() {
        "[OPERATION] "
    } else {
        ""
    };
    command = command.override_usage(format!(
        "octocode {tool} [OPTIONS] {operation}{positionals}\n       octocode {tool} '<json>' | --input FILE|-"
    ));
    // Published fields first, then their short letters, then the rest: clap
    // lists help headings in the order their first argument arrives.
    let flags = spec["flags"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let (primary_flags, more_flags): (Vec<&Value>, Vec<&Value>) =
        flags.iter().partition(|flag| flag["primary"] == true);
    command = add_flags(command, &primary_flags);
    command = add_shorts(command, spec);
    command = add_flags(command, &more_flags);
    for field in selection_only(tool) {
        command = command.mut_arg(field_id(field), |arg| arg.hide(true));
    }
    command
}

/// One long flag per field, typed by the field's kind.
fn add_flags(mut command: Command, flags: &[&Value]) -> Command {
    for flag in flags {
        let Some(field) = flag["field"].as_str() else {
            continue;
        };
        let primary = flag["primary"] == true;
        let mut arg = Arg::new(field_id(field))
            .long(field.to_owned())
            .help(short_help(flag["description"].as_str().unwrap_or_default()))
            .help_heading(if primary { QUERY_OPTIONS } else { MORE_OPTIONS });
        if let Some(short) = flag["short"].as_str().and_then(first_char) {
            arg = arg.short(short);
        }
        let values = strings(&flag["values"]);
        arg = match flag["kind"].as_str().unwrap_or("json") {
            // `--flag` is true; `--flag=false` says false.
            "bool" => arg
                .num_args(0..=1)
                .require_equals(true)
                .default_missing_value("true")
                .value_parser(["true", "false"])
                .value_name("BOOL")
                .hide_possible_values(true),
            "enum" => arg
                .value_name("VALUE")
                .value_parser(PossibleValuesParser::new(values)),
            // A pattern may start with `-`, as `rg -e -x` allows.
            "strings" if values.is_empty() => arg
                .value_name("TEXT")
                .action(ArgAction::Append)
                .allow_hyphen_values(true),
            // An enum list takes `a,b` as gh does; free text keeps commas
            // (`*.{ts,tsx}`).
            "strings" => arg
                .value_name("VALUE")
                .action(ArgAction::Append)
                .value_delimiter(',')
                .value_parser(PossibleValuesParser::new(values)),
            "integer" => arg.value_name("N").value_parser(clap::value_parser!(i64)),
            "integers" => arg
                .value_name("N")
                .action(ArgAction::Append)
                .value_parser(clap::value_parser!(i64)),
            "number" => arg.value_name("N").value_parser(clap::value_parser!(f64)),
            "string" => arg.value_name("TEXT").allow_hyphen_values(true),
            _ => arg.value_name("JSON"),
        };
        command = command.arg(arg);
    }
    command
}

/// The short letters that set a value (`-i`) or split one (`-R OWNER/REPO`).
fn add_shorts(mut command: Command, spec: &Value) -> Command {
    let presets: Vec<(&str, &str)> = spec["presets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|preset| Some((preset["short"].as_str()?, preset["field"].as_str()?)))
        .collect();
    for preset in spec["presets"].as_array().into_iter().flatten() {
        let (Some(short), Some(field)) = (preset["short"].as_str(), preset["field"].as_str())
        else {
            continue;
        };
        let Some(letter) = first_char(short) else {
            continue;
        };
        // The last letter for a field wins, as `rg -i -s` does: each one
        // overrides the field's others and itself.
        let rivals: Vec<String> = presets
            .iter()
            .filter(|(_, other)| *other == field)
            .map(|(short, _)| preset_id(short))
            .collect();
        command = command.arg(
            Arg::new(preset_id(short))
                .short(letter)
                .action(ArgAction::SetTrue)
                .overrides_with_all(rivals)
                .help(format!("{field}: {}", preset["value"]))
                .help_heading(QUERY_OPTIONS),
        );
    }
    for split in spec["splits"].as_array().into_iter().flatten() {
        let Some(short) = split["short"].as_str() else {
            continue;
        };
        let Some(letter) = first_char(short) else {
            continue;
        };
        command = command.arg(
            Arg::new(split_id(short))
                .short(letter)
                .value_name(split["valueName"].as_str().unwrap_or("A/B").to_owned())
                .help(split["description"].as_str().unwrap_or_default().to_owned())
                .help_heading(QUERY_OPTIONS),
        );
    }
    command
}

/// The value one flag holds in `matches`, as the field's JSON value.
fn flag_value(matches: &ArgMatches, id: &str, kind: &str) -> Result<Option<Value>, String> {
    if !matches.contains_id(id)
        || matches.value_source(id) != Some(clap::parser::ValueSource::CommandLine)
    {
        return Ok(None);
    }
    Ok(Some(match kind {
        "bool" => json!(matches.get_one::<String>(id).map(String::as_str) != Some("false")),
        "integer" => json!(matches.get_one::<i64>(id).copied()),
        "integers" => json!(
            matches
                .get_many::<i64>(id)
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
        ),
        "number" => json!(matches.get_one::<f64>(id).copied()),
        "strings" => json!(
            matches
                .get_many::<String>(id)
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
        ),
        "json" => {
            let text = matches.get_one::<String>(id).cloned().unwrap_or_default();
            serde_json::from_str(&text).unwrap_or(Value::String(text))
        }
        _ => json!(matches.get_one::<String>(id)),
    }))
}

/// Split `text` at `separator` into exactly two non-empty parts.
fn split_two<'a>(text: &'a str, separator: &str, name: &str) -> Result<(&'a str, &'a str), String> {
    match text.split_once(separator) {
        Some((first, second)) if !first.is_empty() && !second.is_empty() => Ok((first, second)),
        _ => Err(format!("{name} must look like A{separator}B, got '{text}'")),
    }
}

/// A run of ASCII digits as a number.
fn digits(text: &str) -> Option<u64> {
    (!text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
}

/// `PATH:N` or `PATH:N-M`. A local path that exists under the whole text
/// keeps it; a remote (GitHub) path is never checked on this disk.
fn line_suffix(text: &str, local: bool) -> Option<(&str, u64, Option<u64>)> {
    if local && std::path::Path::new(text).exists() {
        return None;
    }
    let (path, lines) = text.rsplit_once(':')?;
    let (start, end) = match lines.split_once('-') {
        Some((start, end)) => (start, Some(digits(end)?)),
        None => (lines, None),
    };
    let start = digits(start)?;
    (!path.is_empty()).then_some((path, start, end))
}

/// Whether the caller set `field` by flag (or short letter).
fn set_by_flag(matches: &ArgMatches, field: &str) -> bool {
    let id = field_id(field);
    matches.try_contains_id(&id).unwrap_or(false)
        && matches.value_source(&id) == Some(clap::parser::ValueSource::CommandLine)
}

/// A positional's text as the value its field takes.
fn positional_value(spec: &Value, field: &str, text: &str) -> Value {
    let kind = spec["flags"]
        .as_array()
        .and_then(|flags| flags.iter().find(|flag| flag["field"] == field))
        .and_then(|flag| flag["kind"].as_str())
        .unwrap_or("string");
    match kind {
        "integer" => text
            .parse::<i64>()
            .map_or_else(|_| json!(text), |n| json!(n)),
        "strings" => json!([text]),
        _ => json!(text),
    }
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
    for positional in spec["positionals"].as_array().into_iter().flatten() {
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
                    (Some(lines_field), Some((path, start, end))) => {
                        if query.contains_key(lines_field) {
                            return Err(format!(
                                "{text} names lines and --{lines_field} is set too; use one"
                            ));
                        }
                        let value = if lines["kind"] == "line" {
                            if end.is_some() {
                                return Err(format!(
                                    "{name} takes one line (PATH:N), got '{text}'"
                                ));
                            }
                            json!(start)
                        } else {
                            json!([format!("{start}-{}", end.unwrap_or(start))])
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
        let tip = match language {
            Some(short) if matches.try_get_one::<bool>(&preset_id("l")).ok().flatten() == Some(&true) => {
                format!("; -l lists matching files, as rg -l does; for a language use -{short} {extra}")
            }
            _ => String::new(),
        };
        return Err(format!(
            "unexpected argument '{extra}' found{tip}; usage: octocode {tool} --help"
        ));
    }
    Ok((!query.is_empty()).then_some(Value::Object(query)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(tool: &str, argv: &[&str]) -> Result<Option<Value>, String> {
        let command = augment(
            clap::Command::new(tool.to_owned())
                .arg(Arg::new("args").num_args(0..).action(ArgAction::Append)),
            tool,
        );
        let matches = command
            .try_get_matches_from(std::iter::once(tool).chain(argv.iter().copied()))
            .map_err(|error| error.to_string())?;
        let args: Vec<String> = matches
            .get_many::<String>("args")
            .into_iter()
            .flatten()
            .cloned()
            .collect();
        query(tool, &matches, &args)
    }

    #[test]
    fn rg_style_local_search_builds_the_json_query() {
        assert_eq!(
            parse(
                "localSearch",
                &[
                    "SearcherBuilder",
                    "crates",
                    "-w",
                    "-i",
                    "-g",
                    "*.rs",
                    "-C",
                    "1"
                ]
            )
            .unwrap(),
            Some(json!({
                "matchString": ["SearcherBuilder"], "path": "crates", "wholeWord": true,
                "caseMode": "insensitive", "include": ["*.rs"], "contextLines": 1
            }))
        );
        // `-e` sets the pattern, so the first positional is the path; the path defaults to `.`.
        assert_eq!(
            parse("localSearch", &["-e", "a", "-e", "b", "src"]).unwrap(),
            Some(json!({"matchString": ["a", "b"], "path": "src"}))
        );
        assert_eq!(
            parse("localSearch", &["TODO", "-l"]).unwrap(),
            Some(json!({"matchString": ["TODO"], "path": ".", "resultView": "files"}))
        );
        // A long flag beats a preset; `--flag=false` is false.
        assert_eq!(
            parse(
                "localSearch",
                &[
                    "x",
                    "-i",
                    "--caseMode",
                    "sensitive",
                    "--defaultExcludes=false"
                ]
            )
            .unwrap(),
            Some(
                json!({"matchString": ["x"], "path": ".", "caseMode": "sensitive", "defaultExcludes": false})
            )
        );
    }

    #[test]
    fn operations_and_split_values_follow_gh() {
        assert_eq!(
            parse(
                "ghSearchHistory",
                &[
                    "pullRequest",
                    "fix",
                    "bug",
                    "-R",
                    "o/r",
                    "-s",
                    "merged",
                    "-L",
                    "3"
                ]
            )
            .unwrap(),
            Some(json!({
                "operation": "pullRequest", "keywords": ["fix", "bug"], "owner": "o",
                "repo": "r", "state": "merged", "pageSize": 3
            }))
        );
        assert_eq!(
            parse("ghGetHistoryItem", &["pullRequest", "12", "-R", "o/r"]).unwrap(),
            Some(json!({"operation": "pullRequest", "number": 12, "owner": "o", "repo": "r"}))
        );
        assert_eq!(
            parse("ghGetHistoryItem", &["compare", "v1...v2", "-R", "o/r"]).unwrap(),
            Some(
                json!({"operation": "compare", "base": "v1", "head": "v2", "owner": "o", "repo": "r"})
            )
        );
        assert_eq!(
            parse("ghCloneRepo", &["vercel/next.js"]).unwrap(),
            Some(json!({"owner": "vercel", "repo": "next.js"}))
        );
        assert!(parse("ghSearchCode", &["x", "-R", "noslash"]).is_err());
    }

    #[test]
    fn line_suffixes_read_into_ranges_or_line() {
        assert_eq!(
            parse("localFetch", &["no/such/file.rs:10-40"]).unwrap(),
            Some(json!({"path": "no/such/file.rs", "ranges": ["10-40"]}))
        );
        assert_eq!(
            parse("lspSearch", &["references", "no/such/a.ts:10", "run"]).unwrap(),
            Some(
                json!({"operation": "references", "path": "no/such/a.ts", "line": 10, "symbolName": "run"})
            )
        );
    }

    #[test]
    fn review_edge_cases() {
        // `--bool false` with a space is refused with the `=` spelling.
        assert!(
            parse("localSearch", &["foo", "--wholeWord", "false"])
                .is_err_and(|e| e.contains("--wholeWord=false"))
        );
        // `-e` takes a pattern that starts with `-`.
        assert_eq!(
            parse("localSearch", &["-e", "-flag", "src"]).unwrap(),
            Some(json!({"matchString": ["-flag"], "path": "src"}))
        );
        // Repeated short letters: the last one wins.
        assert_eq!(
            parse("localSearch", &["x", "-i", "-s", "-i"]).unwrap(),
            Some(json!({"matchString": ["x"], "path": ".", "caseMode": "insensitive"}))
        );
        // An explicit --operation keeps a positional named like an operation.
        assert_eq!(
            parse("structureSearch", &["--operation", "tree", "files"]).unwrap(),
            Some(json!({"operation": "tree", "path": "files"}))
        );
        // PATH:N beside --ranges, and a range for a single line, are errors.
        assert!(parse("localFetch", &["no/such/a.txt:1", "--ranges", "4-5"]).is_err());
        assert!(parse("lspSearch", &["references", "no/such/a.ts:10-20", "run"]).is_err());
        // `+10` is not a line number; flags win over OWNER/REPO.
        assert_eq!(
            parse("localFetch", &["no/such/a.txt:+10"]).unwrap(),
            Some(json!({"path": "no/such/a.txt:+10"}))
        );
        assert_eq!(
            parse("ghCloneRepo", &["--owner", "o", "--repo", "r"]).unwrap(),
            Some(json!({"owner": "o", "repo": "r"}))
        );
        // A GitHub path with a line suffix is never checked on this disk.
        assert_eq!(
            parse("ghGetFileContent", &["src/a.rs:1-5", "-R", "o/r"]).unwrap(),
            Some(json!({"owner": "o", "repo": "r", "path": "src/a.rs", "ranges": ["1-5"]}))
        );
    }

    #[test]
    fn no_flags_and_no_arguments_is_the_json_form() {
        assert_eq!(parse("localSearch", &[]).unwrap(), None);
        assert!(
            parse("localSearch", &["a", "b", "c"])
                .is_err_and(|e| e.contains("unexpected argument 'c' found"))
        );
    }
}
