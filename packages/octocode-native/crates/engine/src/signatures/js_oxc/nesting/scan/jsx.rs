//! JSX for the nesting pass: an element is a level, its attributes and
//! children tokens of it, and its text is skipped as oxc skips it. A TSX
//! generic arrow (`<T,>`, `<T extends U>`) or anything else JSX does not
//! spell leaves the pass unsure.

use super::{Kind, Prev, Scan, Step, Stop, is_word_byte};

fn is_name_byte(byte: u8) -> bool {
    (is_word_byte(byte) && byte != b'\\') || matches!(byte, b'-' | b'.' | b':')
}

impl Scan<'_> {
    fn skip_space(&mut self) {
        while matches!(self.peek(0), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            self.at += 1;
        }
    }

    fn jsx_name(&mut self) -> &[u8] {
        let start = self.at;
        while self.peek(0).is_some_and(is_name_byte) {
            self.at += 1;
        }
        self.bytes.get(start..self.at).unwrap_or_default()
    }

    /// Past the `<` of an element: a fragment (`<>`) or a named tag.
    pub(super) fn jsx_open(&mut self) -> Step {
        self.skip_space();
        if self.peek(0) == Some(b'>') {
            self.at += 1;
            return self.push(Kind::Jsx { children: true });
        }
        if !self.peek(0).is_some_and(is_name_byte) {
            return Err(Stop);
        }
        self.push(Kind::Jsx { children: false })?;
        self.jsx_tag_name()
    }

    /// A tag's name, one token per part: each part of `a.b.c` or `a:b` is a
    /// level of the name's syntax tree.
    fn jsx_tag_name(&mut self) -> Step {
        let name = self.jsx_name();
        let parts = 1 + name
            .iter()
            .filter(|&&byte| matches!(byte, b'.' | b':'))
            .count();
        for _ in 0..parts {
            self.count(false)?;
        }
        Ok(())
    }

    pub(super) fn jsx_step(&mut self, children: bool) -> Step {
        if children {
            self.jsx_children()
        } else {
            self.jsx_attribute()
        }
    }

    fn jsx_expression(&mut self, children: bool) -> Step {
        self.at += 1;
        self.push(Kind::JsxExpr { children })?;
        self.prev = Prev::Expression;
        Ok(())
    }

    /// One attribute of a tag, or the tag's end.
    fn jsx_attribute(&mut self) -> Step {
        self.skip_space();
        match self.peek(0).ok_or(Stop)? {
            b'/' if self.peek(1) == Some(b'>') => {
                self.at += 2;
                self.jsx_end()
            }
            b'>' => {
                self.at += 1;
                self.frames.last_mut().ok_or(Stop)?.kind = Kind::Jsx { children: true };
                Ok(())
            }
            b'{' => self.jsx_expression(false),
            byte if is_name_byte(byte) => {
                if self.jsx_name() == b"extends" {
                    return Err(Stop);
                }
                self.count(false)?;
                self.skip_space();
                if self.peek(0) != Some(b'=') {
                    return Ok(());
                }
                self.at += 1;
                self.skip_space();
                match self.peek(0).ok_or(Stop)? {
                    b'{' => self.jsx_expression(false),
                    quote @ (b'"' | b'\'') => {
                        self.count(false)?;
                        self.at += 1;
                        let rest = self.rest();
                        let end = rest.iter().position(|&byte| byte == quote).ok_or(Stop)?;
                        self.at += end + 1;
                        Ok(())
                    }
                    _ => Err(Stop),
                }
            }
            _ => Err(Stop),
        }
    }

    /// Text up to the next child: an expression, an element, or the closing
    /// tag.
    fn jsx_children(&mut self) -> Step {
        loop {
            match self.peek(0).ok_or(Stop)? {
                b'{' => return self.jsx_expression(true),
                b'<' => break,
                b'}' | b'>' => return Err(Stop),
                _ => self.at += 1,
            }
        }
        self.at += 1;
        self.skip_space();
        if self.peek(0) != Some(b'/') {
            return self.jsx_open();
        }
        self.at += 1;
        self.skip_space();
        self.jsx_tag_name()?;
        self.skip_space();
        if self.peek(0) != Some(b'>') {
            return Err(Stop);
        }
        self.at += 1;
        self.jsx_end()
    }

    /// The element ended: what follows belongs to its parent.
    fn jsx_end(&mut self) -> Step {
        match self.pop()? {
            Kind::Jsx { .. } => {
                self.prev = Prev::Operand;
                Ok(())
            }
            _ => Err(Stop),
        }
    }
}
