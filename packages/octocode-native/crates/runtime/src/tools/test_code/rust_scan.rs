//! What a Rust source too large for the declarations parse says about test
//! code, read token by token in one linear pass: the same test spans and file
//! modules the parse reports, so such a file is classified, not guessed at. The
//! parse still decides what an attribute means: each distinct `cfg` or `path`
//! attribute is parsed on its own, so there is one rule for what compiles only
//! under test.
use octocode_engine::graph::GraphFactRustModule;
use std::collections::HashMap;

mod attributes;
mod tokens;

use attributes::Meaning;
use tokens::{Lexeme, Token, Tokens};

/// What the pass found: 1-based line spans compiled only under test, and every
/// `mod` the source declares.
pub(super) struct Scanned {
    pub(super) test_lines: Vec<[u32; 2]>,
    pub(super) modules: Vec<GraphFactRustModule>,
}

pub(super) fn scan(content: &str) -> Scanned {
    let mut pass = Pass {
        content,
        tokens: Tokens::new(content),
        stack: Vec::new(),
        scope: Vec::new(),
        pending: Pending::default(),
        node_line: None,
        span: None,
        whole: false,
        inline_name: None,
        last_end: 1,
        meanings: HashMap::new(),
        out: Scanned {
            test_lines: Vec::new(),
            modules: Vec::new(),
        },
    };
    pass.run();
    pass.out
}

/// The outer attributes read so far for the node they precede.
#[derive(Default)]
struct Pending {
    /// A `cfg` requires test: the line after it, where what it gates can start
    /// (the parse gates a comment or attribute between it and the item too).
    test_from: Option<u32>,
    path: Option<String>,
    test_path: Option<String>,
}

/// One open delimiter.
struct Frame {
    closer: u8,
    line: u32,
    /// It opened the body of `mod name`, whose name is on the scope.
    module: bool,
    /// What it holds are items (a module, or an item-level macro's body).
    items: bool,
    /// A macro body the parse does not read as items: its tokens mean nothing.
    opaque: bool,
    outer_pending: Pending,
    outer_node_line: Option<u32>,
}

/// How the node a test span covers ends, past `;` and the closing of what
/// holds it.
#[derive(Clone, Copy, PartialEq)]
enum Ends {
    /// Its first word is still to come (`pub` may precede it).
    Undecided,
    /// At a `}` too: a `fn`, `impl`, `struct` and the like.
    Item,
    /// Only at `;`: its braces are part of a value (`const X: T = T { .. };`).
    Semicolon,
    /// At a `}` or, outside `<...>`, a `,`: a field, variant, arm or argument.
    Listed { angles: u32 },
}

struct Span {
    start: u32,
    depth: usize,
    ends: Ends,
}

struct Pass<'t> {
    content: &'t str,
    tokens: Tokens<'t>,
    stack: Vec<Frame>,
    scope: Vec<String>,
    pending: Pending,
    /// The line of the first token of the node being read.
    node_line: Option<u32>,
    span: Option<Span>,
    /// An inner `cfg` of the file holds all of it.
    whole: bool,
    /// The `{` to come opens the body of this `mod`.
    inline_name: Option<String>,
    last_end: u32,
    meanings: HashMap<&'t str, Meaning>,
    out: Scanned,
}

impl<'t> Pass<'t> {
    fn run(&mut self) {
        while let Some(lexeme) = self.tokens.next() {
            if !self.attribute(lexeme) {
                self.token(lexeme);
            }
            self.last_end = self.last_end.max(lexeme.end_line);
        }
        if let Some(span) = self.span.take() {
            self.out.test_lines.push([span.start, self.last_end]);
        }
        if self.whole {
            let last = octocode_engine::num::saturating_u32(self.content.lines().count().max(1));
            self.out.test_lines.push([1, last]);
        }
    }

    fn opaque(&self) -> bool {
        self.stack.last().is_some_and(|frame| frame.opaque)
    }

    fn token(&mut self, lexeme: Lexeme<'t>) {
        match lexeme.token {
            Token::Punct(closer @ (b')' | b']' | b'}')) => {
                self.close(closer, lexeme);
                return;
            }
            _ => self.start_node(lexeme),
        }
        match lexeme.token {
            Token::Punct(opener @ (b'(' | b'[' | b'{')) => self.open(opener, lexeme, false),
            Token::Punct(b';') => {
                self.end_span_at(lexeme.end_line, |_| true);
                self.next_node();
            }
            Token::Punct(b',') => {
                let before = self.last_end;
                self.end_span_at(before, |ends| matches!(ends, Ends::Listed { angles: 0 }));
                self.next_node();
            }
            Token::Punct(b'<') => self.count_angle(true),
            // The parse gates a match arm's pattern, not its body.
            Token::Punct(b'>') if self.tokens.follows(lexeme.start, b'=') => {
                let before = self.last_end;
                self.end_span_at(before, |ends| matches!(ends, Ends::Listed { .. }));
            }
            Token::Punct(b'>') if !self.tokens.follows(lexeme.start, b'-') => {
                self.count_angle(false);
            }
            Token::Word(word) => self.word(word, lexeme),
            _ => {}
        }
    }

    /// The first token of a node opens the span an attribute before it asked
    /// for.
    fn start_node(&mut self, lexeme: Lexeme<'t>) {
        if self.node_line.is_none() {
            self.node_line = Some(lexeme.line);
        }
        if let Some(from) = self.pending.test_from.take()
            && self.span.is_none()
            && !self.whole
        {
            self.span = Some(Span {
                start: from.min(lexeme.line),
                depth: self.stack.len(),
                ends: Ends::Undecided,
            });
        }
        if let Some(span) = self.span.as_mut()
            && span.depth == self.stack.len()
            && span.ends == Ends::Undecided
        {
            let ahead = self.tokens.clone().next().map(|next| next.token);
            span.ends = match lexeme.token {
                Token::Word("pub") | Token::Punct(b'(') => Ends::Undecided,
                Token::Word("const")
                    if matches!(
                        ahead,
                        Some(Token::Word("fn" | "unsafe" | "extern" | "async"))
                    ) =>
                {
                    Ends::Item
                }
                Token::Word("const" | "static" | "let" | "type" | "use") => Ends::Semicolon,
                Token::Word(
                    "fn" | "impl" | "struct" | "enum" | "trait" | "union" | "mod" | "extern"
                    | "unsafe" | "async" | "macro_rules",
                ) => Ends::Item,
                _ => Ends::Listed { angles: 0 },
            };
        }
    }

    fn next_node(&mut self) {
        self.node_line = None;
        self.pending = Pending::default();
        self.inline_name = None;
    }

    fn count_angle(&mut self, opens: bool) {
        if let Some(Span {
            depth,
            ends: Ends::Listed { angles },
            ..
        }) = self.span.as_mut()
            && *depth == self.stack.len()
        {
            *angles = if opens {
                angles.saturating_add(1)
            } else {
                angles.saturating_sub(1)
            };
        }
    }

    fn end_span_at(&mut self, line: u32, ends: impl Fn(Ends) -> bool) {
        if let Some(span) = &self.span
            && span.depth == self.stack.len()
            && ends(span.ends)
        {
            self.out.test_lines.push([span.start, line]);
            self.span = None;
        }
    }

    fn word(&mut self, word: &'t str, lexeme: Lexeme<'t>) {
        if word == "mod" && !self.opaque() {
            self.module();
            return;
        }
        // A macro call: `name! (..)`, or `macro_rules! name { .. }`.
        let mut ahead = self.tokens.clone();
        if ahead
            .next()
            .is_none_or(|next| next.token != Token::Punct(b'!'))
        {
            return;
        }
        let mut body = ahead.next();
        if word == "macro_rules" {
            body = ahead.next();
        }
        let Some(body) = body else {
            return;
        };
        if let Token::Punct(opener @ (b'(' | b'[' | b'{')) = body.token {
            self.tokens = ahead;
            self.last_end = body.end_line;
            let items = word != "macro_rules"
                && !self.opaque()
                && self.stack.last().is_none_or(|frame| frame.items);
            self.open(opener, lexeme, !items);
            if let Some(frame) = self.stack.last_mut() {
                frame.items = items;
                frame.line = body.line;
            }
        }
    }

    /// `mod name;` or `mod name {`.
    fn module(&mut self) {
        let mut ahead = self.tokens.clone();
        let Some(Token::Word(name)) = ahead.next().map(|next| next.token) else {
            return;
        };
        let inline = match ahead.next().map(|next| next.token) {
            Some(Token::Punct(b';')) => false,
            Some(Token::Punct(b'{')) => true,
            _ => return,
        };
        self.out.modules.push(GraphFactRustModule {
            name: name.to_owned(),
            line: self.node_line.unwrap_or(self.last_end),
            scope: self.scope.clone(),
            inline,
            path: self.pending.path.take(),
            test_path: self.pending.test_path.take(),
            unsupported: false,
        });
        self.inline_name = inline.then(|| name.to_owned());
    }

    fn open(&mut self, opener: u8, lexeme: Lexeme<'t>, opaque: bool) {
        let module = opener == b'{' && self.inline_name.is_some();
        if let Some(name) = self.inline_name.take().filter(|_| module) {
            self.scope.push(name);
        }
        self.stack.push(Frame {
            closer: match opener {
                b'(' => b')',
                b'[' => b']',
                _ => b'}',
            },
            line: lexeme.line,
            module,
            items: module,
            opaque: opaque || self.opaque(),
            outer_pending: std::mem::take(&mut self.pending),
            outer_node_line: self.node_line.take(),
        });
    }

    fn close(&mut self, closer: u8, lexeme: Lexeme<'t>) {
        let Some(at) = self.stack.iter().rposition(|frame| frame.closer == closer) else {
            return;
        };
        let before = self.last_end;
        while self.stack.len() > at {
            let Some(frame) = self.stack.pop() else {
                break;
            };
            if frame.module {
                self.scope.pop();
            }
            if self.stack.len() == at {
                self.pending = frame.outer_pending;
                self.node_line = frame.outer_node_line;
            }
            // What the frame held ends with it.
            if let Some(span) = &self.span
                && span.depth > self.stack.len()
            {
                self.out.test_lines.push([span.start, before]);
                self.span = None;
            }
        }
        if closer == b'}' {
            let continues =
                self.tokens.clone().next().map(|next| next.token) == Some(Token::Word("else"));
            if !continues {
                self.end_span_at(lexeme.end_line, |ends| ends != Ends::Semicolon);
                let value = self
                    .span
                    .as_ref()
                    .is_some_and(|span| span.depth == self.stack.len());
                if !value {
                    self.next_node();
                }
            }
        }
    }
}
