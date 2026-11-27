use super::*;

fn extract_graph_facts_with_extension(
    content: &str,
    file_path: &str,
    extension: &str,
) -> Option<String> {
    graph_facts::extract_graph_facts_with_metadata_with_extension(content, file_path, extension)
        .and_then(|extraction| serde_json::to_string(&extraction.facts).ok())
}

thread_local! {
    /// Rust attributes folded on this thread (`graph_facts::rust`): each
    /// should fold once, which the work-bound test checks.
    pub(super) static FOLDED_ATTRIBUTES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The outline of `content` read from its own declarations document.
pub(super) fn extract_signatures_inner(
    content: &str,
    file_path: &str,
) -> Result<String, OutlineUnavailable> {
    declarations_outline(
        content,
        file_path,
        super::extract_declarations_document(content, file_path).as_ref(),
    )
}

fn extract(content: &str, path: &str) -> Option<String> {
    extract_signatures_inner(content, path).ok()
}

mod facts;
mod flow_js;
mod outline;
mod owners;
mod ranges;
