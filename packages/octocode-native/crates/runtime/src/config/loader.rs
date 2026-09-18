use super::types::{FileInput, LoadConfigResult};
use serde_json::Value;

fn strip_features(content: &str) -> String {
    let chars: Vec<char> = content.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let mut quote = None;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if quote.is_none() && (c == '"' || c == '\'') {
            quote = Some(c);
            out.push(c);
            i += 1;
            continue;
        }
        if let Some(q) = quote {
            out.push(c);
            if let ('\\', Some(escaped)) = (c, next) {
                out.push(escaped);
                i += 2;
                continue;
            }
            if c == q {
                quote = None
            }
            i += 1;
            continue;
        }
        if c == '/' && next == Some('/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1
            }
            continue;
        }
        if c == '/' && next == Some('*') {
            i += 2;
            while i + 1 < chars.len() {
                if chars[i] == '*' && chars[i + 1] == '/' {
                    i += 2;
                    break;
                }
                i += 1
            }
            continue;
        }
        out.push(c);
        i += 1;
    }
    let chars: Vec<char> = out.chars().collect();
    let mut clean = String::new();
    let mut i = 0;
    let mut quote = None;
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = quote {
            clean.push(c);
            if c == '\\' && i + 1 < chars.len() {
                clean.push(chars[i + 1]);
                i += 2;
                continue;
            }
            if c == q {
                quote = None
            }
            i += 1;
            continue;
        }
        if c == '"' || c == '\'' {
            quote = Some(c);
            clean.push(c);
            i += 1;
            continue;
        }
        if c == ',' {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1
            }
            if j < chars.len() && (chars[j] == '}' || chars[j] == ']') {
                i += 1;
                continue;
            }
        }
        clean.push(c);
        i += 1
    }
    clean
}
pub fn load_config(input: &FileInput) -> LoadConfigResult {
    let path = input.path().clone();
    match input {
        FileInput::Missing { .. } => LoadConfigResult {
            success: false,
            config: None,
            error: Some("Config file does not exist".into()),
            path,
        },
        FileInput::Unreadable { kind, .. } => LoadConfigResult {
            success: false,
            config: None,
            error: Some(format!("Failed to parse config file: {kind}")),
            path,
        },
        FileInput::Read { text, .. } if text.trim().is_empty() => LoadConfigResult {
            success: true,
            config: Some(Value::Object(Default::default())),
            error: None,
            path,
        },
        FileInput::Read { text, .. } => {
            match serde_json::from_str::<Value>(&strip_features(text)) {
                Ok(v) if v.is_object() => LoadConfigResult {
                    success: true,
                    config: Some(v),
                    error: None,
                    path,
                },
                Ok(_) => LoadConfigResult {
                    success: false,
                    config: None,
                    error: Some("Config file has invalid structure: must be a JSON object".into()),
                    path,
                },
                Err(e) => LoadConfigResult {
                    success: false,
                    config: None,
                    error: Some(format!("Failed to parse config file: {e}")),
                    path,
                },
            }
        }
    }
}
