//! Tree-sitter node-kind vocabulary shared across grammars: comments,
//! string literals and annotations. The extractor, the structural matchers
//! and the LSP resolver read these one definitions.

/// A comment node kind (`comment`, `line_comment`, `block_comment`, …).
pub(crate) fn is_comment_kind(kind: &str) -> bool {
    kind.contains("comment")
}

/// A string or character literal node kind (`string`, `string_literal`,
/// `raw_string_literal`, `template_string`, `char_literal`, …).
pub(crate) fn is_string_kind(kind: &str) -> bool {
    kind.contains("string") || kind.contains("char_literal") || kind == "string_content"
}

/// Grammar nodes that annotate the item after them: Rust attributes,
/// decorators, Java and Scala annotations, C# and C++ attribute lists.
pub(crate) fn is_annotation_kind(kind: &str) -> bool {
    matches!(
        kind,
        "attribute_item"
            | "inner_attribute_item"
            | "decorator"
            | "annotation"
            | "marker_annotation"
            | "attribute_list"
            | "attribute_declaration"
    )
}

/// Every node kind name of each grammar, by kind id, built once per grammar.
/// `Node::kind` reads the name through `ts_node_type`, a `strlen` and a UTF-8
/// check on every call; a node walk asks it several times per node, so the
/// walk reads this table by [`Node::kind_id`] instead.
static KIND_TABLES: std::sync::Mutex<Vec<(tree_sitter::Language, &'static [&'static str])>> =
    std::sync::Mutex::new(Vec::new());

thread_local! {
    /// The grammar read last on this thread: a walk stays in one grammar.
    static LAST_TABLE: std::cell::RefCell<Option<(tree_sitter::Language, &'static [&'static str])>> =
        const { std::cell::RefCell::new(None) };
}

/// The name table of `language`. The names leak once per grammar, a bounded
/// set, so the table can be shared as `'static`.
fn kind_table(language: &tree_sitter::Language) -> &'static [&'static str] {
    let mut tables = KIND_TABLES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((_, table)) = tables.iter().find(|(held, _)| held == language) {
        return table;
    }
    let names: Vec<&'static str> = (0..language.node_kind_count())
        .map(|id| {
            let name = u16::try_from(id)
                .ok()
                .and_then(|id| language.node_kind_for_id(id))
                .unwrap_or_default();
            &*Box::leak(name.to_owned().into_boxed_str())
        })
        .collect();
    let table: &'static [&'static str] = Box::leak(names.into_boxed_slice());
    tables.push((language.clone(), table));
    table
}

/// `node.kind()`, read from a per-grammar table: the same string for every
/// node (`kind_names_equal_node_kind` pins it across the grammars).
pub(crate) fn kind_of<'t>(node: impl std::borrow::Borrow<tree_sitter::Node<'t>>) -> &'t str {
    let node = *node.borrow();
    let language = node.language();
    let table = LAST_TABLE.with(|last| {
        let mut last = last.borrow_mut();
        match &*last {
            Some((held, table)) if *held == *language => *table,
            _ => {
                let table = kind_table(&language);
                *last = Some((tree_sitter::Language::clone(&language), table));
                table
            }
        }
    });
    table
        .get(usize::from(node.kind_id()))
        .copied()
        .unwrap_or_else(|| node.kind())
}

#[cfg(test)]
mod tests;
