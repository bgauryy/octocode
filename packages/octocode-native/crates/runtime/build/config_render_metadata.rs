use super::super::{bool_value, integer, invalid, object, string, to_screaming_snake};
use super::{EnvironmentArrays, Field, own_default, write_string_slice};
use serde_json::Value;
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::Write as _;

pub(super) fn render_field_metadata(
    output: &mut String,
    all_fields: &[Field<'_>],
) -> Result<(), Box<dyn Error>> {
    for (index, field) in all_fields.iter().enumerate() {
        let binding = field
            .definition
            .get("env")
            .map(|value| object(value, &format!("{}.env", field.path)))
            .transpose()?;
        let rendered = match binding {
            None => "None".to_owned(),
            Some(binding) => {
                let name = string(
                    binding
                        .get("name")
                        .ok_or_else(|| invalid(format!("{}.env.name is required", field.path)))?,
                    &format!("{}.env.name", field.path),
                )?;
                let normalize = match binding.get("normalize").and_then(Value::as_str) {
                    Some("trim") => "Some(ConfigNormalize::Trim)",
                    Some("lower") => "Some(ConfigNormalize::Lower)",
                    Some(other) => {
                        return Err(invalid(format!("unsupported normalize mode {other}")));
                    }
                    None => "None",
                };
                format!("Some(ConfigEnvBinding {{ name: {name:?}, normalize: {normalize} }})")
            }
        };
        writeln!(
            output,
            "const FIELD_{index}_ENV: Option<ConfigEnvBinding> = {rendered};\n"
        )?;

        let values = field
            .definition
            .get("values")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .map(|value| {
                        string(value, &format!("{}.values", field.path)).map(ToOwned::to_owned)
                    })
                    .collect::<Result<Vec<_>, Box<dyn Error>>>()
            })
            .transpose()?
            .unwrap_or_default();
        write_string_slice(output, &format!("FIELD_{index}_VALUES"), &values)?;
    }

    writeln!(output, "pub const CONFIG_FIELDS: &[ConfigFieldSpec] = &[")?;
    for (index, field) in all_fields.iter().enumerate() {
        let kind_name = match string(
            field
                .definition
                .get("type")
                .ok_or_else(|| invalid(format!("{}.type is required", field.path)))?,
            &format!("{}.type", field.path),
        )? {
            "boolean" => "Boolean",
            "stringArray" => "StringArray",
            "enum" => "Enum",
            "url" => "Url",
            "path" => "Path",
            "string" => "String",
            other => return Err(invalid(format!("unsupported field type {other}"))),
        };
        let default_json = serde_json::to_string(&own_default(field)?)?;
        let enum_style = match field.definition.get("enumStyle").and_then(Value::as_str) {
            Some("quotedOr") => "ConfigEnumStyle::QuotedOr",
            Some("list") | None => "ConfigEnumStyle::List",
            Some(other) => return Err(invalid(format!("unsupported enum style {other}"))),
        };
        let credential = bool_value(
            field.definition.get("credential"),
            false,
            "field.credential",
        )?;
        writeln!(
            output,
            "    ConfigFieldSpec {{ path: {:?}, section: {:?}, key: {:?}, kind: ConfigFieldKind::{kind_name}, file: {}, resolved: {}, credential: {credential}, env: FIELD_{index}_ENV, default_json: {:?}, values: FIELD_{index}_VALUES, enum_style: {enum_style}, item_path: {} }},",
            field.path,
            field.section,
            field.key,
            field.file,
            field.resolved,
            default_json,
            field.definition.get("itemFormat").and_then(Value::as_str) == Some("path")
        )?;
    }
    writeln!(output, "];\n")?;
    // One constant per field, named after its path: code names the setting
    // it reads instead of searching CONFIG_FIELDS by a path string.
    writeln!(
        output,
        "/// Every config field by its path (`classification.api` → `CLASSIFICATION_API`).\npub mod field {{\n    use super::{{CONFIG_FIELDS, ConfigFieldSpec}};"
    )?;
    for (index, field) in all_fields.iter().enumerate() {
        writeln!(
            output,
            "    /// `{}`\n    pub const {}: &ConfigFieldSpec = &CONFIG_FIELDS[{index}];",
            field.path,
            to_screaming_snake(&field.path)
        )?;
    }
    writeln!(output, "}}\n")?;
    Ok(())
}

pub(super) fn environment_arrays(
    contract: &Value,
    all_fields: &[Field<'_>],
) -> Result<EnvironmentArrays, Box<dyn Error>> {
    let root = object(contract, "contract")?;
    let environment = object(
        root.get("environment")
            .ok_or_else(|| invalid("contract.environment is required"))?,
        "contract.environment",
    )?;
    let mut protected = Vec::new();
    let mut home = Vec::new();
    let mut sources = Vec::new();
    let mut tokens = Vec::new();
    let mut narrow = Vec::new();
    for (name, definition_value) in environment {
        let definition = object(definition_value, &format!("environment.{name}"))?;
        match definition
            .get("dotenv")
            .and_then(Value::as_str)
            .unwrap_or("all")
        {
            "never" => protected.push(name.clone()),
            "home" => {
                protected.push(name.clone());
                home.push(name.clone());
            }
            "all" => {}
            other => return Err(invalid(format!("unsupported dotenv policy {other}"))),
        }
        if definition.get("configSource").and_then(Value::as_bool) == Some(true) {
            sources.push(name.clone());
        }
        if let Some(priority) = definition.get("tokenPriority") {
            tokens.push((
                integer(priority, &format!("environment.{name}.tokenPriority"))?,
                name.clone(),
            ));
        }
    }
    for field in all_fields {
        let Some(bindings) = field.definition.get("env") else {
            continue;
        };
        let binding = object(bindings, &format!("{}.env", field.path))?;
        let name = string(
            binding
                .get("name")
                .ok_or_else(|| invalid(format!("{}.env.name is required", field.path)))?,
            &format!("{}.env.name", field.path),
        )?
        .to_owned();
        match binding
            .get("dotenv")
            .and_then(Value::as_str)
            .unwrap_or("all")
        {
            "never" => protected.push(name.clone()),
            "home" => {
                protected.push(name.clone());
                home.push(name.clone());
            }
            "all" => {}
            other => return Err(invalid(format!("unsupported dotenv policy {other}"))),
        }
        if let Some(value) = binding.get("workspaceNarrowsTo") {
            narrow.push((
                name.clone(),
                string(
                    value,
                    &format!("{}.env.{name}.workspaceNarrowsTo", field.path),
                )?
                .to_owned(),
            ));
        }
        sources.push(name);
    }
    tokens.sort_by_key(|(priority, _)| *priority);
    let unique = |values: Vec<String>| {
        values
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    };
    Ok((
        tokens.into_iter().map(|(_, name)| name).collect(),
        unique(protected),
        unique(home),
        unique(sources),
        narrow,
    ))
}
