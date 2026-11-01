//! Test code inside production trees: modules mounted only under test, and
//! which edges are test evidence.
use super::*;
use crate::tools::ast_graph::edges::mounts;

impl<'a> Projector<'a> {
    /// Rust modules mounted only under test are test files: a `mod x;` or a
    /// spliced `include!` inside a `#[cfg(test)]` item, or in a test file,
    /// and every module below them. (A module mounted for production as well keeps
    /// its production line as its `mod` evidence, so it stays production.)
    pub(super) fn mount_test_modules(&mut self) {
        let built = self.built;
        let mounts = built
            .nodes
            .iter()
            .flat_map(|(parent, node)| {
                node.edges.iter().flat_map(move |(child, kinds)| {
                    kinds
                        .iter()
                        .filter(|(kind, _)| mounts(kind))
                        .map(move |(_, line)| (parent.as_str(), child.as_str(), *line))
                })
            })
            .collect::<Vec<_>>();
        loop {
            let mut changed = false;
            for (parent, child, line) in &mounts {
                let test_mount = self.has_role(parent, ROLE_TEST)
                    || built
                        .facts
                        .get(*parent)
                        .is_some_and(|facts| facts.test_scoped(*line));
                if test_mount && !self.has_role(child, ROLE_TEST) {
                    self.mark_test(child);
                    changed = true;
                }
            }
            if !changed {
                return;
            }
        }
    }

    fn mark_test(&mut self, file: &'a str) {
        if let Some(&id) = self.file_nodes.get(file) {
            self.nodes[id as usize].flags |= ROLE_TEST;
            *self.roles.entry(file).or_default() |= ROLE_TEST;
        }
    }

    /// Whether an edge from `src` with evidence at `line` is test code: its
    /// file is a test file, or the line lies in an item compiled only under
    /// test.
    pub(super) fn test_evidence(&self, src: u32, line: u32) -> bool {
        let Some(file) = self.nodes.get(src as usize).map(|node| node.file) else {
            return false;
        };
        let Some(record) = self.nodes.get(file as usize) else {
            return false;
        };
        record.flags & ROLE_TEST != 0
            || self
                .built
                .facts
                .get(self.strings.strings[record.key as usize].as_str())
                .is_some_and(|facts| facts.test_scoped(line))
    }
}
