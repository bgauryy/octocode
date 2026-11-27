//! The document walk that renders page text as one line per block.
use super::Line;
use super::dom::{Data, Dom};
use super::markup::{Collapsed, Kind, kind};

pub(super) struct Walked {
    pub(super) lines: Vec<Line>,
    /// The target element's lines, `[start, end)`.
    pub(super) span: Option<(usize, usize)>,
}

enum Step {
    Enter(usize),
    Exit(usize, Kind),
}

pub(super) struct Walker<'a> {
    dom: &'a Dom,
    lines: Vec<Line>,
    current: Collapsed,
    /// One entry per open list item: whether its marker line is written.
    items: Vec<bool>,
    target: Option<usize>,
    start: usize,
    span: Option<(usize, usize)>,
}

impl<'a> Walker<'a> {
    pub(super) fn new(dom: &'a Dom, target: Option<usize>) -> Self {
        Self {
            dom,
            lines: Vec::new(),
            current: Collapsed::default(),
            items: Vec::new(),
            target,
            start: 0,
            span: None,
        }
    }

    pub(super) fn walk(mut self, root: usize) -> Walked {
        let mut stack = vec![Step::Enter(root)];
        while let Some(step) = stack.pop() {
            match step {
                Step::Enter(id) => self.enter(id, &mut stack),
                Step::Exit(id, kind) => self.exit(id, kind),
            }
        }
        self.flush();
        Walked {
            lines: self.lines,
            span: self.span,
        }
    }

    fn enter(&mut self, id: usize, stack: &mut Vec<Step>) {
        let node = &self.dom.nodes[id];
        let local = match &node.data {
            Data::Text(text) => {
                self.current.push(text);
                return;
            }
            Data::Element { name, .. } => name.local.as_ref(),
            Data::Document => "",
            Data::Other => return,
        };
        let kind = kind(local);
        match kind {
            Kind::Skip => self.unit(id, |_| {}),
            Kind::Heading(level) => self.unit(id, |walker| {
                let text = walker.dom.inline_text(id);
                if !text.is_empty() {
                    walker.lines.push(Line {
                        text: format!("{} {text}", "#".repeat(usize::from(level))),
                        level: Some(level),
                    });
                }
            }),
            Kind::Pre => self.unit(id, |walker| {
                let raw = walker.dom.raw_text(id);
                let lines: Vec<&str> = raw.lines().collect();
                let first = lines.iter().position(|line| !line.trim().is_empty());
                let last = lines.iter().rposition(|line| !line.trim().is_empty());
                if let (Some(first), Some(last)) = (first, last) {
                    walker.push_line("```".into(), None);
                    for line in &lines[first..=last] {
                        walker.push_line((*line).to_owned(), None);
                    }
                    walker.push_line("```".into(), None);
                }
            }),
            Kind::Row => self.unit(id, |walker| {
                let cells: Vec<String> = walker.dom.nodes[id]
                    .children
                    .iter()
                    .filter(|child| matches!(walker.dom.local(**child), Some("td" | "th")))
                    .map(|child| walker.dom.inline_text(*child))
                    .collect();
                if cells.iter().any(|cell| !cell.is_empty()) {
                    walker.push_line(cells.join(" | "), None);
                }
            }),
            Kind::Break => self.flush(),
            Kind::Item | Kind::Block | Kind::Inline => {
                if kind != Kind::Inline {
                    self.flush();
                }
                if kind == Kind::Item {
                    self.items.push(false);
                }
                if self.target == Some(id) {
                    self.start = self.lines.len();
                }
                stack.push(Step::Exit(id, kind));
                for child in self.dom.nodes[id].children.iter().rev() {
                    stack.push(Step::Enter(*child));
                }
            }
        }
    }

    fn exit(&mut self, id: usize, kind: Kind) {
        if kind != Kind::Inline {
            self.flush();
        }
        if kind == Kind::Item {
            self.items.pop();
        }
        if self.target == Some(id) {
            let pending = usize::from(!self.current.text.is_empty());
            self.span = Some((self.start, self.lines.len() + pending));
        }
    }

    /// An element rendered whole by `render`; the target's span when the
    /// target is the element or inside it.
    fn unit(&mut self, id: usize, render: impl FnOnce(&mut Self)) {
        self.flush();
        let start = self.lines.len();
        render(self);
        if self
            .target
            .is_some_and(|target| self.dom.is_within(target, id))
        {
            self.span = Some((start, self.lines.len()));
        }
    }

    fn flush(&mut self) {
        let text = self.current.take();
        if !text.is_empty() {
            self.push_line(text, None);
        }
    }

    fn push_line(&mut self, text: String, level: Option<u8>) {
        let depth = self.items.len();
        let text = match self.items.last_mut() {
            Some(marked) if level.is_none() => {
                let indent = "  ".repeat(depth - 1);
                if *marked {
                    format!("{indent}  {text}")
                } else {
                    *marked = true;
                    format!("{indent}- {text}")
                }
            }
            _ => text,
        };
        self.lines.push(Line { text, level });
    }
}
