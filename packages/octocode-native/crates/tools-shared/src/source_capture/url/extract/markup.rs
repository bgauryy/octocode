//! How an element contributes to page text, and whitespace collapsing.
/// What an element contributes to the text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    /// No page text: dropped with its subtree.
    Skip,
    Heading(u8),
    Pre,
    Row,
    Item,
    Break,
    Block,
    Inline,
}

pub(super) fn kind(local: &str) -> Kind {
    match local {
        "script" | "style" | "noscript" | "template" | "svg" | "nav" | "footer" | "head" => {
            Kind::Skip
        }
        "h1" => Kind::Heading(1),
        "h2" => Kind::Heading(2),
        "h3" => Kind::Heading(3),
        "h4" => Kind::Heading(4),
        "h5" => Kind::Heading(5),
        "h6" => Kind::Heading(6),
        "pre" => Kind::Pre,
        "tr" => Kind::Row,
        "li" => Kind::Item,
        "br" | "hr" => Kind::Break,
        "address" | "article" | "aside" | "blockquote" | "body" | "caption" | "center" | "dd"
        | "details" | "dialog" | "dir" | "div" | "dl" | "dt" | "fieldset" | "figcaption"
        | "figure" | "form" | "header" | "hgroup" | "html" | "legend" | "main" | "menu" | "ol"
        | "p" | "search" | "section" | "summary" | "table" | "tbody" | "td" | "tfoot" | "th"
        | "thead" | "ul" => Kind::Block,
        _ => Kind::Inline,
    }
}

/// Whitespace-collapsing text: runs of whitespace become one space, with
/// none at either end.
#[derive(Default)]
pub(super) struct Collapsed {
    pub(super) text: String,
    space: bool,
}

impl Collapsed {
    pub(super) fn push(&mut self, raw: &str) {
        for ch in raw.chars() {
            // HTML collapses ASCII whitespace only; a no-break space stays.
            if matches!(ch, ' ' | '\t' | '\n' | '\r' | '\x0C') {
                self.space = self.space || !self.text.is_empty();
            } else {
                if self.space {
                    self.text.push(' ');
                    self.space = false;
                }
                self.text.push(ch);
            }
        }
    }

    /// A boundary between blocks inside one line (a table cell, a heading).
    pub(super) fn gap(&mut self) {
        self.space = self.space || !self.text.is_empty();
    }

    pub(super) fn take(&mut self) -> String {
        self.space = false;
        std::mem::take(&mut self.text)
    }
}
