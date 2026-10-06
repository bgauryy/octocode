//! Build script: renders config and tool-identity Rust from the
//! `@octocodeai/config` contracts into `OUT_DIR` and embeds the tool contract.
//! [`config_gen`] renders `config_contract.rs`, [`tool_gen`] renders
//! `tool_ids.rs`, and [`embed`] embeds the contract after its fingerprint guards.

use serde_json::{Map, Value};
use std::error::Error;
use std::io::{Error as IoError, ErrorKind};
use std::path::{Path, PathBuf};

#[path = "build/config_gen.rs"]
mod config_gen;
#[path = "build/embed.rs"]
mod embed;
#[path = "build/tool_gen.rs"]
mod tool_gen;

fn invalid(message: impl Into<String>) -> Box<dyn Error> {
    IoError::new(ErrorKind::InvalidData, message.into()).into()
}

fn object<'a>(value: &'a Value, path: &str) -> Result<&'a Map<String, Value>, Box<dyn Error>> {
    value
        .as_object()
        .ok_or_else(|| invalid(format!("{path} must be an object")))
}

fn string<'a>(value: &'a Value, path: &str) -> Result<&'a str, Box<dyn Error>> {
    value
        .as_str()
        .ok_or_else(|| invalid(format!("{path} must be a string")))
}

fn bool_value(value: Option<&Value>, default: bool, path: &str) -> Result<bool, Box<dyn Error>> {
    value.map_or(Ok(default), |value| {
        value
            .as_bool()
            .ok_or_else(|| invalid(format!("{path} must be a boolean")))
    })
}

fn integer(value: &Value, path: &str) -> Result<i64, Box<dyn Error>> {
    value
        .as_i64()
        .ok_or_else(|| invalid(format!("{path} must be an integer")))
}

fn to_screaming_snake(value: &str) -> String {
    let characters = value.chars().collect::<Vec<_>>();
    let mut output = String::with_capacity(value.len());
    for (index, character) in characters.iter().copied().enumerate() {
        if character == '.' {
            output.push('_');
            continue;
        }
        let previous = index.checked_sub(1).and_then(|i| characters.get(i));
        let next = characters.get(index + 1);
        let starts_word = character.is_ascii_uppercase()
            && previous.is_some_and(|value| {
                value.is_ascii_lowercase()
                    || value.is_ascii_digit()
                    || (value.is_ascii_uppercase()
                        && next.is_some_and(|next| next.is_ascii_lowercase()))
            });
        if starts_word {
            output.push('_');
        }
        output.push(character.to_ascii_uppercase());
    }
    output
}

fn to_snake(value: &str) -> String {
    to_screaming_snake(value).to_ascii_lowercase()
}

/// Rust struct-field identifier for a config key, escaping reserved words as
/// raw identifiers so keys like `type` compile. `#[serde(rename_all = "camelCase")]`
/// still serializes the raw ident to its JSON key (serde strips the `r#`).
fn rust_ident(value: &str) -> String {
    let snake = to_snake(value);
    const RESERVED: &[&str] = &[
        "as", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "false",
        "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
        "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
        "unsafe", "use", "where", "while", "async", "await",
    ];
    if RESERVED.contains(&snake.as_str()) {
        format!("r#{snake}")
    } else {
        snake
    }
}

fn to_pascal(value: &str) -> String {
    value
        .split('.')
        .flat_map(|part| {
            let mut words = Vec::new();
            let mut start = 0;
            let characters = part.char_indices().collect::<Vec<_>>();
            for (index, (offset, character)) in characters.iter().copied().enumerate() {
                if index > 0 && character.is_ascii_uppercase() {
                    words.push(&part[start..offset]);
                    start = offset;
                }
            }
            words.push(&part[start..]);
            words
        })
        .map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .map(|first| format!("{}{}", first.to_ascii_uppercase(), chars.as_str()))
                .unwrap_or_default()
        })
        .collect()
}

fn rust_enum_variant(value: &str) -> Result<String, Box<dyn Error>> {
    if value.is_empty()
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        return Err(invalid(format!("invalid runtime surface {value:?}")));
    }
    Ok(to_pascal(value))
}

fn set_path(root: &mut Value, path: &str, value: Value) -> Result<(), Box<dyn Error>> {
    let mut current = root;
    let parts = path.split('.').collect::<Vec<_>>();
    for part in &parts[..parts.len().saturating_sub(1)] {
        let map = current
            .as_object_mut()
            .ok_or_else(|| invalid(format!("default parent for {path} must be an object")))?;
        current = map
            .entry((*part).to_owned())
            .or_insert_with(|| Value::Object(Map::new()));
    }
    let key = parts
        .last()
        .ok_or_else(|| invalid("field path must not be empty"))?;
    current
        .as_object_mut()
        .ok_or_else(|| invalid(format!("default parent for {path} must be an object")))?
        .insert((*key).to_owned(), value);
    Ok(())
}

fn get_path<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = root;
    for part in path.split('.') {
        current = current.as_object()?.get(part)?;
    }
    Some(current)
}

fn config_path(manifest_dir: &Path, name: &str) -> PathBuf {
    manifest_dir.join(format!("../../../../packages/octocode-config/{name}"))
}

fn main() -> Result<(), Box<dyn Error>> {
    config_gen::generate_config_contract()?;
    embed::embed_tool_contract()?;

    Ok(())
}
