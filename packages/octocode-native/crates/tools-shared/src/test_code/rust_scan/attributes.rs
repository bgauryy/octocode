//! The attributes a scan stops at: what a `cfg` or `path` means is the parse's
//! to say, so each distinct one is parsed on its own.
use super::tokens::{Lexeme, Token};
use super::{Ends, Pass, Span};

/// What the parse says one attribute means.
#[derive(Clone, Default)]
pub(super) struct Meaning {
    test: bool,
    path: Option<String>,
    test_path: Option<String>,
    plain_path: Option<String>,
}

impl<'t> Pass<'t> {
    /// Reads the attribute `hash` opens, if it does.
    pub(super) fn attribute(&mut self, hash: Lexeme<'t>) -> bool {
        if hash.token != Token::Punct(b'#') {
            return false;
        }
        let mut ahead = self.tokens.clone();
        let mut next = ahead.next();
        let inner = next.is_some_and(|lexeme| lexeme.token == Token::Punct(b'!'));
        if inner {
            next = ahead.next();
        }
        if next.is_none_or(|lexeme| lexeme.token != Token::Punct(b'[')) {
            return false;
        }
        let name = ahead.clone().next().map(|lexeme| lexeme.token);
        let mut depth = 1_u32;
        let mut end = None;
        while let Some(lexeme) = ahead.next() {
            match lexeme.token {
                Token::Punct(b'[') => depth += 1,
                Token::Punct(b']') => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(lexeme);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(end) = end else {
            return false;
        };
        self.tokens = ahead;
        if !inner && !self.opaque() && self.pending.attributes_from.is_none() {
            // `hash` is the ASCII '#' token, hence a character boundary.
            let before = self.content.split_at(hash.start).0;
            if before
                .rsplit('\n')
                .next()
                .is_some_and(|line| line.trim().is_empty())
            {
                self.pending.attributes_from = Some(hash.line);
            }
        }
        self.last_end = end.end_line;
        // An outer attribute gates the node that follows it: one after the
        // node's first token (`if #[cfg(test)] { .. }` in a macro body) is
        // tokens the parse reads no meaning into.
        let mid_node = !inner && self.node_line.is_some();
        if self.opaque()
            || mid_node
            || !matches!(name, Some(Token::Word("cfg" | "path" | "cfg_attr")))
        {
            return true;
        }
        // The accepted ASCII '#' and ']' tokens bound the entire attribute.
        let tail = self.content.split_at(hash.start).1;
        let attribute = tail.split_at(end.start - hash.start + 1).0;
        let meaning = self.meaning(attribute);
        if inner {
            if meaning.test {
                self.gate_enclosing();
            }
        } else {
            if meaning.test {
                self.pending.test_from.get_or_insert(end.end_line + 1);
            }
            if meaning.path.is_some() {
                self.pending.path = meaning.path;
            }
            if meaning.test_path.is_some() {
                self.pending.test_path = meaning.test_path;
            }
            if meaning.plain_path.is_some() {
                self.pending.plain_path = meaning.plain_path;
            }
        }
        true
    }

    fn meaning(&mut self, attribute: &'t str) -> Meaning {
        self.meanings
            .entry(attribute)
            .or_insert_with(|| {
                let probe = format!("{attribute}\nmod probe;\n");
                crate::declarations::file_document(&probe, "probe.rs")
                    .map(|document| Meaning {
                        test: !document.test_lines.is_empty(),
                        path: document
                            .modules
                            .first()
                            .and_then(|module| module.path.clone()),
                        test_path: document
                            .modules
                            .first()
                            .and_then(|module| module.test_path.clone()),
                        plain_path: document
                            .modules
                            .first()
                            .and_then(|module| module.plain_path.clone()),
                    })
                    .unwrap_or_default()
            })
            .clone()
    }

    /// An inner `cfg` that requires test gates what holds it.
    fn gate_enclosing(&mut self) {
        if self.span.is_some() || self.whole {
            return;
        }
        match self.stack.last() {
            Some(frame) => {
                self.span = Some(Span {
                    start: frame.line,
                    depth: self.stack.len() - 1,
                    ends: Ends::Item,
                });
            }
            None => self.whole = true,
        }
    }
}
