use super::{COPIED_FIELDS, LINE_NUMBER, field_id, selection_only, spec};
use clap::builder::PossibleValuesParser;
use clap::{Arg, ArgAction, Command};
use serde_json::Value;

/// Help heading for the fields MCP publishes.
pub(super) const QUERY_OPTIONS: &str = "Query options";
/// Help heading for secondary, paging and continuation fields.
pub(super) const MORE_OPTIONS: &str = "More query options (also in `schema <tool> --view full`)";

pub(super) fn preset_id(short: &str) -> String {
    format!("preset:{short}")
}

pub(super) fn split_id(short: &str) -> String {
    format!("split:{short}")
}

pub(super) fn first_char(text: &str) -> Option<char> {
    let mut chars = text.chars();
    let first = chars.next()?;
    chars.next().is_none().then_some(first)
}

pub(super) fn strings(value: &Value) -> Vec<String> {
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
pub(super) fn short_help(description: &str) -> String {
    description
        .split("\n")
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned()
}

/// Add the tool's flags and usage to its sub-command.
pub(in super::super) fn augment(mut command: Command, tool: &str) -> Command {
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
    let flag_form = format!("octocode {tool} [OPTIONS] {operation}{positionals}");
    command = command.override_usage(format!(
        "{}\n       octocode {tool} '<json>' | --input FILE|-",
        flag_form.trim_end()
    ));
    // A JSON-only tool has no positional fields to point at.
    if operation.is_empty() && positionals.is_empty() {
        command = command.mut_arg("args", |arg| {
            arg.help("A JSON query ('{…}' or '{\"queries\":[…]}')")
        });
    }
    // Published fields first, then their short letters, then the rest: clap
    // lists help headings in the order their first argument arrives.
    let flags = spec["flags"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let (primary_flags, more_flags): (Vec<&Value>, Vec<&Value>) =
        flags.iter().partition(|flag| flag["primary"] == true);
    let letters = preset_letters(spec);
    command = add_flags(command, &primary_flags, &letters);
    command = add_shorts(command, spec);
    command = add_flags(command, &more_flags, &letters);
    for field in selection_only(tool) {
        command = command.mut_arg(field_id(field), |arg| arg.hide(true));
    }
    // `rg -n` / `grep -n` habit: line numbers always print, so `-n` is
    // accepted and changes nothing.
    if tool == "localSearch" {
        command = command.arg(
            Arg::new(LINE_NUMBER)
                .short('n')
                .long("line-number")
                .action(ArgAction::SetTrue)
                .hide(true),
        );
    }
    command
}

/// Each field's preset letters as help text: `-F literal, -E rust`.
pub(super) fn preset_letters(spec: &Value) -> std::collections::HashMap<String, String> {
    let mut letters = std::collections::HashMap::<String, Vec<String>>::new();
    for preset in spec["presets"].as_array().into_iter().flatten() {
        if let (Some(short), Some(field)) = (preset["short"].as_str(), preset["field"].as_str()) {
            let value = preset["value"]
                .as_str()
                .map_or_else(|| preset["value"].to_string(), str::to_owned);
            letters
                .entry(field.to_owned())
                .or_default()
                .push(format!("-{short} {value}"));
        }
    }
    letters
        .into_iter()
        .map(|(field, list)| (field, list.join(", ")))
        .collect()
}

/// One long flag per field, typed by the field's kind; a field set by preset
/// letters names them (the letters' own rows are hidden).
pub(super) fn add_flags(
    mut command: Command,
    flags: &[&Value],
    letters: &std::collections::HashMap<String, String>,
) -> Command {
    for flag in flags {
        let Some(field) = flag["field"].as_str() else {
            continue;
        };
        let primary = flag["primary"] == true;
        let description = short_help(flag["description"].as_str().unwrap_or_default());
        let help = match letters.get(field) {
            Some(letters) => format!("{description} ({letters})"),
            None => description,
        };
        let copied = COPIED_FIELDS.contains(&field)
            || flag["description"]
                .as_str()
                .is_some_and(|text| text.starts_with("Copy "));
        let mut arg = Arg::new(field_id(field))
            .long(field.to_owned())
            .help(help)
            .hide(copied)
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
            // Names, kinds and ranges never hold a comma, so `a,b` is two
            // items (the spec marks them `commas`).
            "strings" if values.is_empty() && flag["commas"] == true => arg
                .value_name("TEXT")
                .action(ArgAction::Append)
                .value_delimiter(',')
                .allow_hyphen_values(true),
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
pub(super) fn add_shorts(mut command: Command, spec: &Value) -> Command {
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
                // Its field's help names it (`--regex … (-F literal, …)`).
                .hide(true)
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
