use super::super::{bool_value, invalid, object, rust_enum_variant, rust_ident};
use super::{
    Field, default_literal, direct_children, enum_type_name, enum_values, field_type,
    rust_field_type, rust_type_name,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::Write as _;

/// One Rust enum per enum field. Its default is the contract default (the
/// first value); `as_str` is the wire value.
pub(super) fn render_enums(
    output: &mut String,
    all_fields: &[Field<'_>],
) -> Result<(), Box<dyn Error>> {
    let mut rendered = BTreeSet::new();
    for field in all_fields
        .iter()
        .filter(|field| field.resolved && field_type(field).ok() == Some("enum"))
    {
        let name = enum_type_name(field)?;
        if !rendered.insert(name.clone()) {
            return Err(invalid(format!("enum type {name} is declared twice")));
        }
        let values = enum_values(field)?;
        writeln!(
            output,
            "/// `{}` values.\n#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]\npub enum {name} {{",
            field.path
        )?;
        for (index, value) in values.iter().enumerate() {
            if index == 0 {
                writeln!(output, "    #[default]")?;
            }
            writeln!(
                output,
                "    #[serde(rename = {value:?})]\n    {},",
                rust_enum_variant(value)?
            )?;
        }
        writeln!(
            output,
            "}}\n\nimpl {name} {{\n    /// The contract spelling of this value.\n    #[must_use]\n    pub const fn as_str(self) -> &'static str {{\n        match self {{"
        )?;
        for value in &values {
            writeln!(
                output,
                "            Self::{} => {value:?},",
                rust_enum_variant(value)?
            )?;
        }
        writeln!(
            output,
            "        }}\n    }}\n}}\n\nimpl std::fmt::Display for {name} {{\n    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {{\n        f.write_str(self.as_str())\n    }}\n}}\n"
        )?;
    }
    Ok(())
}

/// One member of a generated struct: name, Rust type, default expression.
type Member = (String, String, String);

fn write_struct(output: &mut String, name: &str, members: &[Member]) -> std::fmt::Result {
    writeln!(
        output,
        "#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]\n#[serde(rename_all = \"camelCase\")]\npub struct {name} {{"
    )?;
    for (member, field_type, _) in members {
        writeln!(output, "    pub {member}: {field_type},")?;
    }
    writeln!(
        output,
        "}}\n\n/// The config contract's defaults.\nimpl Default for {name} {{\n    fn default() -> Self {{\n        Self {{"
    )?;
    for (member, _, default) in members {
        writeln!(output, "            {member}: {default},")?;
    }
    writeln!(output, "        }}\n    }}\n}}\n")
}

pub(super) fn render_structs(
    output: &mut String,
    contract: &Value,
    all_fields: &[Field<'_>],
) -> Result<(), Box<dyn Error>> {
    let sections = object(
        object(contract, "contract")?
            .get("sections")
            .ok_or_else(|| invalid("contract.sections is required"))?,
        "contract.sections",
    )?;
    if !sections.contains_key("") {
        return Err(invalid("root section is required"));
    }
    let mut rendered = BTreeMap::<String, Vec<Member>>::new();
    for (section_path, section_value) in sections {
        let section = object(section_value, &format!("sections.{section_path}"))?;
        if !bool_value(section.get("resolved"), false, "section.resolved")? {
            continue;
        }
        let type_name = if section_path.is_empty() {
            "ResolvedConfig".to_owned()
        } else {
            rust_type_name(section_path, section)?
        };
        let mut members = all_fields
            .iter()
            .filter(|field| field.section == section_path && field.resolved)
            .map(|field| {
                Ok((
                    rust_ident(field.key),
                    rust_field_type(field)?,
                    default_literal(field)?,
                ))
            })
            .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
        for child_path in direct_children(section_path, sections) {
            let child = object(
                sections
                    .get(&child_path)
                    .ok_or_else(|| invalid(format!("missing section {child_path}")))?,
                &format!("sections.{child_path}"),
            )?;
            if bool_value(child.get("resolved"), false, "section.resolved")? {
                let key = child_path
                    .rsplit('.')
                    .next()
                    .ok_or_else(|| invalid(format!("invalid child section {child_path}")))?;
                let child_type = rust_type_name(&child_path, child)?;
                members.push((
                    rust_ident(key),
                    child_type.clone(),
                    format!("{child_type}::default()"),
                ));
            }
        }
        if let Some(existing) = rendered.get(&type_name) {
            if existing != &members {
                return Err(invalid(format!(
                    "Rust type {type_name} is reused by incompatible sections"
                )));
            }
            continue;
        }
        write_struct(output, &type_name, &members)?;
        rendered.insert(type_name, members);
    }

    // One typed setter per resolved field: a value that does not fit its
    // field is an error for that field alone, never for the others.
    writeln!(
        output,
        "impl ResolvedConfig {{\n    /// Set the field at contract `path` from its JSON value; the field\n    /// keeps its value when `value` does not fit its type.\n    pub fn set_json(&mut self, path: &str, value: serde_json::Value) -> Result<(), String> {{\n        let error = |error: serde_json::Error| format!(\"{{path}}: {{error}}\");\n        match path {{"
    )?;
    for field in all_fields.iter().filter(|field| field.resolved) {
        let member = field
            .path
            .split('.')
            .map(rust_ident)
            .collect::<Vec<_>>()
            .join(".");
        writeln!(
            output,
            "            {:?} => self.{member} = serde_json::from_value(value).map_err(error)?,",
            field.path
        )?;
    }
    writeln!(
        output,
        "            _ => return Err(format!(\"{{path}} is not a resolved setting\")),\n        }}\n        Ok(())\n    }}\n}}\n"
    )?;
    Ok(())
}
