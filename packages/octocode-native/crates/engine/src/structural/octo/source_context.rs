use super::matching::ExecutionError;
use crate::text::analysis::{
    AnalysisOperation, ByteRange, ObservedSpan, Producer, RepresentationContent,
    RepresentationHandle, SourceTemplates, UnitHandle,
};
use std::{collections::HashMap, sync::Arc};
use tree_sitter::Node;

pub(super) struct SourceContext {
    pub templates: Arc<SourceTemplates>,
    trees: HashMap<usize, UnitHandle>,
}
impl SourceContext {
    #[expect(
        clippy::expect_used,
        reason = "structural admission bounds input below u32 before the root unit is constructed"
    )]
    pub fn root(node: Node<'_>, content: &str) -> Self {
        let language = node.language();
        let name = crate::signatures::languages::all_entries()
            .iter()
            .find(|entry| entry.language == *language)
            .map_or("unregistered-tree-sitter", |entry| entry.name);
        let templates = SourceTemplates::new(
            content,
            Producer::new(format!("tree-sitter-structural:{name}"), "1"),
            AnalysisOperation::StructuralSearch,
        )
        .expect("structural input is bounded before parsing");
        Self {
            templates: Arc::new(templates),
            trees: HashMap::from([(tree_id(node), UnitHandle(0))]),
        }
    }
    pub fn region(
        &self,
        root: Node<'_>,
        range: tree_sitter::Range,
        tail: &str,
    ) -> Result<Self, ExecutionError> {
        let fail = |error: crate::text::analysis::SourceError| {
            ExecutionError::limit("structural.source.invalid", "source", error.to_string())
        };
        let mut templates = (*self.templates).clone();
        let adapter = Producer::new(
            if tail.is_empty() {
                "rust-macro-body"
            } else {
                "c-macro-body"
            },
            "1",
        );
        let representation = templates
            .add_representation(
                adapter.clone(),
                RepresentationContent::Prefix {
                    input: RepresentationHandle(0),
                    end_byte: range.end_byte as u32,
                    tail: tail.to_owned(),
                },
            )
            .map_err(fail)?;
        let unit = templates
            .add_unit(
                adapter,
                AnalysisOperation::StructuralSearch,
                representation,
                vec![ByteRange::new(
                    range.start_byte as u32,
                    (range.end_byte + tail.len()) as u32,
                )],
            )
            .map_err(fail)?;
        let mut trees = self.trees.clone();
        trees.insert(tree_id(root), unit);
        Ok(Self {
            templates: Arc::new(templates),
            trees,
        })
    }
    pub fn span(&self, node: Node<'_>) -> ObservedSpan {
        self.range(
            tree_id(node),
            node.start_byte() as u32,
            node.end_byte() as u32,
        )
    }
    #[expect(
        clippy::expect_used,
        reason = "every capture tree is registered on entering its active parser frame"
    )]
    pub fn range(&self, tree: usize, start: u32, end: u32) -> ObservedSpan {
        ObservedSpan {
            unit: *self
                .trees
                .get(&tree)
                .expect("capture belongs to an active parser tree"),
            range: ByteRange::new(start, end),
        }
    }
}
pub(super) fn tree_id(node: Node<'_>) -> usize {
    // Tree-sitter exposes this identity safely; it is local to the active walk, never persisted.
    node.into_raw().tree as usize
}
