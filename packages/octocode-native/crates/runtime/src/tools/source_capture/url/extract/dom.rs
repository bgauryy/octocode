//! The parsed document arena and its lookups.
use super::PARSE_CHUNK_BYTES;
use super::markup::{Collapsed, Kind, kind};
use super::sink::Sink;
use html5ever::tendril::{StrTendril, TendrilSink};
use html5ever::{Attribute, QualName, parse_document};
use std::cell::RefCell;

pub(super) enum Data {
    Document,
    Element {
        name: QualName,
        attrs: Vec<Attribute>,
        /// A template's contents fragment.
        contents: Option<usize>,
    },
    Text(String),
    Other,
}

pub(super) struct Node {
    pub(super) parent: Option<usize>,
    pub(super) children: Vec<usize>,
    pub(super) data: Data,
}

/// A parsed document: an index arena the tree builder fills.
pub(super) struct Dom {
    pub(super) nodes: Vec<Node>,
}

impl Dom {
    /// The parsed document, or `None` when `stop` ends the parse.
    pub(super) fn parse(source: &str, stop: &dyn Fn() -> bool) -> Option<Self> {
        let sink = Sink {
            nodes: RefCell::new(vec![Node {
                parent: None,
                children: Vec::new(),
                data: Data::Document,
            }]),
        };
        let mut parser = parse_document(sink, html5ever::ParseOpts::default());
        let mut rest = source;
        while !rest.is_empty() {
            if stop() {
                return None;
            }
            let cut = rest.ceil_char_boundary(rest.len().min(PARSE_CHUNK_BYTES));
            parser.process(StrTendril::from(&rest[..cut]));
            rest = &rest[cut..];
        }
        Some(Self {
            nodes: parser.finish(),
        })
    }

    pub(super) fn local(&self, id: usize) -> Option<&str> {
        match &self.nodes[id].data {
            Data::Element { name, .. } => Some(name.local.as_ref()),
            _ => None,
        }
    }

    fn attr(&self, id: usize, attr: &str) -> Option<&str> {
        match &self.nodes[id].data {
            Data::Element { attrs, .. } => attrs
                .iter()
                .find(|candidate| &*candidate.name.local == attr)
                .map(|found| found.value.as_ref()),
            _ => None,
        }
    }

    /// Every node in the document tree, in document order.
    fn in_tree(&self) -> Vec<usize> {
        let mut order = Vec::new();
        let mut stack = vec![0];
        while let Some(id) = stack.pop() {
            order.push(id);
            stack.extend(self.nodes[id].children.iter().rev());
        }
        order
    }

    pub(super) fn body(&self) -> usize {
        self.in_tree()
            .into_iter()
            .find(|id| self.local(*id) == Some("body"))
            .unwrap_or(0)
    }

    pub(super) fn has_script(&self) -> bool {
        self.in_tree()
            .into_iter()
            .any(|id| self.local(id) == Some("script"))
    }

    pub(super) fn anchors(&self, fragment: &str) -> Vec<usize> {
        self.in_tree()
            .into_iter()
            .filter(|id| {
                self.attr(*id, "id") == Some(fragment)
                    || (self.local(*id) == Some("a") && self.attr(*id, "name") == Some(fragment))
            })
            .collect()
    }

    /// `id` itself or its nearest heading ancestor, when that is a heading.
    pub(super) fn heading_at_or_above(&self, id: usize) -> Option<usize> {
        let mut cursor = Some(id);
        while let Some(node) = cursor {
            if self
                .local(node)
                .is_some_and(|local| matches!(kind(local), Kind::Heading(_)))
            {
                return Some(node);
            }
            cursor = self.nodes[node].parent;
        }
        None
    }

    pub(super) fn is_within(&self, id: usize, ancestor: usize) -> bool {
        let mut cursor = Some(id);
        while let Some(node) = cursor {
            if node == ancestor {
                return true;
            }
            cursor = self.nodes[node].parent;
        }
        false
    }

    /// One line of the subtree's collapsed text; nested blocks are spaced.
    pub(super) fn inline_text(&self, root: usize) -> String {
        let mut text = Collapsed::default();
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            match &self.nodes[id].data {
                Data::Text(raw) => text.push(raw),
                Data::Element { name, .. } => {
                    let kind = kind(name.local.as_ref());
                    if kind == Kind::Skip {
                        continue;
                    }
                    if kind != Kind::Inline {
                        text.gap();
                    }
                    stack.extend(self.nodes[id].children.iter().rev());
                }
                Data::Document | Data::Other => {}
            }
        }
        text.take()
    }

    /// The subtree's text exactly as written; `<br>` is a line break.
    pub(super) fn raw_text(&self, root: usize) -> String {
        let mut text = String::new();
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            match &self.nodes[id].data {
                Data::Text(raw) => text.push_str(raw),
                Data::Element { name, .. } => match kind(name.local.as_ref()) {
                    Kind::Skip => {}
                    Kind::Break => text.push('\n'),
                    _ => stack.extend(self.nodes[id].children.iter().rev()),
                },
                Data::Document | Data::Other => {}
            }
        }
        text.replace("\r\n", "\n")
    }
}
