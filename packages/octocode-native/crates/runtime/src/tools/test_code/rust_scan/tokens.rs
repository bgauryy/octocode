//! Rust tokens, as far as telling code from comments and literals takes: words,
//! single punctuation bytes, and literals as one token each. Any text lexes;
//! an unterminated comment or literal runs to the end, as it does for rustc.

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Token<'t> {
    Word(&'t str),
    Punct(u8),
    Literal,
}

#[derive(Clone, Copy)]
pub(super) struct Lexeme<'t> {
    pub(super) token: Token<'t>,
    pub(super) start: usize,
    /// 1-based, where it starts and ends.
    pub(super) line: u32,
    pub(super) end_line: u32,
}

#[derive(Clone)]
pub(super) struct Tokens<'t> {
    text: &'t str,
    bytes: &'t [u8],
    at: usize,
    line: u32,
    /// A comment or literal ran to the end of the text unclosed.
    pub(super) unterminated: bool,
}

fn is_word_byte(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphanumeric() || byte >= 0x80
}

impl<'t> Tokens<'t> {
    pub(super) fn new(text: &'t str) -> Self {
        Self {
            text,
            bytes: text.as_bytes(),
            at: 0,
            line: 1,
            unterminated: false,
        }
    }

    /// Whether the byte before `start` is `byte`: `->` and `=>` are two tokens.
    pub(super) fn follows(&self, start: usize, byte: u8) -> bool {
        start
            .checked_sub(1)
            .and_then(|before| self.bytes.get(before))
            == Some(&byte)
    }

    pub(super) fn next(&mut self) -> Option<Lexeme<'t>> {
        self.skip_trivia();
        let (start, line) = (self.at, self.line);
        let first = *self.bytes.get(start)?;
        let token = if first.is_ascii_digit() {
            self.skip_while(is_word_byte);
            Token::Literal
        } else if is_word_byte(first) {
            self.word()
        } else if first == b'"' {
            self.string();
            Token::Literal
        } else if first == b'\'' {
            self.quote();
            Token::Literal
        } else {
            self.at += 1;
            Token::Punct(first)
        };
        Some(Lexeme {
            token,
            start,
            line,
            end_line: self.line,
        })
    }

    fn peek(&self, ahead: usize) -> Option<u8> {
        self.bytes.get(self.at + ahead).copied()
    }

    fn skip_while(&mut self, keep: impl Fn(u8) -> bool) {
        while self.peek(0).is_some_and(&keep) {
            self.at += 1;
        }
    }

    /// Moves to `to`, counting the lines it passes.
    fn advance_to(&mut self, to: usize) {
        let passed = memchr::memchr_iter(b'\n', &self.bytes[self.at..to]).count();
        self.line = self
            .line
            .saturating_add(octocode_engine::num::saturating_u32(passed));
        self.at = to;
    }

    fn skip_trivia(&mut self) {
        loop {
            match (self.peek(0), self.peek(1)) {
                (Some(b'\n'), _) => {
                    self.at += 1;
                    self.line = self.line.saturating_add(1);
                }
                (Some(byte), _) if byte.is_ascii_whitespace() => self.at += 1,
                (Some(b'/'), Some(b'/')) => {
                    let rest = &self.bytes[self.at..];
                    self.at += memchr::memchr(b'\n', rest).unwrap_or(rest.len());
                }
                (Some(b'/'), Some(b'*')) => self.block_comment(),
                _ => return,
            }
        }
    }

    fn block_comment(&mut self) {
        let start = self.at;
        let mut at = start + 2;
        let mut depth = 1_u32;
        while depth > 0 {
            let Some(next) = memchr::memchr2(b'*', b'/', &self.bytes[at.min(self.bytes.len())..])
            else {
                at = self.bytes.len();
                self.unterminated = true;
                break;
            };
            at += next;
            match (self.bytes[at], self.bytes.get(at + 1)) {
                (b'*', Some(b'/')) => {
                    depth -= 1;
                    at += 2;
                }
                (b'/', Some(b'*')) => {
                    depth += 1;
                    at += 2;
                }
                _ => at += 1,
            }
        }
        self.advance_to(at);
    }

    /// A word, or the literal or raw identifier its prefix opens.
    fn word(&mut self) -> Token<'t> {
        let start = self.at;
        self.skip_while(is_word_byte);
        let word = &self.text[start..self.at];
        match (word, self.peek(0)) {
            ("b" | "c", Some(b'"')) => {
                self.string();
                Token::Literal
            }
            ("b", Some(b'\'')) => {
                self.quote();
                Token::Literal
            }
            ("r" | "br" | "cr", Some(b'"' | b'#')) => {
                let hashes = self.bytes[self.at..]
                    .iter()
                    .take_while(|byte| **byte == b'#')
                    .count();
                if self.peek(hashes) == Some(b'"') {
                    self.raw_string(hashes);
                    Token::Literal
                } else if word == "r" && hashes == 1 && self.peek(1).is_some_and(is_word_byte) {
                    self.at += 1;
                    self.skip_while(is_word_byte);
                    Token::Word(&self.text[start..self.at])
                } else {
                    Token::Word(word)
                }
            }
            _ => Token::Word(word),
        }
    }

    /// A `"` string, the cursor on its opening quote.
    fn string(&mut self) {
        let mut at = self.at + 1;
        while let Some(next) = memchr::memchr2(b'"', b'\\', &self.bytes[at.min(self.bytes.len())..])
        {
            at += next;
            if self.bytes[at] == b'"' {
                self.advance_to(at + 1);
                return;
            }
            at += 2;
        }
        self.unterminated = true;
        self.advance_to(self.bytes.len());
    }

    /// A raw string, the cursor on its `#`s or opening quote.
    fn raw_string(&mut self, hashes: usize) {
        let body = self.at + hashes + 1;
        let close = format!("\"{}", "#".repeat(hashes));
        let found = self.text[body..].find(&close);
        self.unterminated |= found.is_none();
        let end = found.map_or(self.bytes.len(), |at| body + at + close.len());
        self.advance_to(end);
    }

    /// A char literal or a lifetime, the cursor on its quote.
    fn quote(&mut self) {
        let after = self.at + 1;
        if self.bytes.get(after) == Some(&b'\\') {
            let rest = &self.bytes[(after + 2).min(self.bytes.len())..];
            let close =
                memchr::memchr(b'\'', rest).map_or(self.bytes.len(), |at| after + 2 + at + 1);
            self.advance_to(close);
            return;
        }
        let width = self.text[after..].chars().next().map_or(0, char::len_utf8);
        if self.bytes.get(after + width) == Some(&b'\'') {
            self.advance_to(after + width + 1);
        } else {
            self.at = after;
            self.skip_while(is_word_byte);
        }
    }
}
