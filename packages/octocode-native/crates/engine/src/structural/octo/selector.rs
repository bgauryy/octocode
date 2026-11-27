use super::error::CompileError;
use serde_json::{Value, json};

pub(super) fn parse(source: &str) -> Result<Value, CompileError> {
    parse_at(source, 0)
}

fn parse_at(source: &str, depth: usize) -> Result<Value, CompileError> {
    if source.len() > 64_000 || depth >= 32 {
        return Err(CompileError::rule(
            "kind selector exceeds its size or nesting limit",
        ));
    }
    let mut parser = Parser {
        source: source.as_bytes(),
        at: 0,
        depth,
    };
    let value = parser.list()?;
    parser.space();
    if parser.at != parser.source.len() {
        return Err(parser.invalid());
    }
    Ok(value)
}

struct Parser<'a> {
    source: &'a [u8],
    at: usize,
    depth: usize,
}

fn all(mut values: Vec<Value>) -> Value {
    if values.len() == 1 {
        values.remove(0)
    } else {
        json!({"all":values})
    }
}

impl Parser<'_> {
    fn invalid(&self) -> CompileError {
        CompileError::rule(format!("invalid kind selector near byte {}", self.at))
    }
    fn peek(&self) -> Option<u8> {
        self.source.get(self.at).copied()
    }
    fn space(&mut self) -> bool {
        let start = self.at;
        while self.peek().is_some_and(|c| c.is_ascii_whitespace()) {
            self.at += 1;
        }
        self.at != start
    }
    fn list(&mut self) -> Result<Value, CompileError> {
        let mut items = vec![self.complex()?];
        loop {
            self.space();
            if self.peek() != Some(b',') {
                break;
            }
            self.at += 1;
            items.push(self.complex()?);
            if items.len() > 50 {
                return Err(self.invalid());
            }
        }
        Ok(if items.len() == 1 {
            items.remove(0)
        } else {
            json!({"any":items})
        })
    }
    fn complex(&mut self) -> Result<Value, CompileError> {
        self.space();
        let mut left = self.compound()?;
        let mut steps = 0;
        loop {
            let whitespace = self.space();
            let op = match self.peek() {
                None | Some(b')' | b',') => break,
                Some(c @ (b'>' | b'+' | b'~')) => {
                    self.at += 1;
                    c
                }
                Some(_) if whitespace => b' ',
                _ => return Err(self.invalid()),
            };
            steps += 1;
            if steps >= 64 {
                return Err(self.invalid());
            }
            self.space();
            let right = self.compound()?;
            let direction = if matches!(op, b'+' | b'~') {
                "follows"
            } else {
                "inside"
            };
            if matches!(op, b' ' | b'~') {
                left["stopBy"] = json!("end");
            }
            left = all(vec![right, json!({direction:left})]);
        }
        Ok(left)
    }
    fn name(&mut self) -> Result<String, CompileError> {
        let start = self.at;
        if !self
            .peek()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == b'_')
        {
            return Err(self.invalid());
        }
        self.at += 1;
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        {
            self.at += 1;
        }
        String::from_utf8(self.source[start..self.at].to_vec()).map_err(|_| self.invalid())
    }
    fn compound(&mut self) -> Result<Value, CompileError> {
        let mut items = Vec::new();
        if self.peek() != Some(b':') {
            items.push(json!({"kind":self.name()?}));
        }
        while self.peek() == Some(b':') {
            self.at += 1;
            let name = self.name()?;
            if self.peek() != Some(b'(') || self.depth >= 32 {
                return Err(self.invalid());
            }
            self.at += 1;
            self.depth += 1;
            self.space();
            let value = match name.as_str() {
                "is" => self.list()?,
                "not" => json!({"not":self.complex()?}),
                "has" => {
                    let direct = self.peek() == Some(b'>');
                    if direct {
                        self.at += 1;
                    }
                    let mut child = self.complex()?;
                    if !direct {
                        child["stopBy"] = json!("end");
                    }
                    json!({"has":child})
                }
                "nth-child" | "nth-last-child" => {
                    let start = self.at;
                    let mut nesting = 0usize;
                    while let Some(c) = self.peek() {
                        if c == b')' && nesting == 0 {
                            break;
                        }
                        if c == b'(' {
                            nesting += 1;
                        }
                        if c == b')' {
                            nesting = nesting.saturating_sub(1);
                        }
                        self.at += 1;
                    }
                    let body = std::str::from_utf8(&self.source[start..self.at])
                        .map_err(|_| self.invalid())?;
                    let mut split = None;
                    let bytes = body.as_bytes();
                    let mut nesting = 0;
                    for i in 0..bytes.len().saturating_sub(1) {
                        if bytes[i] == b'(' {
                            nesting += 1;
                        }
                        if bytes[i] == b')' {
                            nesting -= 1;
                        }
                        if nesting == 0
                            && i > 0
                            && bytes[i..].starts_with(b"of")
                            && bytes[i - 1].is_ascii_whitespace()
                            && bytes.get(i + 2).is_some_and(u8::is_ascii_whitespace)
                        {
                            split = Some(i);
                            break;
                        }
                    }
                    let (position, filter) =
                        split.map_or((body, None), |i| (&body[..i], Some(&body[i + 2..])));
                    super::position::parse_formula(position)?;
                    let mut ordinal =
                        json!({"position":position.trim(),"reverse":name == "nth-last-child"});
                    if let Some(filter) = filter {
                        ordinal["ofRule"] = parse_at(filter, self.depth)?;
                    }
                    json!({"nthChild":ordinal})
                }
                _ => return Err(self.invalid()),
            };
            self.space();
            if self.peek() != Some(b')') {
                return Err(self.invalid());
            }
            self.at += 1;
            self.depth -= 1;
            items.push(value);
        }
        if items.is_empty() {
            return Err(self.invalid());
        }
        Ok(all(items))
    }
}
