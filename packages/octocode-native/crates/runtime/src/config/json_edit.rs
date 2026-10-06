//! JSON with comments edits preserve every byte outside the changed members.
use serde_json::Value;
use std::collections::BTreeSet;
#[derive(Debug)]
struct Member {
    key: String,
    start: usize,
    end: usize,
    comma: Option<usize>,
    node: Node,
}
#[derive(Debug)]
struct Node {
    start: usize,
    end: usize,
    members: Option<Vec<Member>>,
}
struct Parser<'a> {
    text: &'a str,
    pos: usize,
}
const ERROR: &str = "Configuration must be valid JSON with comments and unique object keys.";
impl Parser<'_> {
    fn skip(&mut self) -> Result<(), String> {
        loop {
            while self
                .text
                .as_bytes()
                .get(self.pos)
                .is_some_and(u8::is_ascii_whitespace)
            {
                self.pos += 1;
            }
            if self.text[self.pos..].starts_with("//") {
                while self.pos < self.text.len() && self.text.as_bytes()[self.pos] != b'\n' {
                    self.pos += 1;
                }
            } else if self.text[self.pos..].starts_with("/*") {
                let end = self.text[self.pos + 2..].find("*/").ok_or(ERROR)?;
                self.pos += end + 4;
            } else {
                return Ok(());
            }
        }
    }
    fn string(&mut self) -> Result<String, String> {
        let start = self.pos;
        if self.text.as_bytes().get(self.pos) != Some(&b'"') {
            return Err(ERROR.into());
        }
        self.pos += 1;
        loop {
            match self.text.as_bytes().get(self.pos) {
                Some(b'\\') => self.pos += 2,
                Some(b'"') => {
                    self.pos += 1;
                    return serde_json::from_str(&self.text[start..self.pos])
                        .map_err(|_| ERROR.into());
                }
                Some(_) => self.pos += 1,
                None => return Err(ERROR.into()),
            }
        }
    }
    fn node(&mut self, depth: usize) -> Result<Node, String> {
        if depth > 128 {
            return Err(ERROR.into());
        }
        self.skip()?;
        let start = self.pos;
        let mut members = None;
        match self.text.as_bytes().get(self.pos) {
            Some(b'{') => {
                self.pos += 1;
                self.skip()?;
                let mut found = BTreeSet::new();
                let mut entries = vec![];
                while self.text.as_bytes().get(self.pos) != Some(&b'}') {
                    let member_start = self.pos;
                    let key = self.string()?;
                    if !found.insert(key.clone()) {
                        return Err(ERROR.into());
                    }
                    self.skip()?;
                    if self.text.as_bytes().get(self.pos) != Some(&b':') {
                        return Err(ERROR.into());
                    }
                    self.pos += 1;
                    let node = self.node(depth + 1)?;
                    let end = self.pos;
                    self.skip()?;
                    let comma = if self.text.as_bytes().get(self.pos) == Some(&b',') {
                        let p = self.pos;
                        self.pos += 1;
                        Some(p)
                    } else {
                        None
                    };
                    entries.push(Member {
                        key,
                        start: member_start,
                        end,
                        comma,
                        node,
                    });
                    self.skip()?;
                    if comma.is_none() && self.text.as_bytes().get(self.pos) != Some(&b'}') {
                        return Err(ERROR.into());
                    }
                }
                self.pos += 1;
                members = Some(entries);
            }
            Some(b'[') => {
                self.pos += 1;
                self.skip()?;
                while self.text.as_bytes().get(self.pos) != Some(&b']') {
                    self.node(depth + 1)?;
                    self.skip()?;
                    match self.text.as_bytes().get(self.pos) {
                        Some(b',') => {
                            self.pos += 1;
                            self.skip()?;
                        }
                        Some(b']') => (),
                        _ => return Err(ERROR.into()),
                    }
                }
                self.pos += 1;
            }
            Some(b'"') => {
                self.string()?;
            }
            Some(_) => {
                while self.text.as_bytes().get(self.pos).is_some_and(|b| {
                    !b.is_ascii_whitespace() && !matches!(b, b',' | b'}' | b']' | b'/')
                }) {
                    self.pos += 1;
                }
                if self.pos == start
                    || serde_json::from_str::<Value>(&self.text[start..self.pos]).is_err()
                {
                    return Err(ERROR.into());
                }
            }
            None => return Err(ERROR.into()),
        }
        Ok(Node {
            start,
            end: self.pos,
            members,
        })
    }
}
fn tree(text: &str) -> Result<Node, String> {
    let mut p = Parser { text, pos: 0 };
    let n = p.node(0)?;
    p.skip()?;
    if p.pos != text.len() {
        return Err(ERROR.into());
    }
    Ok(n)
}
fn value(text: &str, n: &Node) -> Result<Value, String> {
    // The existing loader owns JSONC syntax; the span parser adds duplicate checks.
    if n.members.is_some() {
        let loaded = super::load_config(&super::FileInput::Read {
            path: std::path::PathBuf::new(),
            text: text.into(),
        });
        loaded.config.ok_or_else(|| ERROR.into())
    } else {
        Err("Configuration must be an object.".into())
    }
}
pub fn parse_config_json(text: &str) -> Result<Value, String> {
    if text.trim().is_empty() {
        return Ok(serde_json::json!({}));
    }
    value(text, &tree(text)?)
}
fn apply(
    text: &str,
    node: &Node,
    path: &[&str],
    replacement: Option<&Value>,
) -> Result<String, String> {
    let members = node
        .members
        .as_ref()
        .ok_or("Setting parent must be an object.")?;
    let key = path.first().ok_or("Setting path cannot be empty.")?;
    if let Some((index, member)) = members.iter().enumerate().find(|(_, m)| m.key == *key) {
        if path.len() > 1 {
            return apply(text, &member.node, &path[1..], replacement);
        }
        let mut out = text.to_owned();
        if let Some(v) = replacement {
            out.replace_range(
                member.node.start..member.node.end,
                &serde_json::to_string(v).map_err(|_| ERROR)?,
            );
        } else {
            // Remove punctuation independently to retain comments between members.
            if let Some(comma) = member.comma {
                out.replace_range(comma..comma + 1, "");
            }
            out.replace_range(member.start..member.end, "");
            if member.comma.is_none()
                && index > 0
                && let Some(comma) = members[index - 1].comma
            {
                out.replace_range(comma..comma + 1, "");
            }
        }
        Ok(out)
    } else if let Some(v) = replacement {
        let mut nested = v.clone();
        for part in path[1..].iter().rev() {
            nested = serde_json::json!({*part:nested});
        }
        let property = format!(
            "\n  {}: {}\n",
            serde_json::to_string(key).map_err(|_| ERROR)?,
            serde_json::to_string(&nested).map_err(|_| ERROR)?
        );
        let mut out = text.to_owned();
        out.insert_str(node.end - 1, &property);
        if let Some(last) = members.last()
            && last.comma.is_none()
        {
            out.insert(last.end, ',');
        }
        Ok(out)
    } else {
        Ok(text.into())
    }
}
pub fn edit_config_json(
    text: &str,
    path: &[&str],
    replacement: Option<&Value>,
) -> Result<String, String> {
    let text = if text.trim().is_empty() { "{}" } else { text };
    let node = tree(text)?;
    value(text, &node)?;
    let out = apply(text, &node, path, replacement)?;
    parse_config_json(&out)?;
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn comments_and_siblings_survive() {
        let s =
            "{ // keep\n\"network\":{\"timeout\":5000,/*sibling*/\"maxRetries\":2,},\"other\":3}";
        let out =
            edit_config_json(s, &["network", "timeout"], Some(&serde_json::json!(6000))).unwrap();
        assert!(out.contains("/*sibling*/"));
        assert!(out.contains("// keep"));
        assert_eq!(parse_config_json(&out).unwrap()["network"]["timeout"], 6000);
        let out = edit_config_json(&out, &["network", "maxRetries"], None).unwrap();
        assert!(out.contains("/*sibling*/"));
        assert_eq!(parse_config_json(&out).unwrap()["other"], 3);
    }
    #[test]
    fn duplicate_keys_rejected() {
        assert!(parse_config_json("{\"a\":1,\"a\":2}").is_err());
        assert!(parse_config_json("{\"a\":[{\"b\":1,\"b\":2}]}").is_err());
    }
    #[test]
    fn new_nested_member() {
        let out = edit_config_json(
            "{/*keep*/}",
            &["output", "pagination", "defaultCharLength"],
            Some(&serde_json::json!(4000)),
        )
        .unwrap();
        assert!(out.contains("/*keep*/"));
        assert_eq!(
            parse_config_json(&out).unwrap()["output"]["pagination"]["defaultCharLength"],
            4000
        );
    }
}
