use super::super::{bool_value, invalid, object, rust_enum_variant, string, to_pascal};
use serde_json::{Map, Value};
use std::error::Error;
use std::fmt::Write as _;

pub(super) struct Field<'a> {
    pub(super) path: String,
    pub(super) section: &'a str,
    pub(super) key: &'a str,
    pub(super) definition: &'a Map<String, Value>,
    pub(super) file: bool,
    pub(super) resolved: bool,
}

pub(super) fn fields<'a>(contract: &'a Value) -> Result<Vec<Field<'a>>, Box<dyn Error>> {
    let sections = object(
        object(contract, "contract")?
            .get("sections")
            .ok_or_else(|| invalid("contract.sections is required"))?,
        "contract.sections",
    )?;
    let mut output = Vec::new();
    for (section_path, section_value) in sections {
        let section = object(section_value, &format!("sections.{section_path}"))?;
        let file = bool_value(section.get("file"), false, "section.file")?;
        let section_resolved = bool_value(section.get("resolved"), false, "section.resolved")?;
        let definitions = object(
            section
                .get("fields")
                .ok_or_else(|| invalid(format!("sections.{section_path}.fields is required")))?,
            &format!("sections.{section_path}.fields"),
        )?;
        for (key, definition_value) in definitions {
            let definition = object(
                definition_value,
                &format!("sections.{section_path}.fields.{key}"),
            )?;
            let credential = bool_value(definition.get("credential"), false, "field.credential")?;
            output.push(Field {
                path: if section_path.is_empty() {
                    key.clone()
                } else {
                    format!("{section_path}.{key}")
                },
                section: section_path,
                key,
                definition,
                file,
                resolved: section_resolved && !credential,
            });
        }
    }
    Ok(output)
}

pub(super) fn field_type<'a>(field: &Field<'a>) -> Result<&'a str, Box<dyn Error>> {
    string(
        field
            .definition
            .get("type")
            .ok_or_else(|| invalid(format!("{}.type is required", field.path)))?,
        &format!("{}.type", field.path),
    )
}

/// A field whose contract default is `null`: unset is a distinct value.
fn nullable(field: &Field<'_>) -> bool {
    field.definition.get("default").is_some_and(Value::is_null)
}

pub(super) fn own_default(field: &Field<'_>) -> Result<Value, Box<dyn Error>> {
    match field_type(field)? {
        "enum" => field
            .definition
            .get("values")
            .and_then(Value::as_array)
            .and_then(|values| values.first())
            .cloned()
            .ok_or_else(|| invalid(format!("{}.values must not be empty", field.path))),
        _ => field
            .definition
            .get("default")
            .cloned()
            .ok_or_else(|| invalid(format!("{}.default is required", field.path))),
    }
}

/// The Rust enum an enum field resolves to: its contract `typeName`, else
/// its path in PascalCase (`lsp.prewarm` → `LspPrewarm`).
pub(super) fn enum_type_name(field: &Field<'_>) -> Result<String, Box<dyn Error>> {
    match field.definition.get("typeName") {
        Some(name) => Ok(string(name, &format!("{}.typeName", field.path))?.to_owned()),
        None => Ok(to_pascal(&field.path)),
    }
}

pub(super) fn enum_values<'a>(field: &Field<'a>) -> Result<Vec<&'a str>, Box<dyn Error>> {
    field
        .definition
        .get("values")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid(format!("{}.values must be an array", field.path)))?
        .iter()
        .map(|value| string(value, &format!("{}.values", field.path)))
        .collect()
}

/// The Rust expression of a field's contract default.
pub(super) fn default_literal(field: &Field<'_>) -> Result<String, Box<dyn Error>> {
    let default = own_default(field)?;
    if default.is_null() {
        return Ok("None".to_owned());
    }
    Ok(match field_type(field)? {
        "boolean" => default
            .as_bool()
            .ok_or_else(|| invalid(format!("{}.default must be a boolean", field.path)))?
            .to_string(),
        "enum" => format!(
            "{}::{}",
            enum_type_name(field)?,
            rust_enum_variant(
                default
                    .as_str()
                    .ok_or_else(|| invalid(format!("{} enum default", field.path)))?
            )?
        ),
        "stringArray" => {
            let items = default
                .as_array()
                .ok_or_else(|| invalid(format!("{}.default must be an array", field.path)))?
                .iter()
                .map(|item| {
                    string(item, &format!("{}.default", field.path))
                        .map(|item| format!("{item:?}.to_owned()"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            if items.is_empty() {
                "Vec::new()".to_owned()
            } else {
                format!("vec![{}]", items.join(", "))
            }
        }
        _ => format!(
            "{:?}.to_owned()",
            string(&default, &format!("{}.default", field.path))?
        ),
    })
}

pub(super) fn write_string_slice(
    output: &mut String,
    name: &str,
    values: &[String],
) -> Result<(), std::fmt::Error> {
    writeln!(output, "pub const {name}: &[&str] = &[")?;
    for value in values {
        writeln!(output, "    {value:?},")?;
    }
    writeln!(output, "];\n")
}

pub(super) fn direct_children(section_path: &str, sections: &Map<String, Value>) -> Vec<String> {
    let prefix = if section_path.is_empty() {
        String::new()
    } else {
        format!("{section_path}.")
    };
    sections
        .keys()
        .filter(|candidate| {
            !candidate.is_empty()
                && *candidate != section_path
                && candidate.starts_with(&prefix)
                && !candidate[prefix.len()..].contains('.')
        })
        .cloned()
        .collect()
}

pub(super) fn rust_type_name(
    section_path: &str,
    section: &Map<String, Value>,
) -> Result<String, Box<dyn Error>> {
    let base = if let Some(name) = section.get("rustTypeName") {
        string(name, &format!("sections.{section_path}.rustTypeName"))?.to_owned()
    } else if let Some(name) = section.get("typeName") {
        string(name, &format!("sections.{section_path}.typeName"))?.to_owned()
    } else {
        to_pascal(section_path)
    };
    Ok(format!("{base}Config"))
}

pub(super) fn rust_field_type(field: &Field<'_>) -> Result<String, Box<dyn Error>> {
    let nullable = nullable(field);
    let base = match field_type(field)? {
        "boolean" => "bool".to_owned(),
        "stringArray" => "Vec<String>".to_owned(),
        "string" | "url" | "path" => "String".to_owned(),
        "enum" => enum_type_name(field)?,
        other => {
            return Err(invalid(format!(
                "unsupported generated Rust field type {other}"
            )));
        }
    };
    Ok(if nullable {
        format!("Option<{base}>")
    } else {
        base
    })
}
