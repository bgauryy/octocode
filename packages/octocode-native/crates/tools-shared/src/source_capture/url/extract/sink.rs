//! The html5ever tree builder sink that fills the document arena.
use super::dom::{Data, Node};
use html5ever::tendril::StrTendril;
use html5ever::tree_builder::{ElemName, ElementFlags, NodeOrText, QuirksMode, TreeSink};
use html5ever::{Attribute, LocalName, Namespace, QualName};
use std::borrow::Cow;
use std::cell::RefCell;

/// An element's name as the tree builder asks for it.
#[derive(Debug)]
pub(super) struct Name {
    ns: Namespace,
    local: LocalName,
}

impl ElemName for Name {
    fn ns(&self) -> &Namespace {
        &self.ns
    }
    fn local_name(&self) -> &LocalName {
        &self.local
    }
}

pub(super) struct Sink {
    pub(super) nodes: RefCell<Vec<Node>>,
}

impl Sink {
    fn push(&self, data: Data) -> usize {
        let mut nodes = self.nodes.borrow_mut();
        nodes.push(Node {
            parent: None,
            children: Vec::new(),
            data,
        });
        nodes.len() - 1
    }

    fn detach(&self, id: usize) {
        let mut nodes = self.nodes.borrow_mut();
        if let Some(parent) = nodes[id].parent.take() {
            nodes[parent].children.retain(|child| *child != id);
        }
    }

    /// Insert `child` under `parent` at `index` (the end when `None`),
    /// merging text into an adjacent text node as the DOM does.
    fn insert(&self, parent: usize, index: Option<usize>, child: NodeOrText<usize>) {
        match child {
            NodeOrText::AppendNode(id) => {
                self.detach(id);
                let mut nodes = self.nodes.borrow_mut();
                let children = &mut nodes[parent].children;
                let at = index.unwrap_or(children.len()).min(children.len());
                children.insert(at, id);
                nodes[id].parent = Some(parent);
            }
            NodeOrText::AppendText(text) => {
                let mut nodes = self.nodes.borrow_mut();
                let at = index.unwrap_or(nodes[parent].children.len());
                let previous = at
                    .checked_sub(1)
                    .and_then(|before| nodes[parent].children.get(before).copied());
                if let Some(previous) = previous
                    && let Data::Text(existing) = &mut nodes[previous].data
                {
                    existing.push_str(&text);
                    return;
                }
                drop(nodes);
                let id = self.push(Data::Text(text.to_string()));
                let mut nodes = self.nodes.borrow_mut();
                nodes[parent].children.insert(at, id);
                nodes[id].parent = Some(parent);
            }
        }
    }
}

impl TreeSink for Sink {
    type Handle = usize;
    type Output = Vec<Node>;
    type ElemName<'a> = Name;

    fn finish(self) -> Vec<Node> {
        self.nodes.into_inner()
    }

    fn parse_error(&self, _msg: Cow<'static, str>) {}

    fn get_document(&self) -> usize {
        0
    }

    fn elem_name<'a>(&'a self, target: &'a usize) -> Name {
        match &self.nodes.borrow()[*target].data {
            Data::Element { name, .. } => Name {
                ns: name.ns.clone(),
                local: name.local.clone(),
            },
            _ => Name {
                ns: Namespace::from(""),
                local: LocalName::from(""),
            },
        }
    }

    fn create_element(&self, name: QualName, attrs: Vec<Attribute>, flags: ElementFlags) -> usize {
        let contents = flags.template.then(|| self.push(Data::Document));
        self.push(Data::Element {
            name,
            attrs,
            contents,
        })
    }

    fn create_comment(&self, _text: StrTendril) -> usize {
        self.push(Data::Other)
    }

    fn create_pi(&self, _target: StrTendril, _data: StrTendril) -> usize {
        self.push(Data::Other)
    }

    fn append(&self, parent: &usize, child: NodeOrText<usize>) {
        self.insert(*parent, None, child);
    }

    fn append_based_on_parent_node(
        &self,
        element: &usize,
        prev_element: &usize,
        child: NodeOrText<usize>,
    ) {
        if self.nodes.borrow()[*element].parent.is_some() {
            self.append_before_sibling(element, child);
        } else {
            self.append(prev_element, child);
        }
    }

    fn append_doctype_to_document(&self, _: StrTendril, _: StrTendril, _: StrTendril) {}

    fn get_template_contents(&self, target: &usize) -> usize {
        match &self.nodes.borrow()[*target].data {
            Data::Element {
                contents: Some(contents),
                ..
            } => *contents,
            _ => *target,
        }
    }

    fn same_node(&self, x: &usize, y: &usize) -> bool {
        x == y
    }

    fn set_quirks_mode(&self, _mode: QuirksMode) {}

    fn append_before_sibling(&self, sibling: &usize, child: NodeOrText<usize>) {
        let Some(parent) = self.nodes.borrow()[*sibling].parent else {
            return;
        };
        // A moved node may sit before the sibling: locate after detaching.
        if let NodeOrText::AppendNode(id) = &child {
            self.detach(*id);
        }
        let index = self.nodes.borrow()[parent]
            .children
            .iter()
            .position(|child| child == sibling);
        self.insert(parent, index, child);
    }

    fn add_attrs_if_missing(&self, target: &usize, attrs: Vec<Attribute>) {
        if let Data::Element {
            attrs: existing, ..
        } = &mut self.nodes.borrow_mut()[*target].data
        {
            for attr in attrs {
                if !existing.iter().any(|present| present.name == attr.name) {
                    existing.push(attr);
                }
            }
        }
    }

    fn remove_from_parent(&self, target: &usize) {
        self.detach(*target);
    }

    fn reparent_children(&self, node: &usize, new_parent: &usize) {
        let mut nodes = self.nodes.borrow_mut();
        let children = std::mem::take(&mut nodes[*node].children);
        for child in &children {
            nodes[*child].parent = Some(*new_parent);
        }
        nodes[*new_parent].children.extend(children);
    }
}
