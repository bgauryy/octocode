use super::*;

fn extract_graph_facts_with_extension(
    content: &str,
    file_path: &str,
    extension: &str,
) -> Option<String> {
    graph_facts::extract_graph_facts_with_metadata_with_extension(content, file_path, extension)
        .and_then(|extraction| serde_json::to_string(&extraction.facts).ok())
}

fn extract(content: &str, path: &str) -> Option<String> {
    extract_signatures_inner(content, path).ok()
}

mod facts;
mod flow_js;
mod outline;
mod ranges;
